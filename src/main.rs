use anyhow::Result;
use simple_logger::SimpleLogger;

use crate::config::Config;
use crate::messaging::Worker;

mod config;
mod constants;
mod jobs;
mod logger;
mod messaging;
mod processor;

#[tokio::main]
async fn main() -> Result<()> {
    setup_log();

    let config = Config::load()?;
    log_msg!(
        debug,
        "AMQP: {}, Queue: {}, Workers: {}, Threads: {}",
        config.addr,
        config.queue,
        config.workers,
        config.threads
    );

    let mut worker = Worker::new(config).await?;
    worker.run().await
}

fn setup_log() {
    SimpleLogger::new()
        .env()
        .init()
        .expect("Logger init failed");
}
