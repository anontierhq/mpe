use anyhow::{Result, bail};
use simple_logger::SimpleLogger;

use crate::config::Config;
use crate::constants::{MPE_THREADS, MPE_WORKERS, WATERMARKED_OUTPUT};
use crate::rabbit::RabbitConnection;

mod config;
mod constants;
mod jobs;
mod logger;
mod processor;
mod rabbit;

#[tokio::main]
async fn main() -> Result<()> {
    setup_config()?;
    setup_log();

    // Load configuration from command line arguments and environment variables
    let config = Config::load()?;

    log_msg!(debug, "AMQP Address: {}", config.addr);
    log_msg!(debug, "Consumer Queue: {}", config.queue);
    log_msg!(debug, "Workers: {}", config.workers);
    log_msg!(debug, "Threads: {}", config.threads);

    // We really want to lock the main function while establishing the connection.
    // The real async comes later.
    let mut conn = match RabbitConnection::establish_conn(config).await {
        Ok(conn) => {
            log_msg!(info, "Connected succefully to RabbitMQ instance");
            conn
        }
        Err(err) => {
            log_msg!(error, "Failed to connect to RabbitMQ: {err}");
            bail!("Failed to connect to RabbitMQ: {err}");
        }
    };

    if let Err(err) = conn.process_messages().await {
        log_msg!(
            error,
            "MPE ended. Critical error on Main Loop Reason: {err}"
        )
    } else {
        log_msg!(info, "MPE ending gracefully.")
    }

    println!("Bye!");

    Ok(())
}

fn setup_log() {
    SimpleLogger::new()
        .env()
        .init()
        .expect("Unexpected logger error.");
}

fn setup_config() -> Result<Config> {
    let cfg = Config::load()?;

    // SAFETY:
    // This function is only executed in single thread stage.
    unsafe {
        std::env::set_var(MPE_THREADS, cfg.threads.to_string());
        std::env::set_var(MPE_WORKERS, cfg.workers.to_string());
        std::env::set_var(
            WATERMARKED_OUTPUT,
            cfg.output
                .to_str()
                .map(|v| v.to_string())
                .expect("Output not defined to a valid string!"),
        );
    }

    Ok(cfg)
}
