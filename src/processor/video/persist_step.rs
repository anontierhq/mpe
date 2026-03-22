use std::fs;

use anyhow::{Result, anyhow};

use crate::processor::pipeline::{PipelineContext, Step};

use super::package_step::PackagedOutput;

pub struct PersistStep;

impl Step<PackagedOutput, ()> for PersistStep {
    fn run(&self, input: PackagedOutput, ctx: &PipelineContext) -> Result<()> {
        ctx.report(format!(
            "Persisting output to {}...",
            ctx.output_path.display()
        ));

        if ctx.output_path.exists() {
            fs::remove_dir_all(&ctx.output_path).map_err(|e| {
                anyhow!(
                    "Failed to clear existing output dir {}: {e}",
                    ctx.output_path.display()
                )
            })?;
        }

        fs::create_dir_all(&ctx.output_path)?;

        // prefer rename. Falls back to copy+delete if EXDEV
        if fs::rename(&input.output_dir, &ctx.output_path).is_err() {
            copy_dir(&input.output_dir, &ctx.output_path)?;
            fs::remove_dir_all(&input.output_dir)?;
        }

        ctx.report(format!("Persisted at {}", ctx.output_path.display()));
        Ok(())
    }
}

fn copy_dir(src: &std::path::Path, dst: &std::path::Path) -> Result<()> {
    fs::create_dir_all(dst)?;
    for entry in fs::read_dir(src)? {
        let entry = entry?;
        let dst_path = dst.join(entry.file_name());
        if entry.file_type()?.is_dir() {
            copy_dir(&entry.path(), &dst_path)?;
        } else {
            fs::copy(entry.path(), dst_path)?;
        }
    }
    Ok(())
}
