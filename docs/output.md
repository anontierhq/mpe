# Output files

This document describes **where** MPE writes artifacts, **what** a successful **video** task produces, and **how** to serve files for browser players.

## Output directory resolution

For each task, the worker resolves an **absolute** directory:

- If the task has **`output_path`**, that path is used (and the job must not also supply `base_output` for that task’s resolution rules — see below).
- Otherwise, with **`base_output`** on the job: `{base_output}/{job_id}/{task_id}/`.

Rules enforced by the worker:

- **Absolute paths only** for `base_output` and `output_path`.
- Do not set **both** `base_output` and `output_path` on the same task (configuration error).

## Safety: minimum path depth

Before writing, the video pipeline’s persist step **refuses** output paths that are too shallow (fewer than **four** path components). This reduces the risk of misconfigured jobs deleting or overwriting sensitive directories. Prefer deep, dedicated roots such as `/var/mpe/output/my-tenant/job-id/task-id`.

## Video task layout

On success, the task output directory typically contains:

| Artifact | Role |
| --- | --- |
| `manifest.mpd` | MPEG-DASH manifest (Shaka Packager) |
| `master.m3u8` | HLS master playlist |
| `*_*.m4s` / `audio_*.m4s` | CMAF media segments (naming depends on rendition labels and audio) |
| `*.mp4` | Init segments per stream (video renditions and audio as configured by packager) |
| `matrix.mp4` | Optional sidecar when the matrix step produced a file (copied into the output dir) |

Fragment/segment durations are configured in the packager step (e.g. 2s fragments, 6s segments for packaged output — see `src/processor/video/package_step.rs`).

## Image tasks

`task_type: "Image"` is accepted in job JSON, but the current **image processor is a no-op** (it completes without writing packaged output). Do not expect HLS/DASH artifacts for image tasks until that pipeline is implemented.

## Clearing previous runs

If the output directory **already exists**, MPE **removes it entirely** before writing new results (after the depth check). Plan paths so concurrent jobs never share the same directory.

## Serving for playback

Browsers and players often need **HTTP** access and **CORS** headers when the player runs on another origin.

**Example** — static server from the parent of your job folders with permissive CORS:

```bash
cd /var/mpe/out && python3 -c "
import http.server

class CORSHandler(http.server.SimpleHTTPRequestHandler):
    def end_headers(self):
        self.send_header('Access-Control-Allow-Origin', '*')
        super().end_headers()

http.server.HTTPServer(('', 8080), CORSHandler).serve_forever()
"
```

If `base_output` is `/var/mpe/out` and `job_id` is `job-001` with task `1`, the DASH manifest URL might be:

`http://localhost:8080/job-001/1/manifest.mpd`

Use a reference player (e.g. [dash.js](https://github.com/Dash-Industry-Forum/dash.js)) or your app’s HLS/DASH stack, pointed at that URL.

## Correlation with Redis

While a task runs, Redis key `job:{job_id}:task:{task_id}` shows `processing` with `step` / `message`. When `status` is `finished`, the directory above should contain the packaged files. If `failed`, check `error` on the task key and worker logs.

## Related reading

- [Getting started](getting-started.md) — `base_output` and flags
- [RabbitMQ](rabbitmq.md) — job JSON examples
- [Redis](redis.md) — task and job status fields
