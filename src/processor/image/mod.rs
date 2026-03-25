mod persist_step;
mod privacy_metadata_step;
mod validate_step;

use std::{path::PathBuf, sync::mpsc::Sender};

use anyhow::Result;

use crate::{
    jobs::Task,
    log_msg,
    processor::pipeline::{PipelineContext, Step},
};

use self::{
    persist_step::ImagePersistStep, privacy_metadata_step::StripPrivacyMetadataStep,
    validate_step::ValidateImageStep,
};
use super::{ProcessMessage, TaskProcessor};

pub struct ImageProcessor;

impl TaskProcessor for ImageProcessor {
    fn process_task(
        &self,
        _job_id: &str,
        task: &Task,
        output_path: PathBuf,
        tx: Sender<ProcessMessage>,
    ) -> Result<()> {
        let work_dir = tempfile::Builder::new().prefix("mpe-img-").tempdir()?;

        let ctx = PipelineContext::new(task.id, work_dir.path().to_path_buf(), output_path, &tx);
        let input_path = PathBuf::from(&task.filepath);

        log_msg!(info, "Starting image pipeline for task {}", task.id);

        let validated = ValidateImageStep.run(input_path, &ctx)?;
        let sanitized = StripPrivacyMetadataStep.run(validated, &ctx)?;
        ImagePersistStep.run(sanitized, &ctx)?;

        log_msg!(info, "Image pipeline complete for task {}", task.id);
        Ok(())
    }
}
