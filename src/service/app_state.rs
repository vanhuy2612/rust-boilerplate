use dotenvy::dotenv;
use std::{sync::Arc};

use crate::service::cpu_worker_pool::CpuWorkerPool;
use crate::service::io_service::IoServiceError;

pub struct AppState {
    pub cpu_worker_pool: CpuWorkerPool,
}

impl AppState {
    pub async fn from_env() -> Result<Arc<Self>, IoServiceError> {
        let _ = dotenv();

        Ok(Arc::new(Self {
            cpu_worker_pool: CpuWorkerPool::new_for_available_parallelism(),
        }))
    }
}
