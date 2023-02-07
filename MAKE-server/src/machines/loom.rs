use std::{path::Path, fs::File, io::Write, cmp::{max, min}};

use image::{self, Pixel};
use log::info;
use base64::{Engine as _, prelude::BASE64_STANDARD_NO_PAD};

pub fn render_loom_request(b64_file: &str, file_extension: &str, loom_width: &u32, width: &u32, inner_tabby_width: &usize, outer_tabby_width: &usize, format: &str) -> String {
    let start = std::time::Instant::now();
    // Open file from base64
    let file_path = format!("temp.{}", file_extension);
    let file_path = Path::new(&file_path);
    let mut file = File::create(file_path).unwrap();
    file.write_all(&BASE64_STANDARD_NO_PAD.decode(b64_file).unwrap()).unwrap();

    // Open image file
    let pic_img = image::io::Reader::open(file_path).unwrap();
    let pic_img = pic_img.with_guessed_format().unwrap().decode().unwrap();
    let duration = start.elapsed();

    // Delete temp file
    std::fs::remove_file(file_path).unwrap();
    info!("Opening image took: {:?}", duration);

    let start = std::time::Instant::now();

    let new_width = min(width.clone(), loom_width.clone());
    let new_height = (pic_img.height() as f32 / pic_img.width() as f32 * new_width as f32) as u32;

    let mut img = image::RgbaImage::new(loom_width.clone(), new_height);
    
    // Resize to loom width, keeping aspect ratio
    let pic_img = pic_img.resize(new_width, new_height, image::imageops::FilterType::Nearest);

    // Put image in top left corner of blank image
    image::imageops::overlay(&mut img, &pic_img, 0, 0);

    // Get image pixels grayscale
    let height = img.height();
    let width = img.width();
    let mut img_pixels = img.pixels().map(|p| p.to_luma().channels()[0]).collect::<Vec<u8>>();
    let duration = start.elapsed();
    info!("Resizing image took: {:?}", duration);

    let start = std::time::Instant::now();
    // Normalize image pixels
    let max = &img_pixels.iter().max().unwrap().clone();
    let min = &img_pixels.iter().min().unwrap().clone();
    let range = max - min;

    if range == 0 {
        return String::from("Image is all white");
    }

    let duration = start.elapsed();
    info!("Normalizing took: {:?}", duration);

    for i in 0..img_pixels.len() {
        img_pixels[i] = (((img_pixels[i] - min) as u32 * 255) / range as u32) as u8;
    }

    // Find start and end column of image
    // eg columns where all pixels are white
    let start = std::time::Instant::now();
    let (start_column, end_column) = find_start_end_column(&img_pixels, width, height);
    let duration = start.elapsed();
    info!("Finding start and end column took: {:?}", duration);

    // Dither
    let start = std::time::Instant::now();
    atkinson_dither(&mut img_pixels, width, height);
    let duration = start.elapsed();
    info!("Dithering took: {:?}", duration);
    
    // Apply loom "filter"
    let start = std::time::Instant::now();
    loom_filter(&mut img_pixels, width, height);
    let duration = start.elapsed();
    info!("Loom filter took: {:?}", duration);

    // Apply loom tabby
    let start = std::time::Instant::now();
    loom_tabby(&mut img_pixels, width, height, start_column, end_column, *inner_tabby_width);
    loom_tabby(&mut img_pixels, width, height, *outer_tabby_width as u32, width - *outer_tabby_width as u32, *outer_tabby_width);
    let duration = start.elapsed();
    info!("Loom tabby took: {:?}", duration);

    
    // Convert image to b64 string
    let start = std::time::Instant::now();
    // Encode as tiff
    let img = image::ImageBuffer::from_vec(width, height, img_pixels).unwrap();
    let img = image::DynamicImage::ImageLuma8(img);

    // Temp file path of out.tiff
    let file_path = format!("out.{}", &format);
    let file_path = Path::new(&file_path);
    let mut file = File::create(file_path).unwrap();

    let image_format;

    if format == "tiff" {
        image_format = image::ImageOutputFormat::Tiff;
    } else if format == "png" {
        image_format = image::ImageOutputFormat::Png;
    } else {
        image_format = image::ImageOutputFormat::Jpeg(100);
    }

    img.write_to(&mut file, image_format).unwrap();

    // Encode as base64
    let img_b64 = &BASE64_STANDARD_NO_PAD.encode(&std::fs::read(file_path).unwrap());
    let duration = start.elapsed();
    info!("Converting to b64 took: {:?}", duration);

    img_b64.to_owned()
}


pub fn find_start_end_column(img_pixels: &Vec<u8>, width: u32, height: u32) -> (u32, u32) {
    let mut start_column: Option<u32> = None;
    let mut end_column: Option<u32> = None;
    let mut column = 0;

    while column < width {
        let mut column_white = 10;

        for row in 0..height {
            if img_pixels[(row * width + column) as usize] != 255 {
                column_white -= 1;
                if column_white == 0 {
                    break;
                }
            }
        }

        column += 1;
        
        if column_white == 0 {
            start_column = Some(column - 1);
            break;
        }
    }

    column = width - 1;

    while column > 0 {
        let mut column_white = 10;

        for row in 0..height {
            if img_pixels[(row * width + column) as usize] != 255 {
                column_white -= 1;
                if column_white == 0 {
                    break;
                }
            }
        }

        if column_white == 0 {
            end_column = Some(column + 1);
            break;
        }

        column -= 1;
    }

    (start_column.unwrap_or(0), end_column.unwrap_or(width - 1))
}

