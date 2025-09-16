use anyhow::Result;

use crate::rabbit::ProcessTask;

pub trait MessageProcessor {
    async fn process(&self, task: ProcessTask) -> Result<()>;
}