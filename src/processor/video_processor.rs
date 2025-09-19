use std::{fmt::format, path::Path, process::Command, sync::mpsc::Sender};

use indoc::indoc;

use crate::{
    log_msg,
    processor::processor::{MediaProcessor, ProcessMessage, ProcessMessageType},
    tasks::Media,
};

const ALLOWED_FORMATS: [&'static str; 5] = ["mp4", "avi", "mkv", "mov", "webm"];

pub struct VideoProcessor;

impl MediaProcessor for VideoProcessor {
    fn process_media<'m>(
        &self,
        media: &'m Media,
        tx: std::sync::mpsc::Sender<super::processor::ProcessMessage>,
    ) -> anyhow::Result<(), &'m Media> {
        log_msg!(
            info,
            "Started processing video id: {}, path: {}",
            media.id,
            media.filepath
        );

        if let Err(err) = check_for_file_errors(&tx, media) {
            log_msg!(error, "Error while checking media with path '{}' integrity. Error: {}", media.filepath, err)
            // send error back in tx and return media
        } else {
            log_msg!(info, "Error checking on media with path '{}' occured well.", media.filepath)
        } 

        Ok(())
    }
}

fn check_for_file_errors(tx: &Sender<ProcessMessage>, media: &Media) -> Result<(), String> {
    check_file(&media.filepath)?;
    send_processing_message(&tx, "Started processing video".into(), media.id);
    send_processing_message(
        &tx,
        "Decoding file to check if the file is corrupted or malformed".into(),
        media.id,
    );

    match build_ffmpeg_integrity_check_command(&media.filepath).output() {
        Ok(output) => {
            if output.status.success() {
                log_msg!(debug, "Video integrity check passed sucessfully");
                return Ok(());
            }

            log_msg!(debug, "Video integrity check failed");
            return Err(format!(
                indoc! {"
            MPE failed while checking for errors on media with path \"{}\"
            Stdout: \"{}\"
            Stderr: \"{}\"
            Exit Status: \"{}\"
            "},
                media.filepath,
                String::from_utf8(output.stdout)
                    .unwrap_or_else(|_| "Unable to convert STDOUT to UTF-8 String".into()),
                String::from_utf8(output.stderr)
                    .unwrap_or_else(|_| "Unable to convert STDERR to UTF-8 String".into()),
                output
                    .status
                    .code()
                    .and_then(|val| Some(val.to_string()))
                    .unwrap_or_else(|| "Not present. Probably terminated by signal.".into())
            ));
        }
        Err(err) => {
            log_msg!(debug, "Failed to execute FFMPEG command. Error: {err}");
            return Err(format!("Failed to execute FFMPEG command binary. Error: {err}"));
        }
    }
}

fn build_ffmpeg_integrity_check_command(base_file: &str) -> Command {
    // ffmpeg -v error -i file.mp4 -f null -

    let mut cmd = Command::new("ffmpeg");
    cmd.arg("-v")
        .arg("error")
        .arg(base_file)
        .arg("-f")
        .arg("null")
        .arg("-");

    cmd
}

fn send_processing_message(tx: &Sender<ProcessMessage>, content: String, media_id: u64) {
    if let Err(e) = tx.send(ProcessMessage {
        media_id: media_id as i32,
        m_type: ProcessMessageType::Processing(content),
    }) {
        log_msg!(
            error,
            "Failed to send processing message for media id {media_id}: {e}"
        );
    }
}

fn check_file(base_file: &str) -> Result<(), String> {
    let path = Path::new(base_file);

    if !path.exists() {
        return Err(format!("The file '{}' does not exist.", base_file));
    }

    if !path.is_file() {
        return Err(format!(
            "The file '{}' is not a file or is inaccessible due to permissions or broken links.",
            base_file
        ));
    }

    match path.to_str() {
        Some(s) => s,
        None => {
            return Err(format!(
                "The file '{}' is not a valid utf-8 file.",
                base_file
            ));
        }
    };

    let ext = path.extension().and_then(|e| e.to_str());
    match ext {
        Some(ext_str) if ALLOWED_FORMATS.contains(&ext_str) => {}
        _ => {
            return Err(format!(
                "The file '{}' does not have an allowed extension. Allowed: {:?}",
                base_file, ALLOWED_FORMATS
            ));
        }
    }

    Ok(())
}
