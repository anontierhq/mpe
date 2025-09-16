use anyhow::{Result, bail};
use simple_logger::SimpleLogger;

use crate::rabbit::RabbitConnection;
use crate::config::Config;

mod logger;
mod rabbit;
mod processor;
mod config;

#[tokio::main]
async fn main() -> Result<()> {
    setup_log();

    // Load configuration from command line arguments and environment variables
    let config = Config::load()?;

    log_msg!(debug, "AMQP Address: {}", config.addr);
    log_msg!(debug, "Consumer Queue: {}", config.queue);
    log_msg!(debug, "Workers: {}", config.workers);
    log_msg!(debug, "Threads: {}", config.threads);

    // We really want to lock the main function while establishing the connection.
    // The real async comes later.
    let conn = match RabbitConnection::establish_conn(&config).await {
        Ok(conn) => {
            log_msg!(info, "Connected succefully to RabbitMQ instance");
            conn
        }
        Err(err) => {
            log_msg!(error, "Failed to connect to RabbitMQ: {err}");
            bail!("Failed to connect to RabbitMQ: {err}");
        }
    };

    Ok(())
}

fn setup_log() {
    SimpleLogger::new()
        .env()
        .init()
        .expect("Unexpected logger error.");
}
