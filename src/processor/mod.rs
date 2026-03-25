mod image_processor;
pub mod pipeline;
mod retry;
pub mod status;
mod video;

use std::{
    panic,
    path::PathBuf,
    sync::{
        Arc, Mutex,
        mpsc::{Sender, channel},
    },
    time::Duration,
};

use anyhow::{Result, anyhow};
use redis::aio::MultiplexedConnection;
use threadpool::ThreadPool;

use crate::{
    config::Config,
    jobs::{ProcessJob, Task, TaskType},
    log_msg,
    processor::status::StatusForwarder,
};

use self::{image_processor::ImageProcessor, retry::run_with_retries, video::VideoProcessor};

pub trait TaskProcessor {
    fn process_task(
        &self,
        job_id: &str,
        task: &Task,
        output_path: PathBuf,
        tx: Sender<ProcessMessage>,
    ) -> Result<()>;
}

pub struct ProcessMessage {
    pub task_id: u64,
    pub m_type: TaskMessageType,
}

pub enum TaskMessageType {
    Processing { step: String, message: String },
    Failed(String),
    Finished,
}

pub struct JobHandler;

impl JobHandler {
    pub async fn it<'a>(
        job: ProcessJob<'a>,
        config: &'a Config,
        redis_conn: &MultiplexedConnection,
    ) -> Result<(), Vec<Task>> {
        match job {
            ProcessJob::Composed {
                job_id,
                tasks_to_process,
                base_output,
            } => {
                process_multiple_tasks(
                    job_id,
                    tasks_to_process,
                    base_output.as_deref(),
                    config,
                    redis_conn,
                )
                .await
            }
            ProcessJob::Unique { .. } => todo!(),
        }
    }
}

fn resolve_output_path(job_id: &str, task: &Task, base_output: Option<&str>) -> Result<PathBuf> {
    let path = match (&task.output_path, base_output) {
        (Some(_), Some(_)) => {
            return Err(anyhow!(
                "Task {} specifies both output_path and job base_output: Use one or the other",
                task.id
            ));
        }
        (Some(path), None) => PathBuf::from(path),
        (None, Some(base)) => PathBuf::from(base).join(job_id).join(task.id.to_string()),
        (None, None) => {
            return Err(anyhow!(
                "Task {} has no output path. Set output_path on the task or base_output on the job",
                task.id
            ));
        }
    };

    if path.as_os_str().is_empty() {
        return Err(anyhow!("Task {} output path is empty", task.id));
    }

    if !path.is_absolute() {
        return Err(anyhow!(
            "Task {} output path '{}' must be absolute",
            task.id,
            path.display()
        ));
    }

    Ok(path)
}

async fn process_multiple_tasks(
    job_id: &str,
    tasks: Vec<Task>,
    base_output: Option<&str>,
    config: &Config,
    redis_conn: &MultiplexedConnection,
) -> Result<(), Vec<Task>> {
    let total_tasks = tasks.len() as u64;
    let job_id = job_id.to_string();

    if let Some(base) = base_output
        && !PathBuf::from(base).is_absolute()
    {
        let mut conn = redis_conn.clone();
        StatusForwarder::write_pre_dispatch_failure(
            &job_id,
            total_tasks,
            &format!("base_output '{}' must be an absolute path", base),
            &mut conn,
        )
        .await
        .expect("Redis write failed");
        return Err(vec![]);
    }

    let forwarder = StatusForwarder::new(job_id.clone(), total_tasks, redis_conn.clone())
        .await
        .expect("Failed to create StatusForwarder");
    forwarder
        .init()
        .await
        .expect("Failed to write queued status");

    let (tx, rx) = channel();

    let forwarder_handle = tokio::task::spawn_blocking(move || forwarder.run(rx));

    let pool = ThreadPool::new(config.workers as usize);
    let failed_tasks = Arc::new(Mutex::new(vec![]));
    let task_retries = config.task_retries;
    let retry_delay = Duration::from_millis(config.task_retry_delay_ms);

    for task in tasks {
        let output_path = match resolve_output_path(&job_id, &task, base_output) {
            Ok(p) => p,
            Err(err) => {
                log_msg!(error, "Output path error for task {}: {}", task.id, err);
                let _ = tx.send(ProcessMessage {
                    task_id: task.id,
                    m_type: TaskMessageType::Failed(err.to_string()),
                });
                failed_tasks
                    .lock()
                    .expect("Poisoned lock found!")
                    .push(task);
                continue;
            }
        };

        let processor = get_task_processor(&task.task_type);
        let tx_clone = tx.clone();
        let failed_tasks_mutex = failed_tasks.clone();
        let job_id_clone = job_id.clone();

        pool.execute(move || {
            let task_id = task.id;
            let job_id_for_log = job_id_clone.clone();
            let result = run_with_retries(
                task_retries,
                retry_delay,
                || {
                    match panic::catch_unwind(panic::AssertUnwindSafe(|| {
                        processor.process_task(
                            &job_id_clone,
                            &task,
                            output_path.clone(),
                            tx_clone.clone(),
                        )
                    })) {
                        Ok(inner) => inner,
                        Err(_) => Err(anyhow::anyhow!("task panicked")),
                    }
                },
                |failed_attempt, err| {
                    log_msg!(
                        warn,
                        "Task {task_id} (job {job_id_for_log}) attempt {failed_attempt} failed: {err}; retrying after {:?}",
                        retry_delay
                    );
                },
            );

            match result {
                Ok(()) => {
                    let _ = tx_clone.send(ProcessMessage {
                        task_id,
                        m_type: TaskMessageType::Finished,
                    });
                }
                Err(err) => {
                    log_msg!(
                        error,
                        "Error processing task {} (job {}). Error: {}",
                        task_id,
                        job_id_for_log,
                        err
                    );
                    let _ = tx_clone.send(ProcessMessage {
                        task_id,
                        m_type: TaskMessageType::Failed(err.to_string()),
                    });
                    failed_tasks_mutex
                        .lock()
                        .expect("Poisoned lock found!")
                        .push(task);
                }
            }
        });
    }

    drop(tx);
    pool.join();

    forwarder_handle
        .await
        .expect("StatusForwarder thread panicked");

    let mut failed_tasks = failed_tasks.lock().expect("Poisoned lock found!");
    if !failed_tasks.is_empty() {
        let failed_tasks = failed_tasks.drain(..).collect::<Vec<Task>>();
        log_msg!(info, "Failed tasks: {:#?}", failed_tasks);
        return Err(failed_tasks);
    }

    Ok(())
}

fn get_task_processor(m_type: &TaskType) -> Box<dyn TaskProcessor + Send + Sync> {
    match m_type {
        TaskType::Video => Box::new(VideoProcessor),
        TaskType::Image => Box::new(ImageProcessor),
    }
}
