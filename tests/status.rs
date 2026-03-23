use std::sync::mpsc::{Sender, channel};

use mpe::processor::{
    ProcessMessage, TaskMessageType,
    status::{JobStatus, StatusForwarder, TaskStatus},
};
use redis::AsyncCommands;
use uuid::Uuid;

async fn connect() -> redis::aio::MultiplexedConnection {
    let client =
        redis::Client::open("redis://127.0.0.1:6379").expect("Redis client creation failed");
    client
        .get_multiplexed_async_connection()
        .await
        .expect("Redis connection failed — is the Redis container running?")
}

fn job_id() -> String {
    format!("test-{}", Uuid::new_v4())
}

async fn cleanup(conn: &mut redis::aio::MultiplexedConnection, pattern: &str) {
    let keys: Vec<String> = redis::cmd("KEYS")
        .arg(pattern)
        .query_async(conn)
        .await
        .unwrap_or_default();
    for key in keys {
        let _: () = conn.del(&key).await.unwrap_or(());
    }
}

async fn read_job(conn: &mut redis::aio::MultiplexedConnection, job_id: &str) -> JobStatus {
    let raw: String = conn
        .get(format!("job:{job_id}"))
        .await
        .expect("job key not found");
    serde_json::from_str(&raw).expect("failed to deserialize JobStatus")
}

async fn read_task(
    conn: &mut redis::aio::MultiplexedConnection,
    job_id: &str,
    task_id: u64,
) -> TaskStatus {
    let raw: String = conn
        .get(format!("job:{job_id}:task:{task_id}"))
        .await
        .expect("task key not found");
    serde_json::from_str(&raw).expect("failed to deserialize TaskStatus")
}

async fn drive(forwarder: StatusForwarder, f: impl FnOnce(Sender<ProcessMessage>)) {
    let (tx, rx) = channel();
    f(tx);
    tokio::task::spawn_blocking(move || forwarder.run(rx))
        .await
        .expect("forwarder thread panicked");
}

mod job_lifecycle {
    use mpe::processor::status::JobStatusKind;

    use super::*;

    #[tokio::test]
    async fn written_as_queued_before_dispatch() {
        let mut conn = connect().await;
        let id = job_id();

        let forwarder = StatusForwarder::new(id.clone(), 1, conn.clone())
            .await
            .unwrap();
        forwarder.init().await.unwrap();

        let status = read_job(&mut conn, &id).await;
        assert!(matches!(status.kind, JobStatusKind::Queued));

        cleanup(&mut conn, &format!("job:{id}*")).await;
    }

    #[tokio::test]
    async fn transitions_to_processing_on_first_task() {
        let mut conn = connect().await;
        let id = job_id();

        let forwarder = StatusForwarder::new(id.clone(), 1, conn.clone())
            .await
            .unwrap();
        forwarder.init().await.unwrap();

        drive(forwarder, |tx| {
            tx.send(ProcessMessage {
                task_id: 1,
                m_type: TaskMessageType::Processing {
                    step: "step".into(),
                    message: "step".into(),
                },
            })
            .unwrap();
        })
        .await;

        let status = read_job(&mut conn, &id).await;
        assert!(matches!(
            status.kind,
            JobStatusKind::Processing | JobStatusKind::Completed
        ));

        cleanup(&mut conn, &format!("job:{id}*")).await;
    }

    #[tokio::test]
    async fn transitions_to_completed_when_all_tasks_finish() {
        let mut conn = connect().await;
        let id = job_id();

        let forwarder = StatusForwarder::new(id.clone(), 2, conn.clone())
            .await
            .unwrap();
        forwarder.init().await.unwrap();

        drive(forwarder, |tx| {
            tx.send(ProcessMessage {
                task_id: 1,
                m_type: TaskMessageType::Finished,
            })
            .unwrap();
            tx.send(ProcessMessage {
                task_id: 2,
                m_type: TaskMessageType::Finished,
            })
            .unwrap();
        })
        .await;

        let status = read_job(&mut conn, &id).await;
        assert!(matches!(status.kind, JobStatusKind::Completed));
        assert_eq!(status.completed_tasks, 2);
        assert_eq!(status.failed_tasks, 0);

        cleanup(&mut conn, &format!("job:{id}*")).await;
    }

