use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum ProcessJob<'a> {
    // Job = Whole Job, that can have multiple tasks whitin
    Composed {
        job_id: &'a str,
        tasks_to_process: Vec<Task>,
        base_output: Option<String>,
    },
    Unique {
        attached_job: &'a str,
        task_to_process: Task,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum TaskType {
    Video,
    Image,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Task {
    pub id: u64,
    pub filepath: String,
    pub task_type: TaskType,
    pub output_path: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deserializes_composed_job() {
        let json = r#"{
            "type": "Composed",
            "job_id": "job-001",
            "tasks_to_process": [
                { "id": 1, "filepath": "/media/a.mp4", "task_type": "Video" },
                { "id": 2, "filepath": "/media/b.jpg", "task_type": "Image" }
            ]
        }"#;

        let job: ProcessJob = serde_json::from_str(json).unwrap();

        match job {
            ProcessJob::Composed {
                job_id,
                tasks_to_process,
                ..
            } => {
                assert_eq!(job_id, "job-001");
                assert_eq!(tasks_to_process.len(), 2);
                assert_eq!(tasks_to_process[0].id, 1);
                assert!(matches!(tasks_to_process[0].task_type, TaskType::Video));
                assert_eq!(tasks_to_process[1].id, 2);
                assert!(matches!(tasks_to_process[1].task_type, TaskType::Image));
            }
            _ => panic!("expected Composed"),
        }
    }

    #[test]
    fn deserializes_unique_job() {
        let json = r#"{
            "type": "Unique",
            "attached_job": "job-001",
            "task_to_process": { "id": 3, "filepath": "/media/c.mp4", "task_type": "Video" }
        }"#;

        let job: ProcessJob = serde_json::from_str(json).unwrap();

        match job {
            ProcessJob::Unique {
                attached_job,
                task_to_process,
            } => {
                assert_eq!(attached_job, "job-001");
                assert_eq!(task_to_process.id, 3);
                assert!(matches!(task_to_process.task_type, TaskType::Video));
            }
            _ => panic!("expected Unique"),
        }
    }

    #[test]
    fn rejects_unknown_type() {
        let json = r#"{ "type": "Batch", "job_id": "x", "tasks_to_process": [] }"#;
        assert!(serde_json::from_str::<ProcessJob>(json).is_err());
    }

    #[test]
    fn rejects_composed_missing_job_id() {
        let json = r#"{ "type": "Composed", "tasks_to_process": [] }"#;
        assert!(serde_json::from_str::<ProcessJob>(json).is_err());
    }
}
