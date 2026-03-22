mod children_step;
mod matrix_step;
mod package_step;
mod persist_step;
mod validate_step;

use std::{fs, path::PathBuf, sync::mpsc::Sender};

use anyhow::Result;

use crate::{
    jobs::Task,
    log_msg,
    processor::pipeline::{PipelineContext, Step},
};

use super::{ProcessMessage, TaskProcessor};
use children_step::GenerateChildrenStep;
use matrix_step::MatrixStep;
use package_step::PackageStep;
use persist_step::PersistStep;
use validate_step::ValidateStep;

pub struct VideoProcessor;

impl TaskProcessor for VideoProcessor {
    fn process_task(
        &self,
        job_id: &str,
        task: &Task,
        output_path: PathBuf,
        tx: Sender<ProcessMessage>,
    ) -> Result<()> {
        let work_dir = PathBuf::from(format!("/tmp/mpe/{}/task_{}", job_id, task.id));
        fs::create_dir_all(&work_dir)?;

        let ctx = PipelineContext::new(task.id, work_dir, output_path, &tx);
        let input_path = PathBuf::from(&task.filepath);

        log_msg!(info, "Starting video pipeline for task {}", task.id);

        let validated = ValidateStep.run(input_path, &ctx)?;
        let matrix = MatrixStep.run(validated, &ctx)?;
        let children = GenerateChildrenStep.run(matrix, &ctx)?;
        let packaged = PackageStep.run(children, &ctx)?;
        PersistStep.run(packaged, &ctx)?;

        log_msg!(info, "Video pipeline complete for task {}", task.id);
        Ok(())
    }
}
