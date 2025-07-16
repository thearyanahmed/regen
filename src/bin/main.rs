use clap::Parser as ClapParser;
use csv::ReaderBuilder;
use csv::WriterBuilder;
use futures::future::try_join_all;
use image::{ImageBuffer, Rgb, RgbImage};
use rand::Rng;
use rusoto_core::Region;
use rusoto_s3::{PutObjectRequest, S3, S3Client};
use std::fs;
use std::fs::OpenOptions;
use std::io::Write;
use std::io::{Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::process::Command;
use walkdir::WalkDir;

// For concurrent uploads
#[allow(clippy::too_many_arguments)] // This function signature is intentionally long for demonstration
pub fn generate_mathematical_image(
    width: u32,
    height: u32,
    pattern_type: &str,
    filename: &str,
    mandelbrot_params: Option<(f64, f64, f64, u32, u32, f64)>,
) -> Result<PathBuf, Box<dyn std::error::Error + Send + Sync>> {
    let mut img: RgbImage = ImageBuffer::new(width, height);
    let mut rng = rand::thread_rng();

    // Default to white background for all images
    for x in 0..width {
        for y in 0..height {
            img.put_pixel(x, y, Rgb([255, 255, 255]));
        }
    }

    match pattern_type {
        "mandelbrot" => {
            // Default Mandelbrot parameters, can be overridden by `mandelbrot_params`
            let (x_pos, y_pos, escape_radius, max_iterations, smoothness, color_step) =
                mandelbrot_params.unwrap_or((-0.00275, 0.78912, 0.125689, 800, 8, 6000.0));

            // Calculate the view window based on x_pos, y_pos, and escape_radius
            // The escape_radius in GoBrot seems to relate to the zoom level.
            // We'll map it to a view width/height. A smaller radius means more zoom.
            let view_width = 4.0 * escape_radius;
            let view_height = view_width * (height as f64 / width as f64);

            let x_min = x_pos - view_width / 2.0;
            let x_max = x_pos + view_width / 2.0;
            let y_min = y_pos - view_height / 2.0;
            let y_max = y_pos + view_height / 2.0;

            for x in 0..width {
                for y in 0..height {
                    let c_real = x_min + (x as f64 / width as f64) * (x_max - x_min);
                    let c_imag = y_min + (y as f64 / height as f64) * (y_max - y_min);

                    let mut z_real = 0.0;
                    let mut z_imag = 0.0;

                    let mut iterations = 0;
                    let mut magnitude_sq = 0.0;

                    while magnitude_sq < 4.0 && iterations < max_iterations {
                        let next_z_real = z_real * z_real - z_imag * z_imag + c_real;
                        z_imag = 2.0 * z_real * z_imag + c_imag;
                        z_real = next_z_real;
                        magnitude_sq = z_real * z_real + z_imag * z_imag;
                        iterations += 1;
                    }

                    if iterations == max_iterations {
                        // Point is in the set (black)
                        img.put_pixel(x, y, Rgb([0, 0, 0]));
                    } else {
                        // Point escaped, color based on iteration count with smoothing
                        // GoBrot's smoothing and step suggest a continuous coloring
                        // We'll use a logarithmic smoothing for better visual results
                        let log_zn = magnitude_sq.ln() / 2.0;
                        let nu = (log_zn / 2.0_f64.ln()).ln() / 2.0_f64.ln();
                        let smoothed_iterations = iterations as f64 + 1.0 - nu;

                        // Map smoothed_iterations to a color intensity (black to white gradient for background)
                        // The 'step' parameter in GoBrot affects how rapidly colors change.
                        // Here, we'll use it to scale the intensity for the white background.
                        let color_val = (smoothed_iterations / color_step) * 255.0;
                        let _intensity = (color_val.min(255.0)) as u8;

                        // Since you requested white background and black foreground,
                        // we'll make the escaped points white, with a subtle gradient if smoothness is applied.
                        // For simplicity, if `smoothness` is 0 (or very low), it's just white.
                        // If `smoothness` is high, it will still be white, but the calculation is there
                        // for future palette integration.
                        // For now, if it escapes, it's white.
                        if smoothness == 0 {
                            img.put_pixel(x, y, Rgb([255, 255, 255]));
                        } else {
                            // A subtle gradient in white if smoothness is applied
                            // This might not be visible with pure black/white, but sets up for palettes.
                            // For now, let's just make it white for escaped points.
                            img.put_pixel(x, y, Rgb([255, 255, 255]));
                        }
                    }
                }
            }
        }
        _ => {
            // Default to random noise if pattern_type is not recognized
            for x in 0..width {
                for y in 0..height {
                    let r_val = rng.r#gen();
                    let g_val = rng.r#gen();
                    let b_val = rng.r#gen();
                    img.put_pixel(x, y, Rgb([r_val, g_val, b_val]));
                }
            }
        }
    }
    let temp_dir = PathBuf::from("src/data/images");
    std::fs::create_dir_all(&temp_dir)?; // Ensure the directory exists
    let temp_path = temp_dir.join(filename);

    img.save(&temp_path)?;

    Ok(temp_path)
}

/// Opens the given image file using the system's default image viewer.
/// This function is OS-dependent.
pub fn preview_image(image_path: &PathBuf) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let path_str = image_path.to_str().ok_or("Invalid path for preview")?;

    #[cfg(target_os = "macos")]
    {
        Command::new("open").arg(path_str).spawn()?;
    }
    #[cfg(target_os = "linux")]
    {
        Command::new("xdg-open").arg(path_str).spawn()?;
    }

    println!("Previewing image at: {}", image_path.display());
    Ok(())
}

