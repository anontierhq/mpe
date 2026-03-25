# Getting started

This guide covers what you need on the machine, how to configure MPE, and how to run it against local Docker services.

## Prerequisites

- **Rust** (2024 edition toolchain) — [rustup](https://rustup.rs)
- **ffmpeg** and **ffprobe** on `PATH`, with H.264 encoding support (e.g. libx264)
- **Shaka Packager** binary named `packager` on `PATH` — [releases](https://github.com/shaka-project/shaka-packager/releases) (project tests/docs have used v3.7.x)
- **Docker** (or another RabbitMQ + Redis deployment) if you follow the compose flow below

### Tooling quick install (examples)

**Debian / Ubuntu**

```bash
sudo apt install ffmpeg
```

**macOS**

```bash
brew install ffmpeg
```

**Shaka Packager** — download the binary for your OS, make it executable, install as `packager`:

```bash
chmod +x packager-linux-x64
sudo mv packager-linux-x64 /usr/local/bin/packager
```

Verify:

```bash
ffmpeg -version && ffprobe -version && packager --version
```

## Run Redis and RabbitMQ locally

From the repository root:

```bash
docker compose up -d
```

This starts:

- **Redis** — `6379`
- **RabbitMQ** — AMQP `5672`, management UI `http://localhost:15672` (default user/password `guest` / `guest`)

You must still **create the queue topology** (exchange, DLQ binding, work queue arguments). See [rabbitmq.md](rabbitmq.md).

## Build and run MPE

```bash
cargo build --release
cargo run --release
```

With no arguments, defaults match local Docker: AMQP `amqp://127.0.0.1:5672/%2f`, queue `mpe_default_queue`, Redis `redis://127.0.0.1:6379`, one worker.

## Configuration

All options can be set via **CLI flags** or **environment variables** (clap `env`).

| Flag | Environment variable | Default | Meaning |
| --- | --- | --- | --- |
| `--addr` | `AMQP_ADDR` | `amqp://127.0.0.1:5672/%2f` | RabbitMQ connection URI |
| `--queue` | `CONSUMER_QUEUE` | `mpe_default_queue` | Queue to consume |
| `--workers` | `MPE_WORKERS` | `1` | Concurrent task threads (must be ≥ 1; more than 16 logs a warning) |
| `--task-retries` | `MPE_TASK_RETRIES` | `0` | Extra attempts after the first failure (max 32); total attempts = 1 + value |
| `--task-retry-delay-ms` | `MPE_TASK_RETRY_DELAY_MS` | `1000` | Delay between retries (ms); no sleep after the final failure |
| `--redis-addr` | `REDIS_ADDR` | `redis://127.0.0.1:6379` | Redis connection URI |

**Logging** — `simple_logger` with `RUST_LOG` (e.g. `RUST_LOG=info`, `debug`).

## Job output paths

Each task must resolve to an **absolute** output directory:

- Either set **`base_output`** on the job (paths become `{base_output}/{job_id}/{task_id}/`), **or**
- Set **`output_path`** on that task (and omit `base_output` for that job).

You cannot set both `base_output` and per-task `output_path` on the same task. See [output.md](output.md) for layout and safety rules.

## End-to-end smoke check

1. Topology ready, MPE running.
2. Publish a minimal job message with a real video file and valid absolute `base_output` or `output_path` (see [rabbitmq.md](rabbitmq.md)).
3. Poll Redis: `job:{job_id}` and `job:{job_id}:task:{task_id}` (see [redis.md](redis.md)).
4. Inspect files under the task output directory (see [output.md](output.md)).

## Tests

Integration tests expect Redis (and for AMQP tests, RabbitMQ) reachable at the default URLs unless you override `AMQP_ADDR`:

```bash
docker compose up -d
cargo test
```
