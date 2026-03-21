use anyhow::Result;
use futures::StreamExt;
use lapin::{
    Connection, ConnectionProperties, Consumer,
    message::Delivery,
    options::{BasicAckOptions, BasicConsumeOptions, BasicRejectOptions},
    types::FieldTable,
};

use crate::{constants::DEFAULT_CONSUMER_TAG, log_msg};

pub struct RabbitConnection {
    consumer: Consumer,
}

impl RabbitConnection {
    pub async fn connect(addr: &str, queue: &str) -> Result<Self> {
        log_msg!(debug, "Connecting to RabbitMQ at {addr}");
        let conn = Connection::connect(addr, ConnectionProperties::default()).await?;

        log_msg!(debug, "Creating channel");
        let channel = conn.create_channel().await?;

        log_msg!(debug, "Setting up consumer with tag {DEFAULT_CONSUMER_TAG}");
        let consumer = channel
            .basic_consume(
                queue.into(),
                DEFAULT_CONSUMER_TAG.into(),
                BasicConsumeOptions::default(),
                FieldTable::default(),
            )
            .await?;

        Ok(Self { consumer })
    }

    pub async fn next_message(&mut self) -> Result<Option<Delivery>> {
        match self.consumer.next().await {
            Some(result) => {
                let delivery = result?;
                log_msg!(info, "Received delivery tag {}", delivery.delivery_tag);
                Ok(Some(delivery))
            }
            None => {
                log_msg!(debug, "Consumer stream ended");
                Ok(None)
            }
        }
    }
}

pub async fn ack(delivery: Delivery) -> Result<()> {
    log_msg!(info, "Acknowledging delivery tag {}", delivery.delivery_tag);
    delivery.ack(BasicAckOptions::default()).await?;
    Ok(())
}

pub async fn reject(delivery: Delivery) -> Result<()> {
    log_msg!(info, "Rejecting delivery tag {}", delivery.delivery_tag);
    delivery.reject(BasicRejectOptions::default()).await?;
    Ok(())
}
