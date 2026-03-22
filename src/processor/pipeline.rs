use std::{path::PathBuf, sync::mpsc::Sender};

use anyhow::{Result, anyhow};

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

pub(crate) fn probe_dimensions(path: &PathBuf) -> Result<(u32, u32)> {
    let probe =
        ffprobe::ffprobe(path).map_err(|e| anyhow!("ffprobe failed on {}: {e}", path.display()))?;

    let stream = probe
        .streams
        .iter()
        .find(|s| s.codec_type.as_deref() == Some("video"))
        .ok_or_else(|| anyhow!("No video stream in {}", path.display()))?;

    let w = stream.width.ok_or_else(|| anyhow!("Missing width"))? as u32;
    let h = stream.height.ok_or_else(|| anyhow!("Missing height"))? as u32;
    Ok((w, h))
}