// Main function for testing purposes

#[derive(clap::Parser)]
#[clap(name = "FractalGen")]
#[clap(about = "Generate and upload fractal images", long_about = None)]
struct Cli {
    #[clap(subcommand)]
    command: Commands,
}

#[derive(clap::Subcommand)]
enum Commands {
    /// Generate N Mandelbrot images
    Generate {
        /// Number of images to generate
        #[clap(short, long)]
        count: usize,
    },
    /// Upload images to DigitalOcean Spaces
    Upload,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    match Cli::parse().command {
        Commands::Generate { count } => {
            // Arc for thread-safe RNG seeding (each task gets its own RNG)
            let tasks: Vec<_> = (0..count)
                .map(|i| {
                    tokio::spawn(async move {
                        let mut rng = rand::thread_rng();
                        let width = rng.gen_range(3000..=5000);
                        let height = rng.gen_range(2000..=3500);
                        let x_pos = rng.gen_range(-0.5..0.5);
                        let y_pos = rng.gen_range(0.6..0.9);
                        let escape_radius = rng.gen_range(0.01..0.2);
                        let max_iterations = rng.gen_range(400..1200);
                        let smoothness = rng.gen_range(1..20);
                        let color_step = rng.gen_range(1000.0..10000.0);

                        let path = generate_mathematical_image(
                            width,
                            height,
                            "mandelbrot",
                            &format!("mandelbrot_{}.png", i),
                            Some((
                                x_pos,
                                y_pos,
                                escape_radius,
                                max_iterations,
                                smoothness,
                                color_step,
                            )),
                        )?;

                        // Regenerate the image until the fractal ratio is at least 0.4
                        let mut fractal_ratio = 0.0;
                        let mut path = path;
                        let mut attempts = 0;
                        while fractal_ratio < 0.4 || fractal_ratio > 0.6 {
                            if attempts > 0 {
                                // Regenerate with new random parameters
                                let width = rng.gen_range(3000..=5000);
                                let height = rng.gen_range(2000..=3500);
                                let x_pos = rng.gen_range(-0.5..0.5);
                                let y_pos = rng.gen_range(0.6..0.9);
                                let escape_radius = rng.gen_range(0.01..0.2);
                                let max_iterations = rng.gen_range(400..1200);
                                let smoothness = rng.gen_range(1..20);
                                let color_step = rng.gen_range(1000.0..10000.0);
                                path = generate_mathematical_image(
                                    width,
                                    height,
                                    "mandelbrot",
                                    &format!("mandelbrot_{}.png", i),
                                    Some((
                                        x_pos,
                                        y_pos,
                                        escape_radius,
                                        max_iterations,
                                        smoothness,
                                        color_step,
                                    )),
                                )?;
                            }
                            // Calculate the ratio of black (fractal) pixels to total pixels
                            let img = image::open(&path)?.to_rgb8();
                            let (width, height) = img.dimensions();
                            let total_pixels = (width * height) as f64;
                            let mut black_pixels = 0u64;
                            for pixel in img.pixels() {
                                if pixel.0 == [0, 0, 0] {
                                    black_pixels += 1;
                                }
                            }
                            fractal_ratio = black_pixels as f64 / total_pixels;
                            attempts += 1;
                        }

                        // Add random noise to the image file to defeat PNG compression
                        {
                            let mut file = OpenOptions::new().read(true).write(true).open(&path)?;
                            let metadata = file.metadata()?;
                            let file_size = metadata.len();
                            let noise_bytes = rng.gen_range(1_000_000..=3_000_000);
                            let mut noise = vec![0u8; noise_bytes];
                            rng.fill(&mut noise[..]);
                            file.seek(SeekFrom::End(0))?;
                            file.write_all(&noise)?;
                            // Helper to format bytes as human-readable string
                            fn human_readable_size(bytes: u64) -> String {
                                const KB: u64 = 1024;
                                const MB: u64 = KB * 1024;
                                const GB: u64 = MB * 1024;
                                match bytes {
                                    b if b >= GB => format!("{:.2} GB", b as f64 / GB as f64),
                                    b if b >= MB => format!("{:.2} MB", b as f64 / MB as f64),
                                    b if b >= KB => format!("{:.2} KB", b as f64 / KB as f64),
                                    b => format!("{} bytes", b),
                                }
                            }

                            println!(
                                "Appended {} bytes of noise to {} (original size: {}, new size: {}), fractal ratio: {:.4}",
                                noise_bytes,
                                path.display(),
                                human_readable_size(file_size),
                                human_readable_size(file_size + noise_bytes as u64),
                                fractal_ratio
                            );
                        }

                        preview_image(&path)?;
                        Ok::<(), Box<dyn std::error::Error + Send + Sync>>(())
                    })
                })
                .collect();

            // Await all tasks and propagate errors
            try_join_all(tasks).await?;
        }
        Commands::Upload => {
            upload().await?;
        }
    }

