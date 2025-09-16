use anyhow::{bail, Result};
use clap::Parser;

use crate::log_msg;

/// MPE Configuration
#[derive(Parser, Debug, Clone)]
#[command(name = "mpe")]
#[command(about = "Media Processing Engine")]
#[command(version = "0.1.0")]
pub struct Config {
    /// AMQP server address
    #[arg(short, long, env = "AMQP_ADDR", default_value = "amqp://127.0.0.1:5672/%2f")]
    pub addr: String,

    /// Consumer queue name
    #[arg(short = 'q', long, env = "CONSUMER_QUEUE", default_value = "mpe_default_queue")]
    pub queue: String,

    /// Number of worker processes.
    #[arg(short, long, env = "MPE_WORKERS", default_value = "1")]
    pub workers: i32,

    /// Number of threads per worker
    #[arg(short, long, env = "MPE_THREADS", default_value = "4")]
    pub threads: i32,
}

impl Config {
    pub fn load() -> Result<Self> {
        let config = Config::parse();
        
        if config.workers <= 0 {
            bail!("Workers must be greater than 0");
        }
        
        if config.threads <= 0 {
            bail!("Threads must be greater than 0");
        }

        if config.queue.trim().is_empty() {
            bail!("Consumer queue name cannot be empty");
        }

        if config.addr.trim().is_empty() {
            bail!("AMQP address cannot be empty");
        }

        if config.threads > 32 {
            log_msg!(warn, "Using more than 32 threads per worker is not recommended.");
        }

        if config.workers > 16 {
            log_msg!(warn, "Using more than 16 workers is not recommended.");
        }

        Ok(config)
    }
}