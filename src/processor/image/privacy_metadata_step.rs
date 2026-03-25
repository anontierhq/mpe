use std::{
    fs,
    path::PathBuf,
    process::{Command, Stdio},
};

use anyhow::{Result, anyhow};

use crate::processor::pipeline::{PipelineContext, Step};

use super::validate_step::ValidatedImage;

/// Output after privacy-focused metadata removal (see `docs/image-metadata.md`).
pub struct SanitizedImage {
    pub path: PathBuf,
}

pub struct StripPrivacyMetadataStep;

impl Step<ValidatedImage, SanitizedImage> for StripPrivacyMetadataStep {
    fn run(&self, input: ValidatedImage, ctx: &PipelineContext) -> Result<SanitizedImage> {
        ctx.report(
            "Metadata",
            format!(
                "Removing location and GPS-related metadata ({}x{}, exiftool)...",
                input.width, input.height
            ),
        );

        let file_name = input
            .path
            .file_name()
            .ok_or_else(|| anyhow!("Input path has no file name: {}", input.path.display()))?;

        let stage = ctx.work_dir.join("sanitized");
        fs::create_dir_all(&stage)?;
        let out_path = stage.join(file_name);
        fs::copy(&input.path, &out_path).map_err(|e| {
            anyhow!(
                "Failed to copy image to work dir for metadata strip: {e} (src {})",
                input.path.display()
            )
        })?;

        let output = Command::new("exiftool")
            .arg("-m")
            .arg("-q")
            .arg("-overwrite_original")
            .arg("-P")
            .args(EXIFTOOL_PRIVACY_ARGS)
            .arg(&out_path)
            .stdin(Stdio::null())
            .output()
            .map_err(|e| {
                anyhow!(
                    "Failed to run exiftool: {e}. Is exiftool installed and on PATH? (see docs/getting-started.md)"
                )
            })?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(anyhow!(
                "exiftool failed (exit {:?}): {}",
                output.status.code(),
                stderr.trim()
            ));
        }

        ctx.report(
            "Metadata",
            format!(
                "Stripped privacy-related metadata: {}",
                out_path.display()
            ),
        );

        Ok(SanitizedImage { path: out_path })
    }
}

/// Arguments documented in `docs/image-metadata.md` — keep in sync.
const EXIFTOOL_PRIVACY_ARGS: &[&str] = &[
    "-gps:all=",
    "-XMP:GPSLatitude=",
    "-XMP:GPSLongitude=",
    "-XMP:GPSAltitude=",
    "-XMP-iptcCore:City=",
    "-XMP-iptcCore:CountryName=",
    "-XMP-iptcCore:Location=",
    "-XMP-iptcCore:Region=",
    "-IPTC:City=",
    "-IPTC:Sub-location=",
    "-IPTC:Province-State=",
    "-IPTC:Country-PrimaryLocationName=",
    "-IPTC:Country-PrimaryLocationCode=",
];
