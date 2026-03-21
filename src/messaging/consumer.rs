use anyhow::Result;
use redis::{AsyncCommands, aio::MultiplexedConnection};

use crate::{
    config::Config,
    jobs::ProcessJob,
    log_msg,
    processor::JobHandler,
};
use super::connection::{self as rabbit, RabbitConnection};

pub struct Worker {
    rabbit: RabbitConnection,
    redis: MultiplexedConnection,
    config: Config,
}

impl Worker {
    pub async fn new(config: Config) -> Result<Self> {
        let rabbit = RabbitConnection::connect(&config.addr, &config.queue).await?;
        log_msg!(info, "Connected to RabbitMQ");

        let redis = redis::Client::open(&config.redis_addr[..])?
            .get_multiplexed_async_connection()
            .await?;
        log_msg!(info, "Connected to Redis");

        Ok(Self { rabbit, redis, config })
    }

    pub async fn run(&mut self) -> Result<()> {
        log_msg!(info, "Starting message loop");

        loop {
            match self.rabbit.next_message().await? {
                None => {
                    log_msg!(info, "Consumer stream ended. Exiting gracefully.");
                    return Ok(());
                }
                Some(delivery) => self.handle_delivery(delivery).await,
            }
        }
    }

    async fn handle_delivery(&mut self, delivery: lapin::message::Delivery) {
        let job = match parse_delivery(&delivery) {
            Ok(job) => job,
            Err(_) => {
                log_msg!(error, "Failed to parse delivery, rejecting");
                tokio::spawn(rabbit::reject(delivery));
                return;
            }
        };

        let job_id = job_id_of(&job);
        self.set_job_status(&job_id, format!("[STARTED] Job {job_id} started")).await;

        match JobHandler::it(job, &self.config, &self.redis) {
            Ok(_) => {
                self.set_job_status(&job_id, format!("[FINISHED] Job {job_id} finished gracefully"))
                    .await;
                tokio::spawn(rabbit::ack(delivery));
            }
            Err(failed) => {
                let failed_ids = failed
                    .iter()
                    .map(|t| format!("id:{}", t.id))
                    .collect::<Vec<_>>()
                    .join(", ");
                self.set_job_status(&job_id, format!("[ERROR] Job {job_id} failed. Tasks: {failed_ids}"))
                    .await;
                tokio::spawn(rabbit::reject(delivery));
            }
        }
    }

    async fn set_job_status(&mut self, job_id: &str, status: String) {
        log_msg!(debug, "Job {job_id} status: {status}");
        if let Err(err) = self.redis
            .set::<_, _, ()>(format!("job:{job_id}"), status)
            .await
        {
            log_msg!(error, "Failed to set job {job_id} status: {err}");
        }
    }
}

fn parse_delivery(delivery: &lapin::message::Delivery) -> Result<ProcessJob<'_>> {
    let json = std::str::from_utf8(&delivery.data)?;
    Ok(serde_json::from_str(json)?)
}

fn job_id_of(job: &ProcessJob) -> String {
    match job {
        ProcessJob::Composed { job_id, .. } => job_id.to_string(),
        ProcessJob::Unique { attached_job, .. } => attached_job.to_string(),
    }
}
