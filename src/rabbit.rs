use anyhow::Result;
use futures::StreamExt;
use lapin::{
    Connection, ConnectionProperties, Consumer,
    message::Delivery,
    options::{BasicAckOptions, BasicConsumeOptions, BasicRejectOptions},
    types::FieldTable,
};
use serde::{Deserialize, Serialize};

use crate::log_msg;

const DEFAULT_CONSUMER_TAG: &'static str = "unique_mpe_worker";

pub struct RabbitConnection {
    consumer: Consumer,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum ProcessTask {
    Composed {
        job_id: i32,
        medias_to_process: Vec<Media>,
    },
    Unique {
        attached_job: i32,
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
    pub id: i32,
    pub filepath: String,
    pub media_type: MediaType,
}

impl RabbitConnection {
    /// Establishes a connection to a RabbitMQ server, creates a channel, and sets up a basic consumer.
    ///
    /// # Arguments
    ///
    /// * `addr` - The address of the RabbitMQ server to connect to.
    /// * `consumer_queue` - The name of the queue to consume messages from.
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
    /// let connection = RabbitConnection::establish_conn("amqp://localhost:5672", "my_queue").await?;
    /// ```
    pub async fn establish_conn(addr: &str, consumer_queue: &str) -> Result<Self> {
        log_msg!(debug, "Trying to establish to RabbitMQ server with {addr}");
        let conn = Connection::connect(addr, ConnectionProperties::default()).await?;

        log_msg!(debug, "Trying to create a channel with RabbitMQ server");
        let channel = conn.create_channel().await?;

        log_msg!(
            debug,
            "Trying to create basic consumer with {DEFAULT_CONSUMER_TAG}"
        );
        let consumer = channel
            .basic_consume(
                consumer_queue,
                DEFAULT_CONSUMER_TAG,
                BasicConsumeOptions::default(),
                FieldTable::default(),
            )
            .await?;

        Ok(Self { consumer })
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

    pub async fn ack_message(delivery: &Delivery) -> Result<()> {
        log_msg!(
            info,
            "Acknowledging message with delivery tag: {}",
            delivery.delivery_tag
        );
        delivery.ack(BasicAckOptions::default()).await?;
        Ok(())
    }

    pub async fn reject_message(delivery: &Delivery) -> Result<()> {
        log_msg!(
            info,
            "Rejecting message with delivery tag: {}",
            delivery.delivery_tag
        );
        delivery.reject(BasicRejectOptions::default()).await?;
        Ok(())
    }

    pub async fn process_messages<F, Fut>(&mut self, mut handler: F) -> Result<()>
    where
        F: FnMut(ProcessTask) -> Fut,
        Fut: std::future::Future<Output = Result<()>>,
    {
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
                    let delivery_tag = delivery.delivery_tag;

                    let process_task = if let Ok(json) = String::from_utf8(delivery.data.clone()) {
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
                                    match Self::reject_message(&delivery).await {
                                        Ok(_) => {}
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
                            "Error while trying to parse delivery message to UTF-8 String"
                        );

                        continue;
                    };

                    match handler(process_task).await {
                        Ok(()) => {
                            log_msg!(
                                debug,
                                "Message processed successfully, acknowledging delivery tag: {}",
                                delivery_tag
                            );
                        }
                        Err(err) => {
                            log_msg!(error, "Error processing message: {}", err);
                            log_msg!(
                                debug,
                                "Message rejected due to processing error, delivery tag: {}",
                                delivery_tag
                            );
                        }
                    }
                }
                Ok(None) => {}
                Err(err) => {
                    log_msg!(error, "Error while trying to receive next message: {err}")
                }
            }
        }
    }
}

fn from_json_str(json_str: &str) -> Result<ProcessTask> {
    let task = serde_json::from_str(json_str)?;
    Ok(task)
}
