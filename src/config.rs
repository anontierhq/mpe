use anyhow::{Result, bail};
use clap::Parser;

use crate::constants::*;
use crate::log_msg;

/// MPE Configuration
#[derive(Parser, Debug, Clone)]
#[command(name = "mpe")]
#[command(about = "Task Processing Engine")]
#[command(version = "0.1.0")]
pub struct Config {
    /// AMQP server address
    #[arg(
        long,
        env = AMQP_ADDR,
        default_value = DEFAULT_AMQP_ADDR
    )]
    pub addr: String,

    /// Consumer queue name
    #[arg(
        long,
        env = CONSUMER_QUEUE,
        default_value = DEFAULT_CONSUMER_QUEUE
    )]
    pub queue: String,

    /// Number of worker processes.
    #[arg(short, long, env = MPE_WORKERS, default_value = DEFAULT_WORKERS)]
    pub workers: u64,

    /// Redis server address
    #[arg(long, env = REDIS_ADDR, default_value = DEFAULT_REDIS_ADDR)]
    pub redis_addr: String,
}

impl Config {
    pub fn load() -> Result<Self> {
        let config = Config::parse();

        if config.workers == 0 {
            bail!("Workers must be greater than 0");
        }

        if config.queue.trim().is_empty() {
            bail!("Consumer queue name cannot be empty");
        }

        if config.addr.trim().is_empty() {
            bail!("AMQP address cannot be empty");
        }

        if config.workers > 16 {
            log_msg!(warn, "Using more than 16 workers is not recommended.");
        }

        Ok(config)
    }
}
