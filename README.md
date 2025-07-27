# Regen

Regen is a Rust CLI tool to generate high-resolution Mandelbrot fractal images with randomized parameters, add compression-busting noise, optionally preview them, and finally upload to DigitalOcean Spaces. Also exports a CSV of CDN-accessible URLs.

## Features

1. Generate Mandelbrot fractal images with randomized parameters
2. Ensure image complexity using fractal pixel ratio
3. Append random noise to increase file size (for compression/benchmark testing)
4. Preview generated images (macOS/Linux)
5. Upload images to DigitalOcean Spaces (S3-compatible)
6. Save generated CDN + origin URLs in a CSV

##  Setup

Make sure you have: Rust + Cargo installed and AWS-style credentials exported:

```sh
export AWS_ACCESS_KEY_ID=your_do_access_key
export AWS_SECRET_ACCESS_KEY=your_do_secret_key
```

## Build

```bash
cargo build --release
```

## Commands

### Generate Images

```sh
./target/release/regen generate -c 5 --preview
```

- `-c`, `--count` → Number of images
- `--preview` → Open image using system viewer

### Upload Images

```sh
./target/release/regen upload
```

You must configure the bucket, region, and path inside the upload() function.

## Output

Images saved to: `src/data/images/`. URLs written to: src/data/urls.csv (columns: cdn_url, origin_url, file_name, file_size_kib)
