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
    log_msg,
    processor::{image_processor::ImageProcessor, video_processor::VideoProcessor},
    tasks::{Media, MediaType, ProcessTask},
};

pub(super) trait MediaProcessor {
    fn process_media(&self, media: &Media, tx: Sender<ProcessMessage>) -> Result<()>;
}
pub(super) struct ProcessMessage {
    pub media_id: u64,
    pub m_type: ProcessMessageType,
}

pub(super) enum ProcessMessageType {
    Processing(String),
    Failed(String),
    Finished,
}

pub struct TaskHandler;

impl TaskHandler {
    pub fn it<'a>(
        task: ProcessTask<'a>,
        config: &'a Config,
        redis_conn: &MultiplexedConnection,
    ) -> Result<(), Vec<Media>> {
        match task {
            ProcessTask::Composed {
                job_id,
                medias_to_process,
            } => process_multiple_medias(job_id, medias_to_process, config, redis_conn),
            ProcessTask::Unique { .. } => todo!(),
        }
    }
}

fn process_multiple_medias(
    task_id: &str,
    medias: Vec<Media>,
    config: &Config,
    redis_conn: &MultiplexedConnection,
) -> Result<(), Vec<Media>> {
    let poll = ThreadPool::new(config.workers as usize);
    let (tx, rx) = channel();
    let handler = Handle::current();
    // We only do this to preserve the original redis connection.
    // According to docs theres no problems, cause: "For async connections, connection
    // pooling isn't necessary, unless blocking commands are used. The MultiplexedConnection
    // is cheaply cloneable and can be used safely from multiple threads, so a single connection
    // can be easily reused"
    let redis_conn = redis_conn.clone();
    let task_id = task_id.to_string();
    thread::spawn(move || {
        let mut iter: Iter<'_, ProcessMessage> = rx.iter();

        while let Some(ProcessMessage { media_id, m_type }) = iter.next() {
            log_msg!(
                debug,
                "Received process message to handle. Media id {media_id}"
            );
            let mut redis_conn_clone = redis_conn.clone();
            let task_id = task_id.clone();
            match m_type {
                ProcessMessageType::Processing(message) => handler.spawn(async move {
                    log_msg!(debug, "Received processing message: {message}");
                    if let Err(err) = redis_conn_clone
                        .set(&format!("task:{task_id}:media:{}", media_id), message)
                        .await
                        .map(|_: ()| ())
                    {
                        log_msg!(
                            error,
                            "Failed to send processing message to redis. Error: {err}"
                        )
                    };
                }),
                ProcessMessageType::Failed(message) => handler.spawn(async move {
                    log_msg!(debug, "Received failed message: {message}");
                    if let Err(err) = redis_conn_clone
                        .set(&format!("task:{task_id}:media:{}", media_id), message)
                        .await
                        .map(|_: ()| ())
                    {
                        log_msg!(
                            error,
                            "Failed to send processing message to redis. Error: {err}"
                        )
                    };
                }),
                ProcessMessageType::Finished => todo!(),
            };
        }

        log_msg!(
            info,
            "All processing messages have been handled, exiting message handler thread."
        );
    });

    let failed_medias = Arc::new(Mutex::new(vec![]));
    for media in medias {
        let processor = get_media_processor(&media.media_type);
        let tx_clone = tx.clone();

        let failed_medias_mutex = failed_medias.clone();
        poll.execute(move || {
            let media = media;
            match processor.process_media(&media, tx_clone) {
                Err(err) => {
                    log_msg!(
                        error,
                        "Error while processing media {}. Error: {}",
                        media.id,
                        err
                    );
                    failed_medias_mutex
                        .lock()
                        .expect("Poisoned lock found!")
                        .push(media)
                }
                _ => {}
            }
        });
    }

    // drop tx to close the channel on main thread
    // and allow the receiver thread to exit once
    // all messages are processed
    drop(tx);

    // Wait for all threads be completed
    poll.join();

    let mut failed_medias = failed_medias.lock().expect("Poisoned lock found!");
    if !failed_medias.is_empty() {
        let failed_medias = failed_medias.drain(..).collect::<Vec<Media>>();
        log_msg!(debug, "Failed medias: {:?}", failed_medias);

        return Err(failed_medias);
    }

    Ok(())
}

fn get_media_processor(m_type: &MediaType) -> Box<dyn MediaProcessor + Send + Sync> {
    match m_type {
        MediaType::Video => Box::new(VideoProcessor),
        MediaType::Image => Box::new(ImageProcessor),
    }
}
