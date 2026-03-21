pub const AMQP_ADDR: &str = "AMQP_ADDR";
pub const CONSUMER_QUEUE: &str = "CONSUMER_QUEUE";
pub const MPE_WORKERS: &str = "MPE_WORKERS";
pub const MPE_THREADS: &str = "MPE_THREADS";
pub const REDIS_ADDR: &str = "REDIS_ADDR";
pub const WATERMARKED_OUTPUT: &str = "WATERMARK_OUTPUT";

pub const DEFAULT_AMQP_ADDR: &str = "amqp://127.0.0.1:5672/%2f";
pub const DEFAULT_CONSUMER_QUEUE: &str = "mpe_default_queue";
pub const DEFAULT_REDIS_ADDR: &str = "redis://127.0.0.1:6379";
pub const DEFAULT_WORKERS: &str = "1";
pub const DEFAULT_THREADS: &str = "4";

pub const DEFAULT_CONSUMER_TAG: &str = "unique_mpe_worker";
