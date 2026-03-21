use crate::{jobs::Task, processor::processor::TaskProcessor};

pub struct ImageProcessor;

impl TaskProcessor for ImageProcessor {
    fn process_task(
        &self,
        _task: &Task,
        _tx: std::sync::mpsc::Sender<super::processor::ProcessMessage>,
    ) -> anyhow::Result<()> {
        Ok(())
    }
}
