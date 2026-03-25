# MPE — Media Processing Engine

MPE is a background media processing service written in Rust. It consumes jobs from a RabbitMQ queue, processes video (and image) tasks concurrently, and writes the output to disk. Each job contains one or more tasks. Each task runs through a pipeline that validates, normalizes, generates adaptive renditions, and packages the media into HLS and MPEG-DASH (CMAF) format.

## How it works

1. A job message arrives on the RabbitMQ queue.
2. The consumer parses the message into a `ProcessJob` (either `Composed` with multiple tasks, or `Unique` with a single task attached to an existing job).
3. Each task is dispatched to a thread pool. Workers run in parallel up to the configured limit.
4. Every task runs through the video pipeline:
   - **Validate** — checks the container format and runs ffprobe to confirm the file is readable.
   - **Normalize** — re-encodes to H.264/AAC, yuv420p, with loudnorm audio leveling and faststart for streaming.
   - **Generate renditions** — produces a top-down ladder of resolutions (up to 2160p down to 144p) based on the source resolution. If a rendition matches the source resolution, the file is copied instead of re-encoded.
   - **Package** — runs Shaka Packager to produce CMAF segments, an MPEG-DASH manifest (`manifest.mpd`), and an HLS master playlist (`master.m3u8`).
5. Task status updates are written to Redis under the key `job:{job_id}:task:{task_id}`.

## Dependencies

**Rust** — https://rustup.rs

**ffmpeg / ffprobe** — must be installed and available in PATH with libx264 support.

On Debian/Ubuntu:

```bash
apt install ffmpeg
```

On macOS:

```bash
brew install ffmpeg
```

**Shaka Packager** — download the binary for your platform from the releases page:
https://github.com/shaka-project/shaka-packager/releases/tag/v3.7.0

Rename the binary to `packager` and place it somewhere in your PATH:

```bash
# Linux example
chmod +x packager-linux-x64
mv packager-linux-x64 /usr/local/bin/packager
```

```bash
# macOS example
chmod +x packager-osx-x64
mv packager-osx-x64 /usr/local/bin/packager
```

Verify both are available:

```bash
ffmpeg -version
ffprobe -version
packager --version
```

**Docker** — required to run RabbitMQ and Redis via `docker compose`.

## Running locally

**1. Start dependencies**

```bash
docker compose up -d
```

This starts RabbitMQ (port 5672, management UI on 15672) and Redis (port 6379).

**2. Create the queue and dead-letter routing**

Open the RabbitMQ management UI at `http://localhost:15672` (guest/guest).

1. Create a **direct** exchange, e.g. `mpe.dlx`.
2. Create a **dead-letter queue**, e.g. `mpe_default_queue.dlq`, durable, bound to `mpe.dlx` with routing key `mpe_default_queue.dlq` (routing key can match your naming convention).
3. Create the **work queue** `mpe_default_queue` (durable) with **arguments**:
   - `x-dead-letter-exchange` = `mpe.dlx`
   - `x-dead-letter-routing-key` = `mpe_default_queue.dlq` (must match the binding above)

When a job still fails after in-process task retries, the worker **rejects** the message with `requeue=false`. With these arguments, RabbitMQ **dead-letters** the message to the DLQ instead of dropping it. Monitor the DLQ depth in production (metrics/alerts); replay messages manually after fixing the cause.

**3. Run**

```bash
cargo run --release
```

Available options:

| Flag                      | Env                        | Default                     | Description                                                                 |
| ------------------------- | -------------------------- | --------------------------- | --------------------------------------------------------------------------- |
| `--addr`                  | `AMQP_ADDR`                | `amqp://127.0.0.1:5672/%2f` | RabbitMQ address                                                            |
| `--queue`                 | `CONSUMER_QUEUE`           | `mpe_default_queue`         | Queue name                                                                  |
| `--workers`               | `MPE_WORKERS`              | `1`                         | Number of concurrent task workers                                           |
| `--task-retries`          | `MPE_TASK_RETRIES`         | `0`                         | Extra attempts per task after a failure (total attempts = 1 + this value) |
| `--task-retry-delay-ms`   | `MPE_TASK_RETRY_DELAY_MS`  | `1000`                      | Delay between task retry attempts (milliseconds)                            |
| `--redis-addr`            | `REDIS_ADDR`               | `redis://127.0.0.1:6379`    | Redis address                                                               |

**4. Publish a job**

Go to the RabbitMQ management UI, select the queue, and publish a message with this payload:

```json
{
  "type": "Composed",
  "job_id": "job-001",
  "tasks_to_process": [
    {
      "id": 1,
      "filepath": "/absolute/path/to/video.mp4",
      "task_type": "Video"
    }
  ]
}
```

**5. Check output**

Processed files are written to `{output}/{job_id}/{task_id}/`. Each task produces:

- `manifest.mpd` — MPEG-DASH manifest
- `master.m3u8` — HLS master playlist
- `*.m4s` — CMAF media segments
- `*.mp4` — init segments per rendition

To preview in a browser, serve the output directory with CORS headers:

```bash
cd output && python3 -c "
import http.server

class CORSHandler(http.server.SimpleHTTPRequestHandler):
    def end_headers(self):
        self.send_header('Access-Control-Allow-Origin', '*')
        super().end_headers()

http.server.HTTPServer(('', 8080), CORSHandler).serve_forever()
"
```

Then open the DASH.js reference player and load:

```
http://localhost:8080/job-001/1/manifest.mpd
```

**6. Check task status**

```bash
redis-cli GET "job:job-001:task:1"
```
