use std::path::PathBuf;

use anyhow::{Result, anyhow};

use crate::processor::pipeline::{PipelineContext, Step};

/// Substrings matched against ffprobe `format.format_name` (comma-separated tokens).
const ALLOWED_FORMAT_TOKENS: &[&str] = &[
    "image2",
    "jpeg_pipe",
    "png_pipe",
    "webp_pipe",
    "bmp_pipe",
    "gif",
    "tiff",
    "tif",
    "heif",
    "heic",
    "avif",
    "jp2",
    "jpegxl",
];

pub struct ValidatedImage {
    pub path: PathBuf,
    pub width: u32,
    pub height: u32,
}

pub struct ValidateImageStep;

impl Step<PathBuf, ValidatedImage> for ValidateImageStep {
    fn run(&self, path: PathBuf, ctx: &PipelineContext) -> Result<ValidatedImage> {
        ctx.report("Validating", "Validating input image...");

        if !path.exists() {
            return Err(anyhow!("File not found: {}", path.display()));
        }

        let probe = ffprobe::ffprobe(&path)
            .map_err(|e| anyhow!("ffprobe failed: file may be corrupted or unreadable: {e}"))?;

        let format_ok = probe
            .format
            .format_name
            .split(',')
            .any(|f| ALLOWED_FORMAT_TOKENS.contains(&f.trim()));

        if !format_ok {
            return Err(anyhow!(
                "Image container '{}' is not in the allowed formats list",
                probe.format.format_name
            ));
        }

        let video = probe
            .streams
            .iter()
            .find(|s| s.codec_type.as_deref() == Some("video"))
            .ok_or_else(|| anyhow!("No image/video stream found (ffprobe)"))?;

        let width = video.width.ok_or_else(|| anyhow!("Could not read width"))? as u32;
        let height = video
            .height
            .ok_or_else(|| anyhow!("Could not read height"))? as u32;

        ctx.report(
            "Validating",
            format!(
                "Validation OK: {}x{}, format: {}",
                width, height, probe.format.format_name
            ),
        );

        Ok(ValidatedImage {
            path,
            width,
            height,
        })
    }
}
