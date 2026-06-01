use std::{
    fmt,
    sync::{Arc, Mutex, mpsc},
    thread,
};
use tokio::sync::Semaphore;

type Job = Box<dyn FnOnce() + Send + 'static>;

pub struct CpuWorkerPool {
    queue_permits: Arc<Semaphore>,
    sender: Option<mpsc::Sender<Job>>,
    workers: Vec<thread::JoinHandle<()>>,
}

#[derive(Debug)]
pub enum CpuWorkerPoolError {
    Closed,
    ShutDown,
    WorkerDropped,
}

impl CpuWorkerPool {
    pub fn new_for_available_parallelism() -> Self {
        let worker_count = thread::available_parallelism()
            .map(usize::from)
            .unwrap_or(1)
            .max(1);

        Self::new(worker_count)
    }

    fn new(worker_count: usize) -> Self {
        let (sender, receiver) = mpsc::channel::<Job>();
        let receiver = Arc::new(Mutex::new(receiver));
        let mut workers = Vec::with_capacity(worker_count);

        for worker_index in 0..worker_count {
            let receiver = Arc::clone(&receiver);
            let worker = thread::Builder::new()
                .name(format!("cpu-worker-{worker_index}"))
                .spawn(move || {
                    loop {
                        let job = {
                            let receiver =
                                receiver.lock().expect("cpu worker receiver lock poisoned");
                            receiver.recv()
                        };

                        match job {
                            Ok(job) => job(),
                            Err(_) => break,
                        }
                    }
                })
                .expect("failed to spawn CPU worker");

            workers.push(worker);
        }

        Self {
            queue_permits: Arc::new(Semaphore::new(worker_count)),
            sender: Some(sender),
            workers,
        }
    }

    pub async fn execute<F, R>(&self, task: F) -> Result<R, CpuWorkerPoolError>
    where
        F: FnOnce() -> R + Send + 'static,
        R: Send + 'static,
    {
        self.submit(task)
            .await?
            .await
            .map_err(|_| CpuWorkerPoolError::WorkerDropped)
    }

    pub async fn submit<F, R>(
        &self,
        task: F,
    ) -> Result<tokio::sync::oneshot::Receiver<R>, CpuWorkerPoolError>
    where
        F: FnOnce() -> R + Send + 'static,
        R: Send + 'static,
    {
        let sender = self.sender.as_ref().ok_or(CpuWorkerPoolError::ShutDown)?;
        let permit = self
            .queue_permits
            .clone()
            .acquire_owned()
            .await
            .map_err(|_| CpuWorkerPoolError::Closed)?;
        let (result_sender, result_receiver) = tokio::sync::oneshot::channel();
        let job = Box::new(move || {
            let _permit = permit;
            let _ = result_sender.send(task());
        });

        sender.send(job).map_err(|_| CpuWorkerPoolError::ShutDown)?;
        Ok(result_receiver)
    }
}

impl Drop for CpuWorkerPool {
    fn drop(&mut self) {
        self.sender.take();

        while let Some(worker) = self.workers.pop() {
            let _ = worker.join();
        }
    }
}

impl fmt::Display for CpuWorkerPoolError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Closed => formatter.write_str("CPU worker pool is closed"),
            Self::ShutDown => formatter.write_str("CPU worker pool is shut down"),
            Self::WorkerDropped => {
                formatter.write_str("CPU worker dropped before returning result")
            }
        }
    }
}

impl std::error::Error for CpuWorkerPoolError {}