    Ok(())
}

pub async fn upload_folder_to_do_space(
    local_folder_path: &Path,
    bucket_name: &str,
    do_region_name: &str,
    space_folder_prefix: Option<&str>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    // <-- FIX 1: Add Send + Sync
    // 1. Initialize S3 Client with DigitalOcean Endpoint
    let endpoint = format!("https://{}.digitaloceanspaces.com", do_region_name);
    let region = Region::Custom {
        endpoint,
        name: do_region_name.to_string(),
    };
    let s3_client = S3Client::new(region);

    println!("Starting upload of folder: {}", local_folder_path.display());
    println!("To Space: {} in region: {}", bucket_name, do_region_name);

    let mut upload_tasks = Vec::new();

    // 2. Traverse the local folder
    for entry in WalkDir::new(local_folder_path)
        .into_iter()
        .filter_map(|e| e.ok())
    {
        let path = entry.path().to_path_buf();
        if path.is_file() {
            // Get the relative path for the S3 key
            let relative_path = path.strip_prefix(local_folder_path)?;
            let mut s3_key_path = PathBuf::new();

            if let Some(prefix) = space_folder_prefix {
                s3_key_path.push(prefix);
            }
            s3_key_path.push(relative_path);

            let s3_key = s3_key_path.to_string_lossy().replace("\\", "/"); // Ensure forward slashes

            println!("- Preparing to upload: {} -> {}", path.display(), s3_key);

            let file_data = fs::read(&path)?;
            let client_clone = s3_client.clone();
            let bucket_name_clone = bucket_name.to_string();
            let path_clone = path.clone();

            // Create an async task for each file upload
            let task = tokio::spawn(async move {
                let mut put_request = PutObjectRequest {
                    bucket: bucket_name_clone,
                    key: s3_key.clone(),
                    body: Some(file_data.into()),
                    acl: Some("public-read".to_string()), // Make the object public
                    // Optional: Set Content-Type based on file extension
                    // content_type: Some(mime_guess::from_path(path).first_or_text_plain().to_string()),
                    ..Default::default()
                };

                if let Some(extension) = path_clone.extension().and_then(|s| s.to_str()) {
                    let mime_type = match extension.to_lowercase().as_str() {
                        "png" => "image/png",
                        "jpg" | "jpeg" => "image/jpeg",
                        "gif" => "image/gif",
                        "webp" => "image/webp",
                        // Add more image types as needed
                        _ => "application/octet-stream", // Default to download if unknown
                    };
                    put_request.content_type = Some(mime_type.to_string());
                }

                match client_clone.put_object(put_request).await {
                    Ok(_) => {
                        println!("  - Successfully uploaded: {}", s3_key);
                        Ok(())
                    }
                    Err(e) => {
                        eprintln!("  - Failed to upload {}: {:?}", s3_key, e);
                        // FIX 2: Add + Send + Sync to the dyn Error trait object within the spawned task
                        Err(Box::new(e) as Box<dyn std::error::Error + Send + Sync>)
                    }
                }
            });
            upload_tasks.push(task);
        }
    }

    // 3. Wait for all upload tasks to complete
    try_join_all(upload_tasks).await?;

    println!("Folder upload complete!");
    Ok(())
}

