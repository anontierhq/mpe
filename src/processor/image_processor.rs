use super::{ProcessMessage, TaskProcessor};
use crate::jobs::Task;

pub struct ImageProcessor;

impl TaskProcessor for ImageProcessor {
    fn process_task(
        &self,
        _job_id: &str,
        _task: &Task,
        _output_path: std::path::PathBuf,
        _tx: std::sync::mpsc::Sender<ProcessMessage>,
    ) -> anyhow::Result<()> {
        Ok(())
    }
}
