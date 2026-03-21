use std::{fs, path::PathBuf, process::Command};

use anyhow::{Result, anyhow};

use crate::processor::pipeline::{PipelineContext, Step, probe_dimensions};

use super::normalize_step::NormalizedVideo;

pub struct Rendition {
    pub path: PathBuf,
    pub label: String,
    pub width: u32,
    pub height: u32,
}

pub struct RenditionSet {
    pub renditions: Vec<Rendition>,
    pub audio_path: Option<PathBuf>,
}

struct RenditionSpec {
    label: &'static str,
    target_h: u32, // ref for landscape
    target_w: u32, // ref for portrait
}

// top down ladder. Only renditions <= source resolution are generated.
const RENDITION_LADDER: &[RenditionSpec] = &[
    RenditionSpec {
        label: "2160p",
        target_h: 2160,
        target_w: 3840,
    },
    RenditionSpec {
        label: "1080p",
        target_h: 1080,
        target_w: 1920,
    },
    RenditionSpec {
        label: "720p",
        target_h: 720,
        target_w: 1280,
    },
    RenditionSpec {
        label: "480p",
        target_h: 480,
        target_w: 854,
    },
    RenditionSpec {
        label: "360p",
        target_h: 360,
        target_w: 640,
    },
    RenditionSpec {
        label: "240p",
        target_h: 240,
        target_w: 426,
    },
    RenditionSpec {
        label: "144p",
        target_h: 144,
        target_w: 256,
    },
];

pub struct GenerateChildrenStep;

impl Step<NormalizedVideo, RenditionSet> for GenerateChildrenStep {
    fn run(&self, input: NormalizedVideo, ctx: &PipelineContext) -> Result<RenditionSet> {
        let is_portrait = input.height > input.width;

        let applicable: Vec<&RenditionSpec> = RENDITION_LADDER
            .iter()
            .filter(|r| {
                if is_portrait {
                    r.target_w <= input.width
                } else {
                    r.target_h <= input.height
                }
            })
            .collect();

        ctx.report(format!("Generating {} renditions...", applicable.len()));

        let mut renditions = Vec::new();

        for spec in applicable {
            let out_path = ctx.work_dir.join(format!("{}.mp4", spec.label));

            let is_same_res = if is_portrait {
                spec.target_w == input.width
            } else {
                spec.target_h == input.height
            };

            ctx.report(format!("Generating {} rendition...", spec.label));

            if is_same_res {
                fs::copy(&input.path, &out_path)?;
            } else {
                let scale_filter = if is_portrait {
                    format!("scale=w={}:h=-2", spec.target_w)
                } else {
                    format!("scale=w=-2:h={}", spec.target_h)
                };

                let status = Command::new("ffmpeg")
                    .args(["-i"])
                    .arg(&input.path)
                    .args([
                        "-c:v", "libx264",
                        "-crf", "23",
                        "-preset", "fast",
                        "-vf", &scale_filter,
                        "-an",
                        "-y",
                    ])
                    .arg(&out_path)
                    .status()?;

                if !status.success() {
                    return Err(anyhow!("ffmpeg failed generating {} rendition", spec.label));
                }
            }

            let (w, h) = probe_dimensions(&out_path)?;
            renditions.push(Rendition {
                path: out_path,
                label: spec.label.to_string(),
                width: w,
                height: h,
            });
        }

        let audio_path = if input.has_audio {
            let ap = ctx.work_dir.join("audio.mp4");
            ctx.report("Extracting audio track...");

            let status = Command::new("ffmpeg")
                .args(["-i"])
                .arg(&input.path)
                .args(["-vn", "-c:a", "aac", "-b:a", "192k", "-y"])
                .arg(&ap)
                .status()?;

            if !status.success() {
                return Err(anyhow!("ffmpeg audio extraction failed"));
            }

            Some(ap)
        } else {
            None
        };

        ctx.report(format!(
            "Children generation complete: {} renditions produced",
            renditions.len()
        ));

        Ok(RenditionSet {
            renditions,
            audio_path,
        })
    }
}