async fn upload() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    // Upload all files from the src/data/images folder
    let test_folder = PathBuf::from("src/data/images");
    if !test_folder.exists() {
        println!("No images to upload: src/data/images folder does not exist.");
        return Ok(());
    }

    // IMPORTANT: Replace with your actual DigitalOcean Space details
    let bucket = "benchmarkap"; // e.g., "my-app-space"
    let region = "lon1"; // e.g., "nyc3", "lon1", "fra1"
    let space_prefix = Some("fractals/"); // Optional: upload into a specific folder within the Space

    // Ensure your AWS_ACCESS_KEY_ID and AWS_SECRET_ACCESS_KEY environment variables are set.
    // For example, in your shell:
    // export AWS_ACCESS_KEY_ID="your_access_key"
    // export AWS_SECRET_ACCESS_KEY="your_secret_key"

    match upload_folder_to_do_space(&test_folder, bucket, region, space_prefix).await {
        Ok(_) => println!("\nFolder upload to DigitalOcean Spaces succeeded!"),
        Err(e) => eprintln!("\nFolder upload failed: {}", e),
    }
    // After upload, append URLs to a CSV file

    // Path to your CSV file
    let csv_path = PathBuf::from("src/data/urls.csv");
    let csv_path = csv_path.as_path();

    // Read all files in the uploaded folder
    let mut urls = Vec::new();
    for entry in WalkDir::new(&test_folder)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| e.path().is_file())
    {
        let rel_path = entry.path().strip_prefix(&test_folder)?;
        let file_name = rel_path.to_string_lossy().replace("\\", "/");
        let url = format!(
            "https://{}.{}.cdn.digitaloceanspaces.com/{}{}",
            bucket,
            region,
            space_prefix.unwrap_or(""),
            file_name
        );
        urls.push((file_name, url));
    }

    // Read existing CSV (if any)
    let mut existing_rows = Vec::new();
    if std::path::Path::new(csv_path).exists() {
        let mut rdr = ReaderBuilder::new().has_headers(true).from_path(csv_path)?;
        for result in rdr.records() {
            let record = result?;
            // Support both old (1 column), new (2 column), or extended (4 column) CSVs
            if record.len() == 4 {
                existing_rows.push((
                    record[0].to_string(),
                    record[1].to_string(),
                    record[2].to_string(),
                    record[3].to_string(),
                ));
            } else if record.len() == 2 {
                existing_rows.push((
                    record[0].to_string(),
                    record[1].to_string(),
                    String::new(),
                    String::new(),
                ));
            } else if record.len() == 1 {
                existing_rows.push((
                    record[0].to_string(),
                    String::new(),
                    String::new(),
                    String::new(),
                ));
            }
        }
    }

    // Append new URLs, avoiding duplicates
    for (file, _cdn_url) in &urls {
        let origin_url = format!(
            "https://{}.{}.digitaloceanspaces.com/{}{}",
            bucket,
            region,
            space_prefix.unwrap_or(""),
            file
        );
        let cdn_url = format!(
            "https://{}.{}.cdn.digitaloceanspaces.com/{}{}",
            bucket,
            region,
            space_prefix.unwrap_or(""),
            file
        );
        // File name
        let file_name = Path::new(file)
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or(file);

        // File size in KiB
        let file_path = test_folder.join(file);
        let file_size_kib = match fs::metadata(&file_path) {
            Ok(meta) => format!("{:.2}", meta.len() as f64 / 1024.0),
            Err(_) => String::from(""),
        };

        if !existing_rows.iter().any(|(f, _, _, _)| f == file) {
            existing_rows.push((cdn_url, origin_url, file_name.to_string(), file_size_kib));
        }
    }

    // Write back to CSV (cdn_url, origin_url columns)
    if let Some(parent) = csv_path.parent() {
        fs::create_dir_all(parent)?;
    }
    let mut wtr = WriterBuilder::new().has_headers(true).from_path(csv_path)?;
    wtr.write_record(&["cdn_url", "origin_url", "file_name", "file_size_kib"])?;
    for (cdn_url, origin_url, file_name, file_size_kib) in existing_rows {
        wtr.write_record(&[cdn_url, origin_url, file_name, file_size_kib])?;
    }
    wtr.flush()?;

    Ok(())
}
