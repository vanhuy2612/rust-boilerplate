use axum::{
    body::Body,
    http::{header, HeaderValue, Response, StatusCode},
    response::IntoResponse,
    routing::get,
    Router,
};
use image::{
    codecs::jpeg::JpegEncoder,
    imageops::FilterType,
    DynamicImage, ImageBuffer, Rgb,
};
use std::{fs, io::Cursor, path::Path};

const SOURCE_IMAGE_PATH: &str = "./public/images.jpeg";

async fn hello() -> &'static str {
    "Hello Axum 🚀"
}

async fn cpu_bound() -> impl IntoResponse {
    match tokio::task::spawn_blocking(render_resized_jpeg).await {
        Ok(Ok(bytes)) => jpeg_response(StatusCode::OK, bytes),
        Ok(Err(error)) => {
            tracing::error!(%error, "failed to render cpu-bound response");
            jpeg_response(StatusCode::INTERNAL_SERVER_ERROR, Vec::new())
        }
        Err(error) => {
            tracing::error!(%error, "cpu-bound worker panicked");
            jpeg_response(StatusCode::INTERNAL_SERVER_ERROR, Vec::new())
        }
    }
}

fn render_resized_jpeg() -> Result<Vec<u8>, image::ImageError> {
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

fn jpeg_response(status: StatusCode, bytes: Vec<u8>) -> Response<Body> {
    let mut response = Response::new(Body::from(bytes));
    *response.status_mut() = status;
    response.headers_mut().insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("image/jpeg"),
    );
    response
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt::init();

    let app = Router::new()
        .route("/", get(hello))
        .route("/cpu-bound", get(cpu_bound))
        .route("/api/v1/cpu-bound", get(cpu_bound));

    let listener = tokio::net::TcpListener::bind("0.0.0.0:3000")
        .await
        .unwrap();

    axum::serve(listener, app).await.unwrap();
}
