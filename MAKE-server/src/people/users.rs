use std::collections::HashMap;
use std::time::SystemTime;
use log::warn;
use serde::{Deserialize, Serialize};

use crate::inventory::checkout::*;

use crate::machines::printers::PrintQueueEntry;
use crate::people::quizzes::*;

#[derive(Default, Debug, Deserialize, Serialize, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum AuthLevel {
    Banned,
    #[default] User,
    Steward,
    Admin,
    Faculty,
    System,
}

#[derive(Default, Deserialize, Serialize, Clone)]
#[serde(default)]
pub struct Users {
    users: HashMap<u64, User>,
}

impl Users {
    pub fn has_user(&self, user: &User) -> bool {
        self.users.contains_key(&user.college_id)
    }

    pub fn get_user(&self, user: &User) -> Option<User> {
        self.users.get(&user.college_id).cloned()
    }

    pub fn add_set_user(&mut self, user: User) {
        self.users.insert(user.college_id, user);
    }

    pub fn get_user_by_id(&self, id_number: &u64) -> Option<User> {
        self.users.get(id_number).cloned()
    }

    pub fn len(&self) -> usize {
        self.users.len()
    }

    pub fn update_from(&mut self, other: &Users) {
        // If the user doesn't exist, add it
        for (id_number, user) in other.users.iter() {
            if !self.users.contains_key(id_number) {
                self.users.insert(*id_number, user.clone());
            } else {
                // If the user exists, update passed quizzes, but don't delete any quizzes

                let mut current_user = self.users.get(id_number).unwrap().clone();

                current_user.update_soft_from(user);

                self.users.insert(*id_number, current_user.clone());
            }
        }
    }

    pub fn exists(&self, id_number: &u64) -> bool {
        self.users.contains_key(id_number)
    }

    pub fn get_user_by_email(&self, email: &str) -> Option<User> {
        for (_, user) in self.users.iter() {
            if user.college_email == email {
                return Some(user.clone());
            }
        }

        None
    }
}

#[derive(Default, Deserialize, Serialize, Clone)]
pub struct User {
    name: String,
    college_id: u64,
    college_email: String,
    passed_quizzes: Vec<QuizName>,
    auth_level: AuthLevel,
}

impl User {
    pub fn from_response(response: &Response) -> Self {
        User {
            name: response.name.clone(),
            college_id: response.college_id,
            college_email: response.college_email.clone(),
            passed_quizzes: vec![],
            auth_level: AuthLevel::User,
        }
    }

    pub fn log_quiz(&mut self, quiz_name: QuizName, passed: bool) {
        if passed {
            self.passed_quizzes.push(quiz_name)
        };
    }

    pub fn get_id(&self) -> u64 {
        self.college_id
    }

    pub fn get_email(&self) -> String {
        self.college_email.to_string()
    }

    pub fn get_name(&self) -> String {
        self.name.to_string()
    }

    pub fn get_passed_quizzes(&self) -> Vec<QuizName> {
        self.passed_quizzes.clone()
    }

    pub fn get_pending_checked_out_items(
        &self,
        checkout_log: &CheckoutLog,
    ) -> Vec<CheckoutLogEntry> {
        checkout_log
            .currently_checked_out
            .iter()
            .filter(|x| x.college_id == self.get_id())
            .cloned()
            .collect()
    }

    pub fn get_all_checked_out_items(&self, checkout_log: &CheckoutLog) -> Vec<CheckoutLogEntry> {
        checkout_log
            .currently_checked_out
            .iter()
            .chain(checkout_log.checkout_history.iter())
            .filter(|x| x.college_id == self.get_id())
            .cloned()
            .collect()
    }

    pub fn update_soft_from(&mut self, other: &User) {
        // Take union of passed quizzes
        self.passed_quizzes = self
            .passed_quizzes
            .iter()
            .chain(other.passed_quizzes.iter())
            .cloned()
            .collect();

        // Use longest name
        if self.name.len() < other.name.len() {
            self.name = other.name.clone();
        }

        // Remove duplicates
        self.passed_quizzes.sort();
        self.passed_quizzes.dedup();
    }

    pub fn get_auth_level(&self) -> AuthLevel {
        self.auth_level.clone()
    }

    pub fn set_auth_level(&mut self, auth_level: AuthLevel) {
        self.auth_level = auth_level;
    }

    pub fn set_quiz_passed(&mut self, quiz_name: &QuizName, passed: bool) {
        if passed {
            self.passed_quizzes.push(quiz_name.clone());
        } else {
            self.passed_quizzes.retain(|x| x != quiz_name);
        }
    }

    pub fn create_print_queue_entry(&self) -> PrintQueueEntry {
        PrintQueueEntry {
            uuid: uuid::Uuid::new_v4().to_string(),
            college_id: self.college_id,
            email: self.college_email.clone(),
            timestamp_submitted: SystemTime::now()
                .duration_since(SystemTime::UNIX_EPOCH)
                .expect("Time went backwards")
                .as_secs(),
            timestamp_notified: None,
            timestamp_accepted: None,
        }
    }
}

pub fn create_users_from_quizzes(quizzes: &Vec<Quiz>) -> Users {
    let mut users = Users::default();

    // Keep track of emails and the relevent ID numbers
    // The problem is that ID numbers are often typed out wrong,
    // and sometimes the incorrect ID is the still a valid one
    // so we need to match the correct ID number to the correct email
    // and combine duplicates
    let mut email_key: HashMap<String, Vec<u64>> = HashMap::new();

    for quiz in quizzes {
        for response in quiz.get_responses() {
            let email = response.college_email.to_lowercase();

            if !email_key.contains_key(&email) {
                email_key.insert(email.clone(), vec![response.college_id]);
            } else {
                email_key.get_mut(&email).unwrap().push(response.college_id);
            }

            let mut user = User::from_response(response);

            if users.has_user(&user) {
                user = users.get_user(&user).unwrap().clone();
            }

            user.log_quiz(quiz.get_name().clone(), response.passed);

            users.add_set_user(user);
        }
    }

    let mut final_users = Users::default();

    // After, get users with the same email
    for (_, id_numbers) in email_key.iter() {
        // Get most common ID number
        let most_common_id_number = id_numbers
            .iter()
            .cloned()
            .fold(HashMap::new(), |mut acc, x| {
                *acc.entry(x).or_insert(0) += 1;
                acc
            })
            .into_iter()
            .max_by_key(|x| x.1)
            .unwrap()
            .0;

        
        let mut user = users.get_user_by_id(&most_common_id_number).unwrap().clone();

        for id_number in id_numbers.iter() {
            if id_number != &most_common_id_number {
                let other_user = users.get_user_by_id(id_number).unwrap().clone();

                user.update_soft_from(&other_user);
            }
        }

        if most_common_id_number >= 100_000_000 || most_common_id_number <= 10_000_000 {
            warn!("ID number {} for {} is not 8 digits", most_common_id_number, user.get_name());
        }

        final_users.add_set_user(user);
    }


    final_users
}