    #[tokio::test]
    async fn transitions_to_failed_when_all_tasks_fail() {
        let mut conn = connect().await;
        let id = job_id();

        let forwarder = StatusForwarder::new(id.clone(), 2, conn.clone())
            .await
            .unwrap();
        forwarder.init().await.unwrap();

        drive(forwarder, |tx| {
            tx.send(ProcessMessage {
                task_id: 1,
                m_type: TaskMessageType::Failed("err".into()),
            })
            .unwrap();
            tx.send(ProcessMessage {
                task_id: 2,
                m_type: TaskMessageType::Failed("err".into()),
            })
            .unwrap();
        })
        .await;

        let status = read_job(&mut conn, &id).await;
        assert!(matches!(status.kind, JobStatusKind::Failed));
        assert_eq!(status.failed_tasks, 2);
        assert_eq!(status.completed_tasks, 0);

        cleanup(&mut conn, &format!("job:{id}*")).await;
    }

    #[tokio::test]
    async fn transitions_to_partially_failed() {
        let mut conn = connect().await;
        let id = job_id();

        let forwarder = StatusForwarder::new(id.clone(), 2, conn.clone())
            .await
            .unwrap();
        forwarder.init().await.unwrap();

        drive(forwarder, |tx| {
            tx.send(ProcessMessage {
                task_id: 1,
                m_type: TaskMessageType::Finished,
            })
            .unwrap();
            tx.send(ProcessMessage {
                task_id: 2,
                m_type: TaskMessageType::Failed("err".into()),
            })
            .unwrap();
        })
        .await;

        let status = read_job(&mut conn, &id).await;
        assert!(matches!(status.kind, JobStatusKind::PartiallyFailed));
        assert_eq!(status.completed_tasks, 1);
        assert_eq!(status.failed_tasks, 1);

        cleanup(&mut conn, &format!("job:{id}*")).await;
    }

    #[tokio::test]
    async fn total_tasks_matches_dispatched_count() {
        let mut conn = connect().await;
        let id = job_id();

        let forwarder = StatusForwarder::new(id.clone(), 5, conn.clone())
            .await
            .unwrap();
        forwarder.init().await.unwrap();

        let status = read_job(&mut conn, &id).await;
        assert_eq!(status.total_tasks, 5);

        cleanup(&mut conn, &format!("job:{id}*")).await;
    }

    #[tokio::test]
    async fn completed_plus_failed_equals_total_at_terminal() {
        let mut conn = connect().await;
        let id = job_id();

        let forwarder = StatusForwarder::new(id.clone(), 3, conn.clone())
            .await
            .unwrap();
        forwarder.init().await.unwrap();

        drive(forwarder, |tx| {
            tx.send(ProcessMessage {
                task_id: 1,
                m_type: TaskMessageType::Finished,
            })
            .unwrap();
            tx.send(ProcessMessage {
                task_id: 2,
                m_type: TaskMessageType::Failed("err".into()),
            })
            .unwrap();
            tx.send(ProcessMessage {
                task_id: 3,
                m_type: TaskMessageType::Finished,
            })
            .unwrap();
        })
        .await;

        let status = read_job(&mut conn, &id).await;
        assert_eq!(
            status.completed_tasks + status.failed_tasks,
            status.total_tasks
        );

        cleanup(&mut conn, &format!("job:{id}*")).await;
    }

    #[tokio::test]
    async fn finished_at_set_at_terminal() {
        let mut conn = connect().await;
        let id = job_id();

        let forwarder = StatusForwarder::new(id.clone(), 1, conn.clone())
            .await
            .unwrap();
        forwarder.init().await.unwrap();

        drive(forwarder, |tx| {
            tx.send(ProcessMessage {
                task_id: 1,
                m_type: TaskMessageType::Finished,
            })
            .unwrap();
        })
        .await;

        let status = read_job(&mut conn, &id).await;
        assert!(status.finished_at.is_some());

        cleanup(&mut conn, &format!("job:{id}*")).await;
    }
}

mod task_lifecycle {
    use mpe::processor::status::{StatusForwarder, TaskStatusKind};

    use super::*;

    #[tokio::test]
    async fn written_with_processing_on_first_report() {
        let mut conn = connect().await;
        let id = job_id();

        let forwarder = StatusForwarder::new(id.clone(), 1, conn.clone())
            .await
            .unwrap();
        forwarder.init().await.unwrap();

        drive(forwarder, |tx| {
            tx.send(ProcessMessage {
                task_id: 1,
                m_type: TaskMessageType::Processing {
                    step: "step".into(),
                    message: "Validating".into(),
                },
            })
            .unwrap();
        })
        .await;

        let task = read_task(&mut conn, &id, 1).await;
        assert!(matches!(task.kind, TaskStatusKind::Processing));

        cleanup(&mut conn, &format!("job:{id}*")).await;
    }

