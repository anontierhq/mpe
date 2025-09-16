use crate::{
    config::Config,
    rabbit::{Media, ProcessTask},
};

pub trait MessageProcessor {
    fn process_message(&self, task: ProcessTask, config: &Config) -> Result<(), Vec<Media>>;
}

pub struct TaskHandler;

impl TaskHandler {
    pub fn it(task: ProcessTask) -> Result<(), Vec<Media>> {
        match task {
            ProcessTask::Composed {
                job_id,
                medias_to_process,
            } => todo!(),
            ProcessTask::Unique {
                attached_job,
                media_to_process,
            } => todo!(),
        }
    }
}
