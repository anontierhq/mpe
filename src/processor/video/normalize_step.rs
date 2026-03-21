use std::{path::PathBuf, process::Command};

use anyhow::{Result, anyhow};

use crate::processor::pipeline::{PipelineContext, Step};

use super::validate_step::ValidatedVideo;

pub struct NormalizedVideo {
    pub path: PathBuf,
    pub width: u32,
    pub height: u32,
    pub has_audio: bool,
}

pub struct NormalizeStep;

impl Step<ValidatedVideo, NormalizedVideo> for NormalizeStep {
    fn run(&self, input: ValidatedVideo, ctx: &PipelineContext) -> Result<NormalizedVideo> {
        ctx.report("Normalizing video...");

        let output_path = ctx.work_dir.join("normalized.mp4");

        let mut cmd = Command::new("ffmpeg");
        cmd.args(["-i"]).arg(&input.path).args([
            // Video: H.264, CRF 18 (high quality), yuv420p for max compatibility
            "-c:v",
            "libx264",
            "-crf",
            "18",
            "-preset",
            "medium",
            "-pix_fmt",
            "yuv420p",
            // Fast start: move moov atom to front for streaming
            "-movflags",
            "+faststart",
        ]);

        if input.has_audio {
            // Audio: AAC 192k, loudnorm for consistent loudness levels
            cmd.args(["-c:a", "aac", "-b:a", "192k", "-af", "loudnorm"]);
        } else {
            cmd.arg("-an");
        }

        cmd.args(["-y"]).arg(&output_path);

        let status = cmd.status()?;
        if !status.success() {
            return Err(anyhow!("ffmpeg normalization failed"));
        }

        ctx.report("Normalization complete");

        Ok(NormalizedVideo {
            path: output_path,
            width: input.width,
            height: input.height,
            has_audio: input.has_audio,
        })
    }
}