    #[tokio::test]
    async fn step_field_reflects_current_pipeline_step() {
        let mut conn = connect().await;
        let id = job_id();

        let forwarder = StatusForwarder::new(id.clone(), 1, conn.clone())
            .await
            .unwrap();
        forwarder.init().await.unwrap();

        drive(forwarder, |tx| {
            tx.send(ProcessMessage {
                task_id: 1,
                m_type: TaskMessageType::Processing {
                    step: "Packaging".into(),
                    message: "Packaging streams...".into(),
                },
            })
            .unwrap();
        })
        .await;

        let task = read_task(&mut conn, &id, 1).await;
        assert_eq!(task.step.as_deref(), Some("Packaging"));
        assert_eq!(task.message.as_deref(), Some("Packaging streams..."));

        cleanup(&mut conn, &format!("job:{id}*")).await;
    }

    #[tokio::test]
    async fn step_field_is_null_on_finished() {
        let mut conn = connect().await;
        let id = job_id();

        let forwarder = StatusForwarder::new(id.clone(), 1, conn.clone())
            .await
            .unwrap();
        forwarder.init().await.unwrap();

        drive(forwarder, |tx| {
            tx.send(ProcessMessage {
                task_id: 1,
                m_type: TaskMessageType::Finished,
            })
            .unwrap();
        })
        .await;

        let task = read_task(&mut conn, &id, 1).await;
        assert!(task.step.is_none());

        cleanup(&mut conn, &format!("job:{id}*")).await;
    }

    #[tokio::test]
    async fn step_field_is_null_on_failed() {
        let mut conn = connect().await;
        let id = job_id();

        let forwarder = StatusForwarder::new(id.clone(), 1, conn.clone())
            .await
            .unwrap();
        forwarder.init().await.unwrap();

        drive(forwarder, |tx| {
            tx.send(ProcessMessage {
                task_id: 1,
                m_type: TaskMessageType::Failed("err".into()),
            })
            .unwrap();
        })
        .await;

        let task = read_task(&mut conn, &id, 1).await;
        assert!(task.step.is_none());

        cleanup(&mut conn, &format!("job:{id}*")).await;
    }

    #[tokio::test]
    async fn updated_at_is_set_on_write() {
        use chrono::Utc;

        let mut conn = connect().await;
        let id = job_id();

        let forwarder = StatusForwarder::new(id.clone(), 1, conn.clone())
            .await
            .unwrap();
        forwarder.init().await.unwrap();

        let before = Utc::now();

        drive(forwarder, |tx| {
            tx.send(ProcessMessage {
                task_id: 1,
                m_type: TaskMessageType::Processing {
                    step: "Validating".into(),
                    message: "checking...".into(),
                },
            })
            .unwrap();
        })
        .await;

        let after = Utc::now();
        let task = read_task(&mut conn, &id, 1).await;
        assert!(task.updated_at >= before, "updated_at should be >= before");
        assert!(task.updated_at <= after, "updated_at should be <= after");

        cleanup(&mut conn, &format!("job:{id}*")).await;
    }

    #[tokio::test]
    async fn transitions_to_finished() {
        let mut conn = connect().await;
        let id = job_id();

        let forwarder = StatusForwarder::new(id.clone(), 1, conn.clone())
            .await
            .unwrap();
        forwarder.init().await.unwrap();

        drive(forwarder, |tx| {
            tx.send(ProcessMessage {
                task_id: 1,
                m_type: TaskMessageType::Processing {
                    step: "step".into(),
                    message: "step".into(),
                },
            })
            .unwrap();
            tx.send(ProcessMessage {
                task_id: 1,
                m_type: TaskMessageType::Finished,
            })
            .unwrap();
        })
        .await;

        let task = read_task(&mut conn, &id, 1).await;
        assert!(matches!(task.kind, TaskStatusKind::Finished));
        assert!(task.error.is_none());

        cleanup(&mut conn, &format!("job:{id}*")).await;
    }

