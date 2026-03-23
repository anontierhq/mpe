use anyhow::Result;
use mpe::log_msg;
use simple_logger::SimpleLogger;

use mpe::config::Config;
use mpe::messaging::Worker;

#[tokio::main]
async fn main() -> Result<()> {
    setup_log();

    let config = Config::load()?;
    log_msg!(
        debug,
        "AMQP: {}, Queue: {}, Workers: {}",
        config.addr,
        config.queue,
        config.workers
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
