# MPE — Media Processing Engine

MPE is a Rust service that pulls **jobs** from **RabbitMQ**, runs **video** (and placeholder **image**) tasks in parallel, and writes **HLS / MPEG-DASH (CMAF)** output to disk. **Redis** holds live **job** and **per-task** status for observers and dashboards.

## Why it exists

Offload long-running media packaging (validate → normalize → adaptive renditions → Shaka Packager) from your API or workers. You publish a JSON job to a queue; MPE does the heavy lifting and reports progress in Redis.

## Features

- **Queue-driven** — one message per job; configurable queue and AMQP URL.
- **Multi-task jobs** — one message can list many tasks; concurrency is capped by `--workers`.
- **Video pipeline** — ffprobe validation, ffmpeg normalization, rendition ladder, Shaka Packager for `manifest.mpd` and `master.m3u8`.
- **Status in Redis** — aggregate job state under `job:{job_id}` and per-task state under `job:{job_id}:task:{task_id}`.
- **Retries** — optional per-task retries with delay before the message is rejected (and optionally dead-lettered).

## Quick start

1. **Install** [Rust](https://rustup.rs), **ffmpeg** / **ffprobe**, and **Shaka Packager** as `packager` on your `PATH` (see [Getting started](docs/getting-started.md#tooling)).
2. **Start** Redis and RabbitMQ (example: `docker compose up -d` using the repo’s `compose.yml`).
3. **Declare** the work queue, dead-letter exchange, and DLQ as described in [RabbitMQ](docs/rabbitmq.md).
4. **Run** the worker:

   ```bash
   cargo run --release
   ```

5. **Publish** a job JSON body to your queue (see [RabbitMQ](docs/rabbitmq.md)) and watch **Redis** keys (see [Redis](docs/redis.md)).

For paths, manifests, and serving playback, see [Output](docs/output.md).

## Documentation

| Doc | Purpose |
| --- | --- |
| [Getting started](docs/getting-started.md) | Prerequisites, configuration, running locally |
| [RabbitMQ](docs/rabbitmq.md) | Topology, publishing Composed jobs |
| [Redis](docs/redis.md) | Key schema, job and task JSON, TTL |
| [Output](docs/output.md) | Directory layout, artifacts, serving with CORS |
