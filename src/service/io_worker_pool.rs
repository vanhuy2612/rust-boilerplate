use std::{env, sync::Arc};
use tokio::sync::{AcquireError, OwnedSemaphorePermit, Semaphore};

const DEFAULT_IO_WORKERS: usize = 100;

pub struct IoWorkerPool {
    permits: Arc<Semaphore>,
}

impl IoWorkerPool {
    pub fn from_env() -> Self {
        let worker_count = env::var("IO_WORKER_POOL_SIZE")
            .or_else(|_| env::var("IO_WORKERS"))
            .or_else(|_| env::var("MAX_IO_WORKERS"))
            .ok()
            .and_then(|value| value.parse::<usize>().ok())
            .filter(|value| *value > 0)
            .unwrap_or(DEFAULT_IO_WORKERS);

        Self::new(worker_count)
    }

    pub fn new(worker_count: usize) -> Self {
        Self {
            permits: Arc::new(Semaphore::new(worker_count.max(1))),
        }
    }

    pub async fn acquire(&self) -> Result<OwnedSemaphorePermit, AcquireError> {
        self.permits.clone().acquire_owned().await
    }
}
