use std::collections::HashSet;
use std::sync::mpsc::Receiver;

use anyhow::Result;
use chrono::{DateTime, Utc};
use redis::AsyncCommands;
use redis::aio::MultiplexedConnection;
use serde::{Deserialize, Serialize};
use tokio::runtime::Handle;

use crate::processor::{ProcessMessage, TaskMessageType};

const TTL_SECS: u64 = 86400; // 24 Hours

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum JobStatusKind {
    Queued,
    Processing,
    Completed,
    Failed,
    PartiallyFailed,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TaskStatusKind {
    Processing,
    Finished,
    Failed,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct JobStatus {
    #[serde(rename = "status")]
    pub kind: JobStatusKind,
    pub total_tasks: u64,
    pub completed_tasks: u64,
    pub failed_tasks: u64,
    pub started_at: Option<DateTime<Utc>>,
    pub finished_at: Option<DateTime<Utc>>,
    pub error: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct TaskStatus {
    #[serde(rename = "status")]
    pub kind: TaskStatusKind,
    pub step: Option<String>,
    pub message: Option<String>,
    pub error: Option<String>,
    pub updated_at: DateTime<Utc>,
}

pub struct StatusForwarder {
    job_id: String,
    total_tasks: u64,
    redis: MultiplexedConnection,
}

impl StatusForwarder {
    pub async fn new(
        job_id: String,
        total_tasks: u64,
        redis: MultiplexedConnection,
    ) -> Result<Self> {
        Ok(Self {
            job_id,
            total_tasks,
            redis,
        })
    }

    pub async fn init(&self) -> Result<()> {
        let status = JobStatus {
            kind: JobStatusKind::Queued,
            total_tasks: self.total_tasks,
            completed_tasks: 0,
            failed_tasks: 0,
            started_at: None,
            finished_at: None,
            error: None,
        };
        write_job_key(&mut self.redis.clone(), &self.job_id, &status).await
    }

    pub fn run(self, rx: Receiver<ProcessMessage>) {
        let handle = Handle::current();
        let mut redis = self.redis;
        let job_id = self.job_id;
        let total_tasks = self.total_tasks;

        let mut settled: HashSet<u64> = HashSet::new();
        let mut completed: u64 = 0;
        let mut failed: u64 = 0;
        let mut started_at: Option<DateTime<Utc>> = None;

        if total_tasks == 0 {
            let status = JobStatus {
                kind: JobStatusKind::Completed,
                total_tasks: 0,
                completed_tasks: 0,
                failed_tasks: 0,
                started_at: None,
                finished_at: Some(Utc::now()),
                error: None,
            };
            handle
                .block_on(write_job_key(&mut redis, &job_id, &status))
                .expect("Redis write failed");
            return;
        }

        for ProcessMessage { task_id, m_type } in rx {
            match m_type {
                TaskMessageType::Processing { step, message } => {
                    if settled.contains(&task_id) {
                        continue;
                    }
                    let task = TaskStatus {
                        kind: TaskStatusKind::Processing,
                        step: Some(step),
                        message: Some(message),
                        error: None,
                        updated_at: Utc::now(),
                    };
                    handle
                        .block_on(write_task_key(&mut redis, &job_id, task_id, &task))
                        .expect("Redis write failed");

                    if started_at.is_none() {
                        started_at = Some(Utc::now());
                        let job = JobStatus {
                            kind: JobStatusKind::Processing,
                            total_tasks,
                            completed_tasks: completed,
                            failed_tasks: failed,
                            started_at,
                            finished_at: None,
                            error: None,
                        };
                        handle
                            .block_on(write_job_key(&mut redis, &job_id, &job))
                            .expect("Redis write failed");
                    }
                }

                TaskMessageType::Finished => {
                    if settled.contains(&task_id) {
                        continue;
                    }
                    settled.insert(task_id);
                    completed += 1;

                    let task = TaskStatus {
                        kind: TaskStatusKind::Finished,
                        step: None,
                        message: None,
                        error: None,
                        updated_at: Utc::now(),
                    };
                    handle
                        .block_on(write_task_key(&mut redis, &job_id, task_id, &task))
                        .expect("Redis write failed");

                    let job = if completed + failed == total_tasks {
                        JobStatus {
                            kind: terminal_job_kind(completed, failed),
                            total_tasks,
                            completed_tasks: completed,
                            failed_tasks: failed,
                            started_at,
                            finished_at: Some(Utc::now()),
                            error: None,
                        }
                    } else {
                        JobStatus {
                            kind: JobStatusKind::Processing,
                            total_tasks,
                            completed_tasks: completed,
                            failed_tasks: failed,
                            started_at,
                            finished_at: None,
                            error: None,
                        }
                    };
                    handle
                        .block_on(write_job_key(&mut redis, &job_id, &job))
                        .expect("Redis write failed");
                }

                TaskMessageType::Failed(err) => {
                    if settled.contains(&task_id) {
                        continue;
                    }
                    settled.insert(task_id);
                    failed += 1;

                    let task = TaskStatus {
                        kind: TaskStatusKind::Failed,
                        step: None,
                        message: None,
                        error: Some(err),
                        updated_at: Utc::now(),
                    };
                    handle
                        .block_on(write_task_key(&mut redis, &job_id, task_id, &task))
                        .expect("Redis write failed");

                    let job = if completed + failed == total_tasks {
                        JobStatus {
                            kind: terminal_job_kind(completed, failed),
                            total_tasks,
                            completed_tasks: completed,
                            failed_tasks: failed,
                            started_at,
                            finished_at: Some(Utc::now()),
                            error: None,
                        }
                    } else {
                        JobStatus {
                            kind: JobStatusKind::Processing,
                            total_tasks,
                            completed_tasks: completed,
                            failed_tasks: failed,
                            started_at,
                            finished_at: None,
                            error: None,
                        }
                    };
                    handle
                        .block_on(write_job_key(&mut redis, &job_id, &job))
                        .expect("Redis write failed");
                }
            }
        }
    }

    pub async fn write_pre_dispatch_failure(
        job_id: &str,
        total_tasks: u64,
        error: &str,
        redis: &mut MultiplexedConnection,
    ) -> Result<()> {
        let status = JobStatus {
            kind: JobStatusKind::Failed,
            total_tasks,
            completed_tasks: 0,
            failed_tasks: 0,
            started_at: None,
            finished_at: Some(Utc::now()),
            error: Some(error.to_string()),
        };
        write_job_key(redis, job_id, &status).await
    }
}

fn terminal_job_kind(completed: u64, failed: u64) -> JobStatusKind {
    match (completed, failed) {
        (_, 0) => JobStatusKind::Completed,
        (0, _) => JobStatusKind::Failed,
        _ => JobStatusKind::PartiallyFailed,
    }
}

async fn write_job_key(
    redis: &mut MultiplexedConnection,
    job_id: &str,
    status: &JobStatus,
) -> Result<()> {
    let value = serde_json::to_string(status)?;
    redis
        .set_ex::<_, _, ()>(format!("job:{job_id}"), value, TTL_SECS)
        .await?;
    Ok(())
}

async fn write_task_key(
    redis: &mut MultiplexedConnection,
    job_id: &str,
    task_id: u64,
    status: &TaskStatus,
) -> Result<()> {
    let value = serde_json::to_string(status)?;
    redis
        .set_ex::<_, _, ()>(format!("job:{job_id}:task:{task_id}"), value, TTL_SECS)
        .await?;
    Ok(())
}
