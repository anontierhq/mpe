mod image_processor;
pub mod pipeline;
mod video;

use std::{
    path::PathBuf,
    sync::{
        Arc, Mutex,
        mpsc::{Sender, channel},
    },
    thread,
};

use anyhow::{Result, anyhow};
use redis::{AsyncCommands, aio::MultiplexedConnection};
use threadpool::ThreadPool;
use tokio::runtime::Handle;

use crate::{
    config::Config,
    jobs::{ProcessJob, Task, TaskType},
    log_msg,
};

use self::{image_processor::ImageProcessor, video::VideoProcessor};

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

#[allow(dead_code)]
pub enum TaskMessageType {
    Processing(String),
    Failed(String),
    Finished,
}

pub struct JobHandler;

impl JobHandler {
    pub fn it<'a>(
        job: ProcessJob<'a>,
        config: &'a Config,
        redis_conn: &MultiplexedConnection,
    ) -> Result<(), Vec<Task>> {
        match job {
            ProcessJob::Composed {
                job_id,
                tasks_to_process,
                base_output,
            } => process_multiple_tasks(
                job_id,
                tasks_to_process,
                base_output.as_deref(),
                config,
                redis_conn,
            ),
            ProcessJob::Unique { .. } => todo!(),
        }
    }
}

fn resolve_output_path(job_id: &str, task: &Task, base_output: Option<&str>) -> Result<PathBuf> {
    let path = match (&task.output_path, base_output) {
        (Some(_), Some(_)) => return Err(anyhow!(
            "Task {} specifies both output_path and job base_output: Use one or the other",
            task.id
        )),
        (Some(path), None) => PathBuf::from(path),
        (None, Some(base)) => PathBuf::from(base).join(job_id).join(task.id.to_string()),
        (None, None) => return Err(anyhow!(
            "Task {} has no output path. Set output_path on the task or base_output on the job",
            task.id
        )),
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

fn process_multiple_tasks(
    job_id: &str,
    tasks: Vec<Task>,
    base_output: Option<&str>,
    config: &Config,
    redis_conn: &MultiplexedConnection,
) -> Result<(), Vec<Task>> {
    let pool = ThreadPool::new(config.workers as usize);
    let (tx, rx) = channel();
    let handler = Handle::current();
    // MultiplexedConnection is cheaply cloneable and safe across threads (see redis docs)
    let redis_conn = redis_conn.clone();
    let job_id = job_id.to_string();
    let job_id_for_status = job_id.clone();

    thread::spawn(move || {
        let job_id = job_id_for_status;
        for ProcessMessage { task_id, m_type } in rx {
            log_msg!(debug, "Received process message. Task {task_id}");
            let mut redis_conn = redis_conn.clone();
            let job_id = job_id.clone();
            match m_type {
                TaskMessageType::Processing(message) => handler.spawn(async move {
                    if let Err(err) = redis_conn
                        .set::<_, _, ()>(format!("job:{job_id}:task:{task_id}"), message)
                        .await
                    {
                        log_msg!(error, "Failed to set task {task_id} status: {err}");
                    }
                }),
                TaskMessageType::Failed(message) => handler.spawn(async move {
                    if let Err(err) = redis_conn
                        .set::<_, _, ()>(format!("job:{job_id}:task:{task_id}"), message)
                        .await
                    {
                        log_msg!(error, "Failed to set task {task_id} status: {err}");
                    }
                }),
                TaskMessageType::Finished => todo!(),
            };
        }

        log_msg!(
            info,
            "All processing messages handled. Exiting message handler thread."
        );
    });

    let failed_tasks = Arc::new(Mutex::new(vec![]));
    for task in tasks {
        let output_path = match resolve_output_path(&job_id, &task, base_output) {
            Ok(p) => p,
            Err(err) => {
                log_msg!(error, "Output path error for task {}: {}", task.id, err);
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
            if let Err(err) = processor.process_task(&job_id_clone, &task, output_path, tx_clone) {
                log_msg!(error, "Error processing task {}. Error: {}", task.id, err);
                failed_tasks_mutex
                    .lock()
                    .expect("Poisoned lock found!")
                    .push(task);
            }
        });
    }

    drop(tx);
    pool.join();

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
