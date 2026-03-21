use std::path::PathBuf;

use anyhow::{Result, anyhow};

use crate::processor::pipeline::{PipelineContext, Step};

const ALLOWED_FORMATS: &[&str] = &["mp4", "mov", "mkv", "webm", "avi", "matroska"];

pub struct ValidatedVideo {
    pub path: PathBuf,
    pub width: u32,
    pub height: u32,
    pub has_audio: bool,
}

pub(crate) struct ValidateStep;

impl Step<PathBuf, ValidatedVideo> for ValidateStep {
    fn run(&self, path: PathBuf, ctx: &PipelineContext) -> Result<ValidatedVideo> {
        ctx.report("Validating input file...");

        if !path.exists() {
            return Err(anyhow!("File not found: {}", path.display()));
        }

        let probe = ffprobe::ffprobe(&path)
            .map_err(|e| anyhow!("ffprobe failed: file may be corrupted or unreadable: {e}"))?;

        let allowed = probe
            .format
            .format_name
            .split(',')
            .any(|f| ALLOWED_FORMATS.contains(&f.trim()));

        if !allowed {
            return Err(anyhow!(
                "Container '{}' is not in the allowed formats list",
                probe.format.format_name
            ));
        }

        let video = probe
            .streams
            .iter()
            .find(|s| s.codec_type.as_deref() == Some("video"))
            .ok_or_else(|| anyhow!("No video stream found"))?;

        let width = video.width.ok_or_else(|| anyhow!("Could not read width"))? as u32;
        let height = video
            .height
            .ok_or_else(|| anyhow!("Could not read height"))? as u32;
        let has_audio = probe
            .streams
            .iter()
            .any(|s| s.codec_type.as_deref() == Some("audio"));

        let duration_secs = probe
            .format
            .duration
            .as_deref()
            .and_then(|d| d.parse::<f64>().ok())
            .unwrap_or(0.0);

        ctx.report(format!(
            "Validation OK: {}x{}, {:.1}s, format: {}",
            width, height, duration_secs, probe.format.format_name
        ));

        Ok(ValidatedVideo {
            path,
            width,
            height,
            has_audio,
        })
    }
}
