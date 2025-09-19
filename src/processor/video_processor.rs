use std::process::Output;

use std::{path::Path, process::Command, sync::mpsc::Sender};

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
            "[START] Processing video id: {}, path: {}",
            media.id,
            media.filepath
        );

        log_msg!(
            info,
            "Running integrity and validation checks for media id {}",
            media.id
        );

        if let Err(err) = check_for_file_errors(&tx, media) {
            log_msg!(
                error,
                "[FAIL] Integrity check failed for '{}'. Error: {}",
                media.filepath,
                err
            );
            // send error back in tx and return media
        } else {
            log_msg!(
                info,
                "[OK] Video integrity check passed for '{}'",
                media.filepath
            );
        }

        Ok(())
    }
}

fn check_for_file_errors(tx: &Sender<ProcessMessage>, media: &Media) -> Result<(), String> {
    log_msg!(
        info,
        "Checking file existence and extension for '{}'",
        media.filepath
    );
    check_file(&media.filepath)?;
    log_msg!(info, "File '{}' passed basic validation", media.filepath);
    send_processing_message(&tx, "Started processing video".into(), media.id);
    log_msg!(
        debug,
        "Sent processing start message for media id {}",
        media.id
    );
    send_processing_message(
        &tx,
        "Decoding file to check if the file is corrupted or malformed".into(),
        media.id,
    );
    log_msg!(
        debug,
        "Sent decoding check message for media id {}",
        media.id
    );

    log_msg!(
        info,
        "Running ffmpeg integrity check for '{}'",
        media.filepath
    );
    match build_ffmpeg_integrity_check_command(&media.filepath).output() {
        Ok(output) => {
            if output.status.success() {
                log_msg!(
                    info,
                    "ffmpeg integrity check passed for '{}'",
                    media.filepath
                );
                return Ok(());
            }

            log_msg!(
                error,
                "ffmpeg integrity check failed for '{}'",
                media.filepath
            );
            return Err(format_media_processing_error(
                output,
                "Integrity check",
                &media.filepath,
            ));
        }
        Err(err) => {
            log_msg!(
                error,
                "Failed to execute ffmpeg for '{}'. Error: {err}",
                media.filepath
            );
            return Err(format!(
                "Failed to execute ffmpeg command binary. Error: {err}"
            ));
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

fn format_media_processing_error(output: Output, action: &str, filepath: &str) -> String {
    indoc! {
        "
        Action: {action}
        Filepath: {filepath}
        Stdout: \"{stdout}\"
        Stderr: \"{stderr}\"
        Exit Status: \"{exit_status}\"
        "
    }
    .replace("{action}", action)
    .replace("{filepath}", filepath)
    .replace(
        "{stdout}",
        &String::from_utf8(output.stdout)
            .unwrap_or_else(|_| "Unable to convert STDOUT to UTF-8 String".into()),
    )
    .replace(
        "{stderr}",
        &String::from_utf8(output.stderr)
            .unwrap_or_else(|_| "Unable to convert STDERR to UTF-8 String".into()),
    )
    .replace(
        "{exit_status}",
        &output
            .status
            .code()
            .map(|val| val.to_string())
            .unwrap_or_else(|| "Not present. Probably terminated by signal.".into()),
    )
}

fn send_processing_message(tx: &Sender<ProcessMessage>, content: String, media_id: u64) {
    log_msg!(
        debug,
        "Sending processing message for media_id {}: {}",
        media_id,
        content
    );
    if let Err(e) = tx.send(ProcessMessage {
        media_id: media_id as i32,
        m_type: ProcessMessageType::Processing(content),
    }) {
        log_msg!(
            error,
            "Failed to send processing message for media id {}: {}",
            media_id,
            e
        );
    }
}

fn check_file(base_file: &str) -> Result<(), String> {
    log_msg!(debug, "Checking file path: '{}'", base_file);
    let path = Path::new(base_file);

    if !path.exists() {
        log_msg!(error, "File '{}' does not exist", base_file);
        return Err(format!("The file '{}' does not exist.", base_file));
    }

    if !path.is_file() {
        log_msg!(
            error,
            "Path '{}' is not a file or is inaccessible",
            base_file
        );
        return Err(format!(
            "The file '{}' is not a file or is inaccessible due to permissions or broken links.",
            base_file
        ));
    }

    match path.to_str() {
        Some(_) => {}
        None => {
            log_msg!(error, "File '{}' is not a valid utf-8 path", base_file);
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
            log_msg!(
                error,
                "File '{}' does not have an allowed extension",
                base_file
            );
            return Err(format!(
                "The file '{}' does not have an allowed extension. Allowed: {:?}",
                base_file, ALLOWED_FORMATS
            ));
        }
    }

    log_msg!(debug, "File '{}' passed all checks", base_file);
    Ok(())
}
