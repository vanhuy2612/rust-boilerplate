use image::{DynamicImage, ImageBuffer, Rgb, codecs::jpeg::JpegEncoder, imageops::FilterType};
use std::{fs, io::Cursor, path::Path};

const SOURCE_IMAGE_PATH: &str = "./public/images.jpeg";

pub async fn render_resized_jpeg() -> Result<Vec<u8>, String> {
    tokio::task::spawn_blocking(render_resized_jpeg_blocking)
        .await
        .map_err(|error| error.to_string())?
        .map_err(|error| error.to_string())
}

fn render_resized_jpeg_blocking() -> Result<Vec<u8>, image::ImageError> {
    let image = load_source_image()?;
    let resized = image.resize_exact(500, 500, FilterType::Lanczos3);
    let mut buffer = Cursor::new(Vec::new());
    let mut encoder = JpegEncoder::new_with_quality(&mut buffer, 90);
    encoder.encode_image(&resized)?;
    Ok(buffer.into_inner())
}

fn load_source_image() -> Result<DynamicImage, image::ImageError> {
    let path = Path::new(SOURCE_IMAGE_PATH);

    if path.exists() {
        return image::open(path);
    }

    if let Some(parent) = path.parent() {
        let _ = fs::create_dir_all(parent);
    }

    Ok(generate_fallback_image())
}

fn generate_fallback_image() -> DynamicImage {
    let width = 1200;
    let height = 1200;
    let buffer = ImageBuffer::from_fn(width, height, |x, y| {
        let red = ((x * 255) / width) as u8;
        let green = ((y * 255) / height) as u8;
        let blue = (((x + y) * 255) / (width + height)) as u8;
        Rgb([red, green, blue])
    });

    DynamicImage::ImageRgb8(buffer)
}
