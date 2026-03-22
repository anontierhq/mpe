use std::{path::PathBuf, sync::mpsc::Sender};

use anyhow::Result;

use crate::processor::{ProcessMessage, TaskMessageType};

pub(crate) struct PipelineContext<'a> {
    pub job_id: &'a str,
    pub task_id: u64,
    pub work_dir: PathBuf,
    pub output_path: PathBuf,
    tx: &'a Sender<ProcessMessage>,
}

impl<'a> PipelineContext<'a> {
    pub fn new(
        job_id: &'a str,
        task_id: u64,
        work_dir: PathBuf,
        output_path: PathBuf,
        tx: &'a Sender<ProcessMessage>,
    ) -> Self {
        Self {
            job_id,
            task_id,
            work_dir,
            output_path,
            tx,
        }
    }

    pub fn report(&self, msg: impl Into<String>) {
        let _ = self.tx.send(ProcessMessage {
            task_id: self.task_id,
            m_type: TaskMessageType::Processing(msg.into()),
        });
    }
}

pub(crate) trait Step<I, O> {
    fn run(&self, input: I, ctx: &PipelineContext) -> Result<O>;
}
