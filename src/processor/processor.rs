use std::{
    ops::Deref,
    sync::{
        Arc,
        mpsc::{Sender, channel},
    },
    thread,
};

use anyhow::Result;
use threadpool::ThreadPool;
use tokio::runtime::Handle;

use crate::{
    config::Config,
    log_msg,
    processor::{image_processor::ImageProcessor, video_processor::VideoProcessor},
    tasks::{Media, MediaType, ProcessTask},
};

pub(super) trait MediaProcessor {
    fn process_media<'m>(
        &self,
        media: &'m Media,
        tx: Sender<ProcessMessage>,
    ) -> Result<(), &'m Media>;
}
pub(super) struct ProcessMessage {
    pub media_id: i32,
    pub m_type: ProcessMessageType,
}

pub(super) enum ProcessMessageType {
    Processing(String),
    Failed(String),
    Finished,
}

pub struct TaskHandler;

impl TaskHandler {
    pub fn it<'a>(task: ProcessTask<'a>, config: &'a Config) -> Result<(), Vec<Media>> {
        match task {
            ProcessTask::Composed {
                job_id: _,
                medias_to_process,
            } => process_multiple_medias(medias_to_process, config),
            ProcessTask::Unique { .. } => todo!(),
        }
    }
}

fn process_multiple_medias(medias: Vec<Media>, config: &Config) -> Result<(), Vec<Media>> {
    let poll = ThreadPool::new(config.workers as usize);
    let (tx, rx) = channel();

    let handler = Handle::current();
    thread::spawn(move || {
        let mut iter = rx.iter();

        while let Some(message) = iter.next() {
            handler.spawn(async {
                // Here we will dispatch the appropriatted messages for redis
            });
        }

        log_msg!(
            info,
            "All processing messages have been handled, exiting message handler thread."
        );
    });

    for media in medias {
        let processor = get_media_processor(&media.media_type);
        let tx_clone = tx.clone();

        poll.execute(move || match processor.process_media(&media, tx_clone) {
            Err(failed) => todo!(),
            _ => {}
        });
    }

    // drop tx to close the channel on main thread
    // and allow the receiver thread to exit once
    // all messages are processed
    drop(tx);

    Ok(())
}

fn get_media_processor(m_type: &MediaType) -> Box<dyn MediaProcessor + Send + Sync> {
    match m_type {
        MediaType::Video => Box::new(VideoProcessor),
        MediaType::Image => Box::new(ImageProcessor),
    }
}