    #[tokio::test]
    async fn transitions_to_failed_with_error_message() {
        let mut conn = connect().await;
        let id = job_id();

        let forwarder = StatusForwarder::new(id.clone(), 1, conn.clone())
            .await
            .unwrap();
        forwarder.init().await.unwrap();

        drive(forwarder, |tx| {
            tx.send(ProcessMessage {
                task_id: 1,
                m_type: TaskMessageType::Failed("codec error".into()),
            })
            .unwrap();
        })
        .await;

        let task = read_task(&mut conn, &id, 1).await;
        assert!(matches!(task.kind, TaskStatusKind::Failed));
        assert_eq!(task.error.as_deref(), Some("codec error"));

        cleanup(&mut conn, &format!("job:{id}*")).await;
    }

    #[tokio::test]
    async fn terminal_state_not_overwritten_by_processing() {
        let mut conn = connect().await;
        let id = job_id();

        let forwarder = StatusForwarder::new(id.clone(), 1, conn.clone())
            .await
            .unwrap();
        forwarder.init().await.unwrap();

        drive(forwarder, |tx| {
            tx.send(ProcessMessage {
                task_id: 1,
                m_type: TaskMessageType::Finished,
            })
            .unwrap();
            tx.send(ProcessMessage {
                task_id: 1,
                m_type: TaskMessageType::Processing {
                    step: "step".into(),
                    message: "late step".into(),
                },
            })
            .unwrap();
        })
        .await;

        let task = read_task(&mut conn, &id, 1).await;
        assert!(matches!(task.kind, TaskStatusKind::Finished));

        cleanup(&mut conn, &format!("job:{id}*")).await;
    }
}

mod ttl {
    use super::*;

    #[tokio::test]
    async fn task_key_has_ttl_on_write() {
        let mut conn = connect().await;
        let id = job_id();

        let forwarder = StatusForwarder::new(id.clone(), 1, conn.clone())
            .await
            .unwrap();
        forwarder.init().await.unwrap();

        drive(forwarder, |tx| {
            tx.send(ProcessMessage {
                task_id: 1,
                m_type: TaskMessageType::Processing {
                    step: "step".into(),
                    message: "step".into(),
                },
            })
            .unwrap();
        })
        .await;

        let ttl: i64 = redis::cmd("TTL")
            .arg(format!("job:{id}:task:1"))
            .query_async(&mut conn)
            .await
            .unwrap();

        assert!(ttl > 0, "expected TTL to be set, got {ttl}");
        assert!(ttl <= 86400, "expected TTL <= 24h, got {ttl}");

        cleanup(&mut conn, &format!("job:{id}*")).await;
    }

    #[tokio::test]
    async fn job_key_has_ttl_on_write() {
        let mut conn = connect().await;
        let id = job_id();

        let forwarder = StatusForwarder::new(id.clone(), 1, conn.clone())
            .await
            .unwrap();
        forwarder.init().await.unwrap();

        let ttl: i64 = redis::cmd("TTL")
            .arg(format!("job:{id}"))
            .query_async(&mut conn)
            .await
            .unwrap();

        assert!(ttl > 0, "expected TTL to be set, got {ttl}");
        assert!(ttl <= 86400, "expected TTL <= 24h, got {ttl}");

        cleanup(&mut conn, &format!("job:{id}*")).await;
    }

    #[tokio::test]
    async fn ttl_reset_on_processing_writes() {
        let mut conn = connect().await;
        let id = job_id();

        let forwarder = StatusForwarder::new(id.clone(), 1, conn.clone())
            .await
            .unwrap();
        forwarder.init().await.unwrap();

        drive(forwarder, |tx| {
            tx.send(ProcessMessage {
                task_id: 1,
                m_type: TaskMessageType::Processing {
                    step: "step".into(),
                    message: "step 1".into(),
                },
            })
            .unwrap();
            tx.send(ProcessMessage {
                task_id: 1,
                m_type: TaskMessageType::Processing {
                    step: "step".into(),
                    message: "step 2".into(),
                },
            })
            .unwrap();
        })
        .await;

        let ttl: i64 = redis::cmd("TTL")
            .arg(format!("job:{id}:task:1"))
            .query_async(&mut conn)
            .await
            .unwrap();

        assert!(ttl > 0);

        cleanup(&mut conn, &format!("job:{id}*")).await;
    }
}

mod edge_cases {
    use mpe::processor::status::JobStatusKind;

    use super::*;

    #[tokio::test]
    async fn zero_tasks_completes_immediately() {
        let mut conn = connect().await;
        let id = job_id();

        let forwarder = StatusForwarder::new(id.clone(), 0, conn.clone())
            .await
            .unwrap();
        forwarder.init().await.unwrap();

        drive(forwarder, |_tx| {}).await;

        let status = read_job(&mut conn, &id).await;
        assert!(matches!(status.kind, JobStatusKind::Completed));

        cleanup(&mut conn, &format!("job:{id}*")).await;
    }

