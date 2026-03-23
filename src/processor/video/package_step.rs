use std::{fs, path::PathBuf, process::Command};

use anyhow::{Result, anyhow};

use crate::processor::pipeline::{PipelineContext, Step};

use super::children_step::RenditionSet;

pub struct PackagedOutput {
    pub output_dir: PathBuf,
}

pub struct PackageStep;

impl Step<RenditionSet, PackagedOutput> for PackageStep {
    fn run(&self, input: RenditionSet, ctx: &PipelineContext) -> Result<PackagedOutput> {
        ctx.report("Packaging", "Packaging streams (HLS + MPEG-DASH / CMAF)...");

        let output_dir = ctx.work_dir.join("packaged");
        fs::create_dir_all(&output_dir)?;

        let mut cmd = Command::new("packager");
        cmd.current_dir(&output_dir);

        for rendition in &input.renditions {
            cmd.arg(format!(
                "input={},stream=video,output={}.mp4,segment_template={}_$Number$.m4s",
                rendition.path.display(),
                rendition.label,
                rendition.label,
            ));
        }

        if let Some(audio) = &input.audio_path {
            cmd.arg(format!(
                "input={},stream=audio,output=audio.mp4,segment_template=audio_$Number$.m4s",
                audio.display(),
            ));
        }

        cmd.arg("--mpd_output")
            .arg("manifest.mpd")
            .arg("--hls_master_playlist_output")
            .arg("master.m3u8")
            .args(["--fragment_duration", "2", "--segment_duration", "6"])
            .arg("--generate_static_live_mpd");

        let status = cmd.status()?;
        if !status.success() {
            return Err(anyhow!("packager failed"));
        }

        ctx.report(
            "Packaging",
            format!("Packaging complete: output at {}", output_dir.display()),
        );

        Ok(PackagedOutput { output_dir })
    }
}
