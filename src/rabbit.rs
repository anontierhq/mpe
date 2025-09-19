use anyhow::{Result, bail};
use futures::StreamExt;
use lapin::{
    Connection, ConnectionProperties, Consumer,
    message::Delivery,
    options::{BasicAckOptions, BasicConsumeOptions, BasicRejectOptions},
    types::FieldTable,
};
use redis::{AsyncCommands, aio::MultiplexedConnection};

use crate::{config::Config, jobs::ProcessJob, log_msg, processor::processor::JobHandler};

const DEFAULT_CONSUMER_TAG: &'static str = "unique_mpe_worker";

pub struct RabbitConnection {
    consumer: Consumer,
    config: Config,
    redis_connection: MultiplexedConnection,
}

impl RabbitConnection {
    /// Establishes a connection to a RabbitMQ server, creates a channel, and sets up a basic consumer.
    ///
    /// # Arguments
    ///
    /// * `config` - The configuration containing AMQP address and consumer queue details.
    ///
    /// # Returns
    ///
    /// Returns a `Result<Self>` containing the initialized `RabbitConnection` on success,
    /// or an error if the connection, channel creation, or consumer setup fails.
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - The connection to the RabbitMQ server cannot be established.
    /// - The channel cannot be created.
    /// - The consumer cannot be set up.
    ///
    /// # Example
    ///
    /// ```rust
    /// let config = Config::load()?;
    /// let connection = RabbitConnection::establish_conn(&config).await?;
    /// ```
    pub async fn establish_conn(config: Config) -> Result<Self> {
        log_msg!(
            debug,
            "Trying to establish to RabbitMQ server with {}",
            config.addr
        );
        let conn = Connection::connect(&config.addr, ConnectionProperties::default()).await?;

        log_msg!(debug, "Trying to create a channel with RabbitMQ server");
        let channel = conn.create_channel().await?;

        log_msg!(
            debug,
            "Trying to create basic consumer with {DEFAULT_CONSUMER_TAG}"
        );
        let consumer = channel
            .basic_consume(
                &config.queue,
                DEFAULT_CONSUMER_TAG,
                BasicConsumeOptions::default(),
                FieldTable::default(),
            )
            .await?;

        let redis = redis::Client::open(&config.redis_addr[..])?;
        let redis_connection = redis.get_multiplexed_async_connection().await?;
        log_msg!(info, "Connected sucessfully to redis");

        Ok(Self {
            consumer,
            config,
            redis_connection,
        })
    }

    async fn next_message(&mut self) -> Result<Option<Delivery>> {
        match self.consumer.next().await {
            Some(delivery_result) => {
                let delivery = delivery_result?;
                log_msg!(
                    info,
                    "Received message with delivery tag: {}",
                    delivery.delivery_tag
                );
                Ok(Some(delivery))
            }
            None => {
                log_msg!(debug, "Consumer stream has ended");
                Ok(None)
            }
        }
    }

    pub async fn process_messages(&mut self) -> Result<()> {
        log_msg!(info, "Starting message processing loop");

        // Okay, all is sync until here. The main loop. Each message will
        // be started on a sync loop, since the objective here is to process
        // only one message at a time.

        // TODO: Enable process multiple tasks at a time, maybe by an env
        // variable or an argument.
        loop {
            log_msg!(info, "Waiting for next message...");

            match self.next_message().await {
                Ok(Some(delivery)) => {
                    log_msg!(
                        info,
                        "Started processing delivery with delivery tag {}",
                        delivery.delivery_tag
                    );

                    let process_job = if let Ok(json) = str::from_utf8(&delivery.data[..]) {
                        match from_json_str(&json) {
                            Ok(r) => r,
                            Err(_) => {
                                log_msg!(
                                    error,
                                    "Error while trying to parse received from RabbitMQ Json String to a valid task message"
                                );

                                // Do not lose time rejecting the message.
                                // Schedule this and go to next message.
                                tokio::spawn(async move {
                                    match reject_message(&delivery).await {
                                        Ok(_) => {
                                            log_msg!(info, "Reject malformed RabbitMQ Json String")
                                        }
                                        Err(err) => log_msg!(
                                            error,
                                            "Error while trying to reject message: {err}"
                                        ),
                                    };
                                });

                                continue;
                            }
                        }
                    } else {
                        log_msg!(
                            error,
                            "Error while trying to parse delivery message to UTF-8 String. 
                            Going to the next message round."
                        );

                        continue;
                    };

                    let job_id = get_job_id(&process_job);
                    if let Err(unprocessed) =
                        JobHandler::it(process_job, &self.config, &self.redis_connection)
                    {
                        if let Err(err) = set_job_status(
                            &job_id,
                            format!(
                                "[ERROR] Error while processing tasks from job {}. Tasks that have errors: {:?}",
                                job_id, unprocessed
                            ),
                            &mut self.redis_connection,
                        ).await {
                            log_msg!(error, "Failed to set job {job_id} status to {err}")
                        };
                        tokio::spawn(async move { reject_message(&delivery).await });
                    } else {
                        tokio::spawn(async move { ack_message(&delivery).await });
                    };
                }
                Ok(None) => {
                    log_msg!(
                        info,
                        "Consumer stream has ended by someone. Exiting gracefully"
                    );
                    return Ok(());
                }
                Err(err) => {
                    log_msg!(error, "Error while trying to receive next message: {err}");
                    return Err(err);
                }
            }
        }
    }
}

fn from_json_str(json_str: &str) -> Result<ProcessJob<'_>> {
    let job = serde_json::from_str(json_str)?;
    Ok(job)
}

fn get_job_id(job: &ProcessJob) -> String {
    match job {
        ProcessJob::Composed { job_id, .. } => job_id.to_string(),
        ProcessJob::Unique { attached_job, .. } => attached_job.to_string(),
    }
}

async fn ack_message(delivery: &Delivery) -> Result<()> {
    log_msg!(
        info,
        "Acknowledging message with delivery tag: {}",
        delivery.delivery_tag
    );
    delivery.ack(BasicAckOptions::default()).await?;
    Ok(())
}

async fn reject_message(delivery: &Delivery) -> Result<()> {
    log_msg!(
        info,
        "Rejecting message with delivery tag: {}",
        delivery.delivery_tag
    );
    delivery.reject(BasicRejectOptions::default()).await?;
    Ok(())
}

async fn set_job_status(
    job_id: &str,
    status: String,
    redis: &mut MultiplexedConnection,
) -> Result<()> {
    log_msg!(debug, "Setting status of job {job_id} to {status}");
    if let Err(err) = redis
        .set(format!("job:{job_id}"), status)
        .await
        .map(|_: ()| ())
    {
        bail!("Failed to set general status for job {job_id}. Error: {err}");
    };

    Ok(())
}
