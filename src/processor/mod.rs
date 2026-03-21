mod image_processor;
mod video_processor;

use std::{
    sync::{
        Arc, Mutex,
        mpsc::{Iter, Sender, channel},
    },
    thread,
};

use anyhow::Result;
use redis::{AsyncCommands, aio::MultiplexedConnection};
use threadpool::ThreadPool;
use tokio::runtime::Handle;

use crate::{
    config::Config,
    jobs::{ProcessJob, Task, TaskType},
    log_msg,
};

use self::{image_processor::ImageProcessor, video_processor::VideoProcessor};

pub(crate) trait TaskProcessor {
    fn process_task(&self, task: &Task, tx: Sender<ProcessMessage>) -> Result<()>;
}

pub(crate) struct ProcessMessage {
    pub task_id: u64,
    pub m_type: TaskMessageType,
}

pub(crate) enum TaskMessageType {
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
            } => process_multiple_tasks(job_id, tasks_to_process, config, redis_conn),
            ProcessJob::Unique { .. } => todo!(),
        }
    }
}

fn process_multiple_tasks(
    job_id: &str,
    tasks: Vec<Task>,
    config: &Config,
    redis_conn: &MultiplexedConnection,
) -> Result<(), Vec<Task>> {
    let poll = ThreadPool::new(config.workers as usize);
    let (tx, rx) = channel();
    let handler = Handle::current();
    // MultiplexedConnection is cheaply cloneable and safe across threads (see redis docs)
    let redis_conn = redis_conn.clone();
    let job_id = job_id.to_string();

    thread::spawn(move || {
        let mut iter: Iter<'_, ProcessMessage> = rx.iter();

        while let Some(ProcessMessage { task_id, m_type }) = iter.next() {
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
        let processor = get_task_processor(&task.task_type);
        let tx_clone = tx.clone();
        let failed_tasks_mutex = failed_tasks.clone();

        poll.execute(move || {
            if let Err(err) = processor.process_task(&task, tx_clone) {
                log_msg!(error, "Error processing task {}. Error: {}", task.id, err);
                failed_tasks_mutex
                    .lock()
                    .expect("Poisoned lock found!")
                    .push(task);
            }
        });
    }

    drop(tx);
    poll.join();

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
