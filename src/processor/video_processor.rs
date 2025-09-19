use std::process::Output;
use std::{path::Path, process::Command, sync::mpsc::Sender};

use anyhow::bail;
use indoc::indoc;

use crate::{
    jobs::Task,
    log_msg,
    processor::processor::{ProcessMessage, TaskMessageType, TaskProcessor},
};

const ALLOWED_FORMATS: [&'static str; 5] = ["mp4", "avi", "mkv", "mov", "webm"];

pub struct VideoProcessor;

struct ProcessingContext<'a> {
    tx: &'a Sender<ProcessMessage>,
    task: &'a Task,
}

impl<'a> ProcessingContext<'a> {
    fn new(tx: &'a Sender<ProcessMessage>, task: &'a Task) -> Self {
        Self { tx, task }
    }

    fn send_processing_message(&self, content: &str) {
        let message = content.to_string();
        log_msg!(info, "Task {}: {}", self.task.id, message);

        if let Err(err) = self.tx.send(ProcessMessage {
            task_id: self.task.id,
            m_type: TaskMessageType::Processing(message.clone()),
        }) {
            log_msg!(
                error,
                "Failed to send processing message for task id {}: {}",
                self.task.id,
                err
            );
        }
    }

    fn send_error(&self, content: &str) {
        let message = content.to_string();
        log_msg!(error, "Task {} error: {}", self.task.id, message);

        if let Err(err) = self.tx.send(ProcessMessage {
            task_id: self.task.id,
            m_type: TaskMessageType::Failed(message.clone()),
        }) {
            log_msg!(
                error,
                "Failed to send error message for task id {}: {}",
                self.task.id,
                err
            );
        }
    }

    fn send_debug(&self, content: &str) {
        log_msg!(debug, "Task {}: {}", self.task.id, content);
    }
}

impl TaskProcessor for VideoProcessor {
    fn process_task(
        &self,
        task: &Task,
        tx: std::sync::mpsc::Sender<super::processor::ProcessMessage>,
    ) -> anyhow::Result<()> {
        let context = ProcessingContext::new(&tx, task);

        context.send_processing_message("Starting video processing");
        context.send_debug(&format!("Processing video path: {}", task.filepath));

        context.send_processing_message("Running integrity and validation checks");
        if let Err(err) = check_for_file_errors(&context) {
            context.send_error(&format!("Integrity check failed: {}", err));
            bail!("Found errors on task {}. Error: {}", task.id, err)
        }

        context.send_processing_message("Video processing completed successfully");
        Ok(())
    }
}

fn check_for_file_errors(context: &ProcessingContext) -> Result<(), String> {
    context.send_processing_message("Checking file existence and extension");
    check_file(&context.task.filepath)?;

    context.send_processing_message("File passed basic validation");
    context.send_processing_message("Decoding file to check for corruption");

    context.send_processing_message("Running FFmpeg integrity check");
    match build_ffmpeg_integrity_check_command(&context.task.filepath).output() {
        Ok(output) => {
            if output.status.success() {
                context.send_processing_message("FFmpeg integrity check passed");
                return Ok(());
            }

            let error_msg =
                format_task_processing_error(output, "Integrity check", &context.task.filepath);
            context.send_debug(&format!("FFmpeg check failed: {}", error_msg));
            Err(error_msg)
        }
        Err(err) => {
            let error_msg = format!("Failed to execute ffmpeg command. Error: {}", err);
            context.send_debug(&error_msg);
            Err(error_msg)
        }
    }
}

fn build_ffmpeg_integrity_check_command(base_file: &str) -> Command {
    let mut cmd = Command::new("ffmpeg");
    cmd.arg("-v")
        .arg("error")
        .arg("-i")
        .arg(base_file)
        .arg("-f")
        .arg("null")
        .arg("-");
    cmd
}

fn format_task_processing_error(output: Output, action: &str, filepath: &str) -> String {
    indoc! {"
        Action: {action}
        Filepath: {filepath}
        Stdout: \"{stdout}\"
        Stderr: \"{stderr}\"
        Exit Status: \"{exit_status}\"
    "}
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

    if path.to_str().is_none() {
        return Err(format!(
            "The file '{}' is not a valid utf-8 file.",
            base_file
        ));
    }

    let ext = path.extension().and_then(|e| e.to_str());
    match ext {
        Some(ext_str) if ALLOWED_FORMATS.contains(&ext_str) => Ok(()),
        _ => Err(format!(
            "The file '{}' does not have an allowed extension. Allowed: {:?}",
            base_file, ALLOWED_FORMATS
        )),
    }
}