pub fn loom_filter(img: &mut Vec<u8>, width: u32, height: u32) {
    // Can't have more then 3 black pixels in a row without a white pixel
    // Can't have more then 3 white pixels in a column without a black pixel
    // Iterate and fix pixels

    // Horizontal
    for y in 0..height {
        let mut black_count = 0;

        for x in 0..width {
            let index = (y * width + x) as usize;

            if img[index] == 0 {
                black_count += 1;
            } else {
                black_count = 0;
            }

            if black_count == 5 {
                img[index] = 255;
                black_count = 0;
            }
        }
    }

    // Vertical
    for x in 0..width {
        let mut white_count = 0;

        for y in 0..height {
            let index = (y * width + x) as usize;

            if img[index] == 0 {
                white_count = 0;
            } else {
                white_count += 1;
            }

            if white_count == 5 {
                img[index] = 0;
                white_count = 0;
            }
        }
    }
}

pub fn loom_tabby(img: &mut Vec<u8>, width: u32, height: u32, start_column: u32, end_column: u32, tabby_width: usize) {
    // Add tabby pattern to image
    // for width 5, it's 
    //  bwbwb
    //  wbwbw

    // Add to start and end columns, and to the beginning and end of the image
    // eg if start_column is 100, add 5 pixels to the left of it
    // eg if end_column is 100, add 5 pixels to the right of it

    // If start_column is < tabby_width, add to the beginning of the image
    // If end_column is > width - tabby_width, add to the end of the image

    let start_column = max(start_column, tabby_width as u32);
    let end_column = min(end_column, width - tabby_width as u32);

    let mut top_tabby = Vec::new();
    let mut bottom_tabby = Vec::new();

    for i in 0..tabby_width {
        if i % 2 == 0 {
            top_tabby.push(0);
            bottom_tabby.push(255);
        } else {
            top_tabby.push(255);
            bottom_tabby.push(0);
        }
    }

    // Add to start and end columns
    for y in 0..height {
        let index = (y * width + start_column - tabby_width as u32) as usize;
        if y % 2 == 0 {
            img.splice(index..index + tabby_width, top_tabby.clone());
        } else {
            img.splice(index..index + tabby_width, bottom_tabby.clone());
        }

        let index = (y * width + end_column) as usize;
        if y % 2 == 0 {
            img.splice(index..index + tabby_width, top_tabby.clone());
        } else {
            img.splice(index..index + tabby_width, bottom_tabby.clone());
        }
    }
}

pub fn ordered_4x4_dither(img: &mut Vec<u8>, width: u32, height: u32) {
    let mut dither_matrix = vec![0; 16];
    dither_matrix[0] = 0;
    dither_matrix[1] = 8;
    dither_matrix[2] = 2;
    dither_matrix[3] = 10;
    dither_matrix[4] = 12;
    dither_matrix[5] = 4;
    dither_matrix[6] = 14;
    dither_matrix[7] = 6;
    dither_matrix[8] = 3;
    dither_matrix[9] = 11;
    dither_matrix[10] = 1;
    dither_matrix[11] = 9;
    dither_matrix[12] = 15;
    dither_matrix[13] = 7;
    dither_matrix[14] = 13;
    dither_matrix[15] = 5;

    for i in 0..width as usize * height as usize {
        let x = i % width as usize;
        let y = i / width as usize;
        let dither_value = dither_matrix[(x % 4) + (y % 4) * 4];
        if img[i] > dither_value {
            img[i] = 255;
        } else {
            img[i] = 0;
        }
    }
    
}

pub fn atkinson_dither(img: &mut Vec<u8>, width: u32, height: u32) {
    for i in 0..width as isize * height as isize {
        let x = i % width as isize;
        let y = i / width as isize;
        let old_pixel = img[i as usize];
        let new_pixel = if old_pixel < 128 { 0 } else { 255 };
        img[i as usize] = new_pixel;
        let quant_error = old_pixel as i32 - new_pixel as i32;

        let index = (y * width as isize) + x + 1;
        if index < width as isize * height as isize {
            img[index as usize] = (img[index as usize] as i32 + quant_error * 7 / 16) as u8;
        }

        let index = (y * width as isize) + x - 1 + width as isize;
        if index < width as isize * height as isize {
            img[index as usize] = (img[index as usize] as i32 + quant_error * 3 / 16) as u8;
        }

        let index = (y * width as isize) + x + width as isize;
        if index < width as isize * height as isize {
            img[index as usize] = (img[index as usize] as i32 + quant_error * 5 / 16) as u8;
        }

        let index = (y * width as isize) + x + 1 + width as isize;
        if index < width as isize * height as isize {
            img[index as usize] = (img[index as usize] as i32 + quant_error * 1 / 16) as u8;
        }
    }
}

pub fn floyd_steinberg_dither(img: &mut Vec<u8>, width: u32, height: u32) {
    for i in 0..width as usize * height as usize {
        let x = i % width as usize;
        let y = i / width as usize;
        let old_pixel = img[i];
        let new_pixel = if old_pixel < 128 { 0 } else { 255 };
        img[i] = new_pixel;
        let quant_error = old_pixel as i32 - new_pixel as i32;

        let index = (y * width as usize) + x + 1;
        if index < width as usize * height as usize {
            img[index] = (img[index] as i32 + quant_error * 7 / 16) as u8;
        }

        let index = (y * width as usize) + x - 1 + width as usize;
        if index < width as usize * height as usize {
            img[index] = (img[index] as i32 + quant_error * 3 / 16) as u8;
        }

        let index = (y * width as usize) + x + width as usize;
        if index < width as usize * height as usize {
            img[index] = (img[index] as i32 + quant_error * 5 / 16) as u8;
        }

        let index = (y * width as usize) + x + 1 + width as usize;
        if index < width as usize * height as usize {
            img[index] = (img[index] as i32 + quant_error * 1 / 16) as u8;
        }
    }
}