use std::{fs, path::PathBuf, process::Command};

use anyhow::{Result, anyhow};

use crate::processor::pipeline::{PipelineContext, Step};

use super::children_step::RenditionSet;

pub struct PackagedOutput {
    pub output_dir: PathBuf,
    pub mpd_path: PathBuf,
    pub m3u8_path: PathBuf,
}

pub struct PackageStep;

impl Step<RenditionSet, PackagedOutput> for PackageStep {
    fn run(&self, input: RenditionSet, ctx: &PipelineContext) -> Result<PackagedOutput> {
        ctx.report("Packaging streams (HLS + MPEG-DASH / CMAF)...");

        let output_dir = PathBuf::from("output")
            .join(ctx.job_id)
            .join(ctx.task_id.to_string());
        fs::create_dir_all(&output_dir)?;

        let mut cmd = Command::new("packager");

        for rendition in &input.renditions {
            cmd.arg(format!(
                "input={},stream=video,output={},segment_template={}",
                rendition.path.display(),
                output_dir
                    .join(format!("{}.mp4", rendition.label))
                    .display(),
                output_dir
                    .join(format!("{}_$Number$.m4s", rendition.label))
                    .display(),
            ));
        }

        if let Some(audio) = &input.audio_path {
            cmd.arg(format!(
                "input={},stream=audio,output={},segment_template={}",
                audio.display(),
                output_dir.join("audio.mp4").display(),
                output_dir.join("audio_$Number$.m4s").display(),
            ));
        }

        let mpd_path = output_dir.join("manifest.mpd");
        let m3u8_path = output_dir.join("master.m3u8");

        cmd.arg("--mpd_output")
            .arg(&mpd_path)
            .arg("--hls_master_playlist_output")
            .arg(&m3u8_path)
            .args(["--fragment_duration", "2", "--segment_duration", "6"])
            .arg("--generate_static_live_mpd");

        let status = cmd.status()?;
        if !status.success() {
            return Err(anyhow!("packager failed"));
        }

        ctx.report(format!(
            "Packaging complete: output at {}",
            output_dir.display()
        ));

        Ok(PackagedOutput {
            output_dir,
            mpd_path,
            m3u8_path,
        })
    }
}
