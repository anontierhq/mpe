use std::sync::mpsc::Sender;

use crate::{
    jobs::Task,
    processor::processor::{ProcessMessage, TaskProcessor},
};

pub struct VideoProcessor;

impl TaskProcessor for VideoProcessor {
    fn process_task(&self, _task: &Task, _tx: Sender<ProcessMessage>) -> anyhow::Result<()> {
        todo!()
    }
}
