use std::path::PathBuf;

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

    /// Number of threads per worker
    #[arg(short, long, env = MPE_THREADS, default_value = DEFAULT_THREADS)]
    pub threads: u64,

    /// Redis server address
    #[arg(long, env = REDIS_ADDR, default_value = DEFAULT_REDIS_ADDR)]
    pub redis_addr: String,

    /// Output folder
    #[arg(short = 'o', long, env = WATERMARKED_OUTPUT)]
    pub output: PathBuf,
}

impl Config {
    pub fn load() -> Result<Self> {
        let config = Config::parse();

        if config.workers == 0 {
            bail!("Workers must be greater than 0");
        }

        if config.threads == 0 {
            bail!("Threads must be greater than 0");
        }

        if config.queue.trim().is_empty() {
            bail!("Consumer queue name cannot be empty");
        }

        if config.addr.trim().is_empty() {
            bail!("AMQP address cannot be empty");
        }

        if config.threads > 32 {
            log_msg!(
                warn,
                "Using more than 32 threads per worker is not recommended."
            );
        }

        if config.workers > 16 {
            log_msg!(warn, "Using more than 16 workers is not recommended.");
        }

        if !config.output.exists() || !config.output.is_dir() {
            bail!("Output folder is invalid.")
        }

        Ok(config)
    }
}
