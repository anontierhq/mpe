pub const AMQP_ADDR: &str = "AMQP_ADDR";
pub const CONSUMER_QUEUE: &str = "CONSUMER_QUEUE";
pub const MPE_WORKERS: &str = "MPE_WORKERS";
pub const MPE_TASK_RETRIES: &str = "MPE_TASK_RETRIES";
pub const MPE_TASK_RETRY_DELAY_MS: &str = "MPE_TASK_RETRY_DELAY_MS";
pub const REDIS_ADDR: &str = "REDIS_ADDR";

pub const DEFAULT_AMQP_ADDR: &str = "amqp://127.0.0.1:5672/%2f";
pub const DEFAULT_CONSUMER_QUEUE: &str = "mpe_default_queue";
/// Dead-letter exchange name used with the default work queue when the broker is configured for DLX.
pub const DEFAULT_DLX_EXCHANGE: &str = "mpe.dlx";
/// Dead-letter queue name paired with [`DEFAULT_CONSUMER_QUEUE`] when using [`DEFAULT_DLX_EXCHANGE`].
pub const DEFAULT_DLQ_QUEUE: &str = "mpe_default_queue.dlq";
pub const DEFAULT_REDIS_ADDR: &str = "redis://127.0.0.1:6379";
pub const DEFAULT_WORKERS: &str = "1";
pub const DEFAULT_TASK_RETRIES: &str = "0";
pub const DEFAULT_TASK_RETRY_DELAY_MS: &str = "1000";

pub const DEFAULT_CONSUMER_TAG: &str = "unique_mpe_worker";
