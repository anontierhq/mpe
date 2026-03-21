use crate::jobs::Task;
use super::{ProcessMessage, TaskProcessor};

pub struct ImageProcessor;

impl TaskProcessor for ImageProcessor {
    fn process_task(
        &self,
        _task: &Task,
        _tx: std::sync::mpsc::Sender<ProcessMessage>,
    ) -> anyhow::Result<()> {
        Ok(())
    }
}
