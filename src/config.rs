use std::path::Path;
use std::path::PathBuf;

use anyhow::{Result, bail};
use clap::Parser;

use crate::consts::*;
use crate::log_msg;

const AMQP_ADDR_DEFAULT_VALUE: &'static str = "amqp://127.0.0.1:5672/%2f";
const MPE_DEFAULT_CONSUMER_QUEUE: &'static str = "mpe_default_queue";
const REDIS_DEFAULT_ADDR_VALUE: &'static str = "redis://127.0.0.1:6379";
const MPE_DEFAULT_WORKERS_VALUE: &'static str = "1";
const MPE_DEFAULT_THREADS_VALUES: &'static str = "4";

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
        default_value = AMQP_ADDR_DEFAULT_VALUE
    )]
    pub addr: String,

    /// Consumer queue name
    #[arg(
        long,
        env = CONSUMER_QUEUE,
        default_value = MPE_DEFAULT_CONSUMER_QUEUE
    )]
    pub queue: String,

    /// Number of worker processes.
    #[arg(short, long, env = MPE_WORKERS, default_value = MPE_DEFAULT_WORKERS_VALUE)]
    pub workers: u64,

    /// Number of threads per worker
    #[arg(short, long, env = MPE_THREADS, default_value = MPE_DEFAULT_THREADS_VALUES)]
    pub threads: u64,

    /// Redis server address
    #[arg(long, env = REDIS_ADDR, default_value = REDIS_DEFAULT_ADDR_VALUE)]
    pub redis_addr: String,

    /// Output folder
    #[arg(short = 'o', long, env = WATERMARKED_OUTPUT)]
    pub output: PathBuf,
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
