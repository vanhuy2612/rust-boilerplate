use image::{DynamicImage, ImageBuffer, Rgb, codecs::jpeg::JpegEncoder, imageops::FilterType};
use std::{io::Cursor, path::Path, sync::Arc};

use crate::service::cpu_worker_pool::CpuWorkerPool;

const SOURCE_IMAGE_PATH: &str = "./public/images.jpeg";
const OUTPUT_WIDTH: u32 = 500;
const OUTPUT_HEIGHT: u32 = 500;

/// Resize the (already decoded) source image on the CPU worker pool and encode it as JPEG.
pub async fn render_resized_jpeg(
    cpu_worker_pool: &CpuWorkerPool,
    source_image: Arc<DynamicImage>,
) -> Result<Vec<u8>, String> {
    cpu_worker_pool
        .execute(move || render_resized_jpeg_blocking(&source_image))
        .await
        .map_err(|error| error.to_string())?
        .map_err(|error| error.to_string())
}

fn render_resized_jpeg_blocking(image: &DynamicImage) -> Result<Vec<u8>, image::ImageError> {
    let resized = image.resize_exact(OUTPUT_WIDTH, OUTPUT_HEIGHT, FilterType::Lanczos3);
    let mut buffer = Cursor::new(Vec::new());
    let mut encoder = JpegEncoder::new_with_quality(&mut buffer, 90);
    encoder.encode_image(&resized)?;
    Ok(buffer.into_inner())
}

/// Decode the source image once at startup. Falls back to a generated image
/// (also created only once) when the file is missing or unreadable.
pub fn load_source_image() -> Arc<DynamicImage> {
    let path = Path::new(SOURCE_IMAGE_PATH);

    match image::open(path) {
        Ok(image) => {
            tracing::info!(
                path = SOURCE_IMAGE_PATH,
                width = image.width(),
                height = image.height(),
                "loaded source image"
            );
            Arc::new(image)
        }
        Err(error) => {
            tracing::warn!(
                path = SOURCE_IMAGE_PATH,
                %error,
                "source image not available, using generated fallback image"
            );
            Arc::new(generate_fallback_image())
        }
    }
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
