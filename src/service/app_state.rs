use dotenvy::dotenv;
use image::DynamicImage;
use std::sync::Arc;

use crate::service::cpu_service;
use crate::service::cpu_worker_pool::CpuWorkerPool;
use crate::service::io_service::IoServiceError;

pub struct AppState {
    pub cpu_worker_pool: CpuWorkerPool,
    /// Source image decoded once at startup and shared (read-only) by all requests.
    pub source_image: Arc<DynamicImage>,
}

impl AppState {
    pub async fn from_env() -> Result<Arc<Self>, IoServiceError> {
        let _ = dotenv();

        Ok(Arc::new(Self {
            cpu_worker_pool: CpuWorkerPool::new_for_available_parallelism(),
            source_image: cpu_service::load_source_image(),
        }))
    }
}