    #[tokio::test]
    async fn two_jobs_do_not_interfere() {
        let mut conn = connect().await;
        let id_a = job_id();
        let id_b = job_id();

        let forwarder_a = StatusForwarder::new(id_a.clone(), 1, conn.clone())
            .await
            .unwrap();
        let forwarder_b = StatusForwarder::new(id_b.clone(), 1, conn.clone())
            .await
            .unwrap();

        forwarder_a.init().await.unwrap();
        forwarder_b.init().await.unwrap();

        drive(forwarder_a, |tx| {
            tx.send(ProcessMessage {
                task_id: 1,
                m_type: TaskMessageType::Finished,
            })
            .unwrap();
        })
        .await;

        drive(forwarder_b, |tx| {
            tx.send(ProcessMessage {
                task_id: 1,
                m_type: TaskMessageType::Failed("err".into()),
            })
            .unwrap();
        })
        .await;

        let status_a = read_job(&mut conn, &id_a).await;
        let status_b = read_job(&mut conn, &id_b).await;

        assert!(matches!(status_a.kind, JobStatusKind::Completed));
        assert!(matches!(status_b.kind, JobStatusKind::Failed));

        cleanup(&mut conn, &format!("job:{id_a}*")).await;
        cleanup(&mut conn, &format!("job:{id_b}*")).await;
    }
}

mod state_transition_guards {
    use mpe::processor::status::TaskStatusKind;

    use super::*;

    #[tokio::test]
    async fn finished_not_overwritten_by_failed() {
        let mut conn = connect().await;
        let id = job_id();

        let forwarder = StatusForwarder::new(id.clone(), 1, conn.clone())
            .await
            .unwrap();
        forwarder.init().await.unwrap();

        drive(forwarder, |tx| {
            tx.send(ProcessMessage {
                task_id: 1,
                m_type: TaskMessageType::Finished,
            })
            .unwrap();
            tx.send(ProcessMessage {
                task_id: 1,
                m_type: TaskMessageType::Failed("late error".into()),
            })
            .unwrap();
        })
        .await;

        let task = read_task(&mut conn, &id, 1).await;
        assert!(matches!(task.kind, TaskStatusKind::Finished));

        cleanup(&mut conn, &format!("job:{id}*")).await;
    }

    #[tokio::test]
    async fn failed_not_overwritten_by_finished() {
        let mut conn = connect().await;
        let id = job_id();

        let forwarder = StatusForwarder::new(id.clone(), 1, conn.clone())
            .await
            .unwrap();
        forwarder.init().await.unwrap();

        drive(forwarder, |tx| {
            tx.send(ProcessMessage {
                task_id: 1,
                m_type: TaskMessageType::Failed("err".into()),
            })
            .unwrap();
            tx.send(ProcessMessage {
                task_id: 1,
                m_type: TaskMessageType::Finished,
            })
            .unwrap();
        })
        .await;

        let task = read_task(&mut conn, &id, 1).await;
        assert!(matches!(task.kind, TaskStatusKind::Failed));

        cleanup(&mut conn, &format!("job:{id}*")).await;
    }

    #[tokio::test]
    async fn failed_not_overwritten_by_processing() {
        let mut conn = connect().await;
        let id = job_id();

        let forwarder = StatusForwarder::new(id.clone(), 1, conn.clone())
            .await
            .unwrap();
        forwarder.init().await.unwrap();

        drive(forwarder, |tx| {
            tx.send(ProcessMessage {
                task_id: 1,
                m_type: TaskMessageType::Failed("err".into()),
            })
            .unwrap();
            tx.send(ProcessMessage {
                task_id: 1,
                m_type: TaskMessageType::Processing {
                    step: "step".into(),
                    message: "late".into(),
                },
            })
            .unwrap();
        })
        .await;

        let task = read_task(&mut conn, &id, 1).await;
        assert!(matches!(task.kind, TaskStatusKind::Failed));

        cleanup(&mut conn, &format!("job:{id}*")).await;
    }
}

mod task_isolation {
    use mpe::processor::status::{JobStatusKind, TaskStatusKind};

    use super::*;

