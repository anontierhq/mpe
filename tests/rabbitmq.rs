use std::sync::OnceLock;

use lapin::{
    options::{
        BasicAckOptions, BasicGetOptions, BasicPublishOptions, BasicRejectOptions,
        ExchangeDeclareOptions, QueueBindOptions, QueueDeclareOptions, QueuePurgeOptions,
    },
    types::{AMQPValue, FieldTable, LongString, ShortString},
    BasicProperties, Connection, ConnectionProperties, ExchangeKind,
};
use mpe::constants::DEFAULT_AMQP_ADDR;

/// Both tests share the same queue names; run them one at a time to avoid purge/get races.
static AMQP_TEST_MUTEX: OnceLock<tokio::sync::Mutex<()>> = OnceLock::new();

async fn amqp_test_lock() -> tokio::sync::MutexGuard<'static, ()> {
    AMQP_TEST_MUTEX
        .get_or_init(|| tokio::sync::Mutex::new(()))
        .lock()
        .await
}

const TEST_DLX: &str = "mpe_integration_test.dlx";
const TEST_WORK: &str = "mpe_integration_test.work";
const TEST_DLQ: &str = "mpe_integration_test.work.dlq";
const TEST_DLQ_RK: &str = "mpe_integration_test.work.dlq";

fn amqp_addr() -> String {
    std::env::var("AMQP_ADDR").unwrap_or_else(|_| DEFAULT_AMQP_ADDR.to_string())
}

fn work_queue_arguments() -> FieldTable {
    let mut args = FieldTable::default();
    args.insert(
        ShortString::from("x-dead-letter-exchange"),
        AMQPValue::LongString(LongString::from(TEST_DLX)),
    );
    args.insert(
        ShortString::from("x-dead-letter-routing-key"),
        AMQPValue::LongString(LongString::from(TEST_DLQ_RK)),
    );
    args
}

async fn open_channel() -> lapin::Channel {
    let addr = amqp_addr();
    let conn = Connection::connect(&addr, ConnectionProperties::default())
        .await
        .unwrap_or_else(|e| {
            panic!(
                "RabbitMQ connection failed ({e}). Start the broker, e.g. `docker compose up -d rabbitmq`."
            )
        });
    conn.create_channel()
        .await
        .expect("create_channel failed")
}

async fn ensure_test_topology(ch: &lapin::Channel) {
    ch.exchange_declare(
        TEST_DLX.into(),
        ExchangeKind::Direct,
        ExchangeDeclareOptions {
            durable: true,
            ..Default::default()
        },
        FieldTable::default(),
    )
    .await
    .expect("exchange_declare DLX");

    ch.queue_declare(
        TEST_DLQ.into(),
        QueueDeclareOptions {
            durable: true,
            ..Default::default()
        },
        FieldTable::default(),
    )
    .await
    .expect("queue_declare DLQ");

    ch.queue_bind(
        TEST_DLQ.into(),
        TEST_DLX.into(),
        TEST_DLQ_RK.into(),
        QueueBindOptions::default(),
        FieldTable::default(),
    )
    .await
    .expect("queue_bind DLQ");

    ch.queue_declare(
        TEST_WORK.into(),
        QueueDeclareOptions {
            durable: true,
            ..Default::default()
        },
        work_queue_arguments(),
    )
    .await
    .expect("queue_declare work");
}

async fn purge_pair(ch: &lapin::Channel) {
    let _ = ch
        .queue_purge(TEST_WORK.into(), QueuePurgeOptions::default())
        .await;
    let _ = ch
        .queue_purge(TEST_DLQ.into(), QueuePurgeOptions::default())
        .await;
}

async fn publish_to_work(ch: &lapin::Channel, payload: &[u8]) {
    let confirm = ch
        .basic_publish(
            "".into(),
            TEST_WORK.into(),
            BasicPublishOptions::default(),
            payload,
            BasicProperties::default(),
        )
        .await
        .expect("basic_publish");
    let _ = confirm.await;
}

#[tokio::test]
async fn reject_without_requeue_dead_letters_to_dlq() {
    let _lock = amqp_test_lock().await;
    let ch = open_channel().await;
    ensure_test_topology(&ch).await;
    purge_pair(&ch).await;

    let payload = b"dlq-routing-check";
    publish_to_work(&ch, payload).await;

    let incoming = ch
        .basic_get(
            TEST_WORK.into(),
            BasicGetOptions { no_ack: false },
        )
        .await
        .expect("basic_get")
        .expect("expected one message on work queue");

    incoming
        .reject(BasicRejectOptions { requeue: false })
        .await
        .expect("reject");

    let dlq_msg = ch
        .basic_get(
            TEST_DLQ.into(),
            BasicGetOptions { no_ack: false },
        )
        .await
        .expect("basic_get DLQ")
        .expect("expected message on DLQ after reject");

    assert_eq!(dlq_msg.data.as_slice(), payload);

    dlq_msg
        .ack(BasicAckOptions::default())
        .await
        .expect("ack DLQ");
}

#[tokio::test]
async fn ack_does_not_send_to_dlq() {
    let _lock = amqp_test_lock().await;
    let ch = open_channel().await;
    ensure_test_topology(&ch).await;
    purge_pair(&ch).await;

    publish_to_work(&ch, b"ack-stays-off-dlq").await;

    let incoming = ch
        .basic_get(
            TEST_WORK.into(),
            BasicGetOptions { no_ack: false },
        )
        .await
        .expect("basic_get")
        .expect("expected one message on work queue");

    incoming
        .ack(BasicAckOptions::default())
        .await
        .expect("ack");

    let dlq_tail = ch
        .basic_get(
            TEST_DLQ.into(),
            BasicGetOptions { no_ack: false },
        )
        .await
        .expect("basic_get DLQ");

    assert!(
        dlq_tail.is_none(),
        "DLQ should be empty after ack on work queue"
    );
}
