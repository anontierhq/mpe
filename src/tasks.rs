use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum ProcessTask<'a> {
    Composed {
        job_id: &'a str,
        medias_to_process: Vec<Media>,
    },
    Unique {
        attached_job: &'a str,
        media_to_process: Media,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum MediaType {
    Video,
    Image,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Media {
    pub id: u64,
    pub filepath: String,
    pub media_type: MediaType,
}
