use anyhow::{Result, bail};
use simple_logger::SimpleLogger;

use crate::rabbit::RabbitConnection;

mod logger;
mod rabbit;

const ADDR_KEY: &'static str = "AMQP_ADDR";
const CONSUMER_QUEUE_KEY: &'static str = "CONSUMER_QUEUE";

const DEFAULT_CONSUMER_QUEUE: &'static str = "mpe_default_queue";
const DEFAULT_AMQP_ADDR: &'static str = "amqp://127.0.0.1:5672/%2f";

#[tokio::main]
async fn main() -> Result<()> {
    setup_log();

    let addr = std::env::var(ADDR_KEY).unwrap_or_else(|_| DEFAULT_AMQP_ADDR.into());
    let consumer_queue =
        std::env::var(CONSUMER_QUEUE_KEY).unwrap_or_else(|_| DEFAULT_CONSUMER_QUEUE.into());

    log_msg!(debug, "AMQP Address: {addr}");
    log_msg!(debug, "Consumer Queue: {consumer_queue}");

    // We really want to lock the main function while establishing the connection.
    // The real async comes later.
    let mut conn = match RabbitConnection::establish_conn(&addr, &consumer_queue).await {
        Ok(conn) => {
            log_msg!(info, "Connected succefully to RabbitMQ instance");
            conn
        }
        Err(err) => {
            log_msg!(error, "Failed to connect to RabbitMQ: {err}");
            bail!("Failed to connect to RabbitMQ: {err}");
        }
    };

    conn.process_messages(|delivery| async move { Ok(()) })
        .await?;

    Ok(())
}

fn setup_log() {
    SimpleLogger::new()
        .env()
        .init()
        .expect("Unexpected logger error.");
}