    #[tokio::test]
    async fn failed_task_does_not_affect_sibling_status() {
        let mut conn = connect().await;
        let id = job_id();

        let forwarder = StatusForwarder::new(id.clone(), 2, conn.clone())
            .await
            .unwrap();
        forwarder.init().await.unwrap();

        drive(forwarder, |tx| {
            tx.send(ProcessMessage {
                task_id: 1,
                m_type: TaskMessageType::Failed("err".into()),
            })
            .unwrap();
            tx.send(ProcessMessage {
                task_id: 2,
                m_type: TaskMessageType::Processing {
                    step: "step".into(),
                    message: "step".into(),
                },
            })
            .unwrap();
            tx.send(ProcessMessage {
                task_id: 2,
                m_type: TaskMessageType::Finished,
            })
            .unwrap();
        })
        .await;

        let task2 = read_task(&mut conn, &id, 2).await;
        assert!(matches!(task2.kind, TaskStatusKind::Finished));

        cleanup(&mut conn, &format!("job:{id}*")).await;
    }

    #[tokio::test]
    async fn failed_task_does_not_prevent_sibling_from_finishing() {
        let mut conn = connect().await;
        let id = job_id();

        let forwarder = StatusForwarder::new(id.clone(), 2, conn.clone())
            .await
            .unwrap();
        forwarder.init().await.unwrap();

        drive(forwarder, |tx| {
            tx.send(ProcessMessage {
                task_id: 1,
                m_type: TaskMessageType::Failed("err".into()),
            })
            .unwrap();
            tx.send(ProcessMessage {
                task_id: 2,
                m_type: TaskMessageType::Finished,
            })
            .unwrap();
        })
        .await;

        let job = read_job(&mut conn, &id).await;
        assert!(matches!(job.kind, JobStatusKind::PartiallyFailed));
        assert_eq!(job.completed_tasks, 1);
        assert_eq!(job.failed_tasks, 1);

        cleanup(&mut conn, &format!("job:{id}*")).await;
    }

    #[tokio::test]
    async fn failed_counter_does_not_affect_completed_counter() {
        let mut conn = connect().await;
        let id = job_id();

        let forwarder = StatusForwarder::new(id.clone(), 3, conn.clone())
            .await
            .unwrap();
        forwarder.init().await.unwrap();

        drive(forwarder, |tx| {
            tx.send(ProcessMessage {
                task_id: 1,
                m_type: TaskMessageType::Finished,
            })
            .unwrap();
            tx.send(ProcessMessage {
                task_id: 2,
                m_type: TaskMessageType::Failed("err".into()),
            })
            .unwrap();
            tx.send(ProcessMessage {
                task_id: 3,
                m_type: TaskMessageType::Finished,
            })
            .unwrap();
        })
        .await;

        let job = read_job(&mut conn, &id).await;
        assert_eq!(job.completed_tasks, 2);
        assert_eq!(job.failed_tasks, 1);

        cleanup(&mut conn, &format!("job:{id}*")).await;
    }

    #[tokio::test]
    async fn panicking_task_does_not_corrupt_job_key() {
        let mut conn = connect().await;
        let id = job_id();

        let forwarder = StatusForwarder::new(id.clone(), 2, conn.clone())
            .await
            .unwrap();
        forwarder.init().await.unwrap();

        drive(forwarder, |tx| {
            tx.send(ProcessMessage {
                task_id: 2,
                m_type: TaskMessageType::Finished,
            })
            .unwrap();
        })
        .await;

        let job = read_job(&mut conn, &id).await;
        assert_eq!(job.completed_tasks + job.failed_tasks, job.total_tasks - 1);

        cleanup(&mut conn, &format!("job:{id}*")).await;
    }
}

mod pre_dispatch_failure {
    use mpe::processor::status::JobStatusKind;

    use super::*;

    #[tokio::test]
    async fn writes_failed_status_with_error() {
        let mut conn = connect().await;
        let id = job_id();

        StatusForwarder::write_pre_dispatch_failure(&id, 3, "task 2 has no output path", &mut conn)
            .await
            .unwrap();

        let job = read_job(&mut conn, &id).await;
        assert!(matches!(job.kind, JobStatusKind::Failed));
        assert_eq!(job.error.as_deref(), Some("task 2 has no output path"));

        cleanup(&mut conn, &format!("job:{id}*")).await;
    }

    #[tokio::test]
    async fn started_at_is_null() {
        let mut conn = connect().await;
        let id = job_id();

        StatusForwarder::write_pre_dispatch_failure(&id, 1, "invalid path", &mut conn)
            .await
            .unwrap();

        let job = read_job(&mut conn, &id).await;
        assert!(job.started_at.is_none());

        cleanup(&mut conn, &format!("job:{id}*")).await;
    }

