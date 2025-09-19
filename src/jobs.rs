use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum ProcessJob<'a> {
    // Job = Whole Job, that can have multiple tasks whitin
    Composed {
        job_id: &'a str,
        tasks_to_process: Vec<Task>,
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
}
