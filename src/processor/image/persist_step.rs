use std::fs;

use anyhow::{Result, anyhow};

use crate::processor::pipeline::{PipelineContext, Step};

use super::privacy_metadata_step::SanitizedImage;

pub struct ImagePersistStep;

impl Step<SanitizedImage, ()> for ImagePersistStep {
    fn run(&self, input: SanitizedImage, ctx: &PipelineContext) -> Result<()> {
        ctx.report(
            "Persisting",
            format!("Persisting output to {}...", ctx.output_path.display()),
        );

        let depth = ctx.output_path.components().count();
        if depth < 4 {
            return Err(anyhow!(
                "Refusing to write to shallow path '{}' (depth {depth} < 4)",
                ctx.output_path.display()
            ));
        }

        let dest_name = input
            .path
            .file_name()
            .ok_or_else(|| anyhow!("Sanitized path has no file name"))?;

        if ctx.output_path.exists() {
            fs::remove_dir_all(&ctx.output_path).map_err(|e| {
                anyhow!(
                    "Failed to clear existing output dir {}: {e}",
                    ctx.output_path.display()
                )
            })?;
        }

        if let Some(parent) = ctx.output_path.parent() {
            fs::create_dir_all(parent)?;
        }

        fs::create_dir_all(&ctx.output_path)?;

        let dest = ctx.output_path.join(dest_name);
        fs::copy(&input.path, &dest).map_err(|e| {
            anyhow!(
                "Failed to copy output to {}: {e}",
                dest.display()
            )
        })?;

        ctx.report(
            "Persisting",
            format!("Persisted at {}", dest.display()),
        );
        Ok(())
    }
}