    #[tokio::test]
    async fn counters_are_zero() {
        let mut conn = connect().await;
        let id = job_id();

        StatusForwarder::write_pre_dispatch_failure(&id, 5, "invalid path", &mut conn)
            .await
            .unwrap();

        let job = read_job(&mut conn, &id).await;
        assert_eq!(job.completed_tasks, 0);
        assert_eq!(job.failed_tasks, 0);
        assert_eq!(job.total_tasks, 5);

        cleanup(&mut conn, &format!("job:{id}*")).await;
    }

    #[tokio::test]
    async fn no_task_keys_written() {
        let mut conn = connect().await;
        let id = job_id();

        StatusForwarder::write_pre_dispatch_failure(&id, 2, "invalid path", &mut conn)
            .await
            .unwrap();

        let keys: Vec<String> = redis::cmd("KEYS")
            .arg(format!("job:{id}:task:*"))
            .query_async(&mut conn)
            .await
            .unwrap();

        assert!(keys.is_empty());

        cleanup(&mut conn, &format!("job:{id}*")).await;
    }

    #[tokio::test]
    async fn finished_at_is_set() {
        let mut conn = connect().await;
        let id = job_id();

        StatusForwarder::write_pre_dispatch_failure(&id, 1, "invalid path", &mut conn)
            .await
            .unwrap();

        let job = read_job(&mut conn, &id).await;
        assert!(job.finished_at.is_some());

        cleanup(&mut conn, &format!("job:{id}*")).await;
    }

    #[tokio::test]
    async fn job_key_has_ttl() {
        let mut conn = connect().await;
        let id = job_id();

        StatusForwarder::write_pre_dispatch_failure(&id, 1, "invalid path", &mut conn)
            .await
            .unwrap();

        let ttl: i64 = redis::cmd("TTL")
            .arg(format!("job:{id}"))
            .query_async(&mut conn)
            .await
            .unwrap();

        assert!(ttl > 0, "expected TTL to be set, got {ttl}");
        assert!(ttl <= 86400, "expected TTL <= 24h, got {ttl}");

        cleanup(&mut conn, &format!("job:{id}*")).await;
    }

    #[tokio::test]
    async fn error_field_null_on_completed_job() {
        let mut conn = connect().await;
        let id = job_id();

        let forwarder = StatusForwarder::new(id.clone(), 1, conn.clone())
            .await
            .unwrap();
        forwarder.init().await.unwrap();

        drive(forwarder, |tx| {
            tx.send(ProcessMessage {
                task_id: 1,
                m_type: TaskMessageType::Finished,
            })
            .unwrap();
        })
        .await;

        let job = read_job(&mut conn, &id).await;
        assert!(job.error.is_none());

        cleanup(&mut conn, &format!("job:{id}*")).await;
    }

    #[tokio::test]
    async fn error_field_null_when_tasks_fail_normally() {
        let mut conn = connect().await;
        let id = job_id();

        let forwarder = StatusForwarder::new(id.clone(), 1, conn.clone())
            .await
            .unwrap();
        forwarder.init().await.unwrap();

        drive(forwarder, |tx| {
            tx.send(ProcessMessage {
                task_id: 1,
                m_type: TaskMessageType::Failed("codec error".into()),
            })
            .unwrap();
        })
        .await;

        let job = read_job(&mut conn, &id).await;
        assert!(job.error.is_none());

        cleanup(&mut conn, &format!("job:{id}*")).await;
    }
}

mod started_at_timing {
    use super::*;

    #[tokio::test]
    async fn null_after_init_before_processing() {
        let mut conn = connect().await;
        let id = job_id();

        let forwarder = StatusForwarder::new(id.clone(), 1, conn.clone())
            .await
            .unwrap();
        forwarder.init().await.unwrap();

        let job = read_job(&mut conn, &id).await;
        assert!(job.started_at.is_none());

        cleanup(&mut conn, &format!("job:{id}*")).await;
    }

    #[tokio::test]
    async fn set_after_first_processing_message() {
        let mut conn = connect().await;
        let id = job_id();

        let forwarder = StatusForwarder::new(id.clone(), 1, conn.clone())
            .await
            .unwrap();
        forwarder.init().await.unwrap();

        drive(forwarder, |tx| {
            tx.send(ProcessMessage {
                task_id: 1,
                m_type: TaskMessageType::Processing {
                    step: "step".into(),
                    message: "Validating".into(),
                },
            })
            .unwrap();
        })
        .await;

        let job = read_job(&mut conn, &id).await;
        assert!(job.started_at.is_some());

        cleanup(&mut conn, &format!("job:{id}*")).await;
    }
}

mod early_task_failure {
    use mpe::processor::status::{StatusForwarder, TaskStatusKind};

    use super::*;

    #[tokio::test]
    async fn task_key_created_with_failed_status_without_prior_processing() {
        let mut conn = connect().await;
        let id = job_id();

        let forwarder = StatusForwarder::new(id.clone(), 1, conn.clone())
            .await
            .unwrap();
        forwarder.init().await.unwrap();

        drive(forwarder, |tx| {
            tx.send(ProcessMessage {
                task_id: 1,
                m_type: TaskMessageType::Failed("validation failed".into()),
            })
            .unwrap();
        })
        .await;

        let task = read_task(&mut conn, &id, 1).await;
        assert!(matches!(task.kind, TaskStatusKind::Failed));
        assert_eq!(task.error.as_deref(), Some("validation failed"));

        cleanup(&mut conn, &format!("job:{id}*")).await;
    }

    #[tokio::test]
    async fn increments_failed_counter() {
        let mut conn = connect().await;
        let id = job_id();

        let forwarder = StatusForwarder::new(id.clone(), 1, conn.clone())
            .await
            .unwrap();
        forwarder.init().await.unwrap();

        drive(forwarder, |tx| {
            tx.send(ProcessMessage {
                task_id: 1,
                m_type: TaskMessageType::Failed("validation failed".into()),
            })
            .unwrap();
        })
        .await;

        let job = read_job(&mut conn, &id).await;
        assert_eq!(job.failed_tasks, 1);
        assert_eq!(job.completed_tasks, 0);

        cleanup(&mut conn, &format!("job:{id}*")).await;
    }
}

mod duplicate_terminal {
    use super::*;

    #[tokio::test]
    async fn finished_does_not_double_count() {
        let mut conn = connect().await;
        let id = job_id();

        let forwarder = StatusForwarder::new(id.clone(), 1, conn.clone())
            .await
            .unwrap();
        forwarder.init().await.unwrap();

        drive(forwarder, |tx| {
            tx.send(ProcessMessage {
                task_id: 1,
                m_type: TaskMessageType::Finished,
            })
            .unwrap();
            tx.send(ProcessMessage {
                task_id: 1,
                m_type: TaskMessageType::Finished,
            })
            .unwrap();
        })
        .await;

        let job = read_job(&mut conn, &id).await;
        assert_eq!(job.completed_tasks, 1);

        cleanup(&mut conn, &format!("job:{id}*")).await;
    }

    #[tokio::test]
    async fn failed_does_not_double_count() {
        let mut conn = connect().await;
        let id = job_id();

        let forwarder = StatusForwarder::new(id.clone(), 1, conn.clone())
            .await
            .unwrap();
        forwarder.init().await.unwrap();

        drive(forwarder, |tx| {
            tx.send(ProcessMessage {
                task_id: 1,
                m_type: TaskMessageType::Failed("err".into()),
            })
            .unwrap();
            tx.send(ProcessMessage {
                task_id: 1,
                m_type: TaskMessageType::Failed("err".into()),
            })
            .unwrap();
        })
        .await;

        let job = read_job(&mut conn, &id).await;
        assert_eq!(job.failed_tasks, 1);

        cleanup(&mut conn, &format!("job:{id}*")).await;
    }

    #[tokio::test]
    async fn does_not_exceed_total_tasks() {
        let mut conn = connect().await;
        let id = job_id();

        let forwarder = StatusForwarder::new(id.clone(), 1, conn.clone())
            .await
            .unwrap();
        forwarder.init().await.unwrap();

        drive(forwarder, |tx| {
            tx.send(ProcessMessage {
                task_id: 1,
                m_type: TaskMessageType::Finished,
            })
            .unwrap();
            tx.send(ProcessMessage {
                task_id: 1,
                m_type: TaskMessageType::Finished,
            })
            .unwrap();
            tx.send(ProcessMessage {
                task_id: 1,
                m_type: TaskMessageType::Failed("err".into()),
            })
            .unwrap();
        })
        .await;

        let job = read_job(&mut conn, &id).await;
        assert!(job.completed_tasks + job.failed_tasks <= job.total_tasks);

        cleanup(&mut conn, &format!("job:{id}*")).await;
    }
}
