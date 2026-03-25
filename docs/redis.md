# Redis

MPE writes **JSON** status blobs to Redis using **`SET` with TTL** (expiration). Keys use the job’s **`job_id`** from the message.

Source of truth: `src/processor/status.rs`.

## Key schema

| Key pattern | Purpose |
| --- | --- |
| `job:{job_id}` | Aggregate status for the whole job |
| `job:{job_id}:task:{task_id}` | Status for one task (`task_id` matches the JSON task `id`) |

Replace `{job_id}` and `{task_id}` with your concrete values, e.g. `job:job-001` and `job:job-001:task:1`.

**TTL**: each write sets expiration to **86400 seconds (24 hours)**. Long-running jobs that emit frequent updates refresh TTL on every write.

## Job document (`job:{job_id}`)

JSON object (serde field names below).

| Field | Type | Meaning |
| --- | --- | --- |
| `status` | string | One of: `queued`, `processing`, `completed`, `failed`, `partially_failed`. |
| `total_tasks` | number | Task count for this job (from the message). |
| `completed_tasks` | number | Tasks that finished successfully. |
| `failed_tasks` | number | Tasks that failed. |
| `started_at` | string or null | ISO-8601 UTC timestamp when the first task reported progress; null while only queued. |
| `finished_at` | string or null | ISO-8601 UTC when the job reached a terminal state; null while in progress. |
| `error` | string or null | Set for **pre-dispatch** failures (e.g. invalid `base_output`); usually null when tasks fail individually (see per-task keys). |

### `status` values

- **`queued`** — written when the job is accepted and the status forwarder initializes; no task progress yet.
- **`processing`** — at least one task has reported activity; not all tasks have settled.
- **`completed`** — every task succeeded (`failed_tasks == 0`).
- **`failed`** — every task failed (`completed_tasks == 0` and `failed_tasks == total_tasks`), or a pre-dispatch failure wrote this key with `error` set.
- **`partially_failed`** — mix of successes and failures.

**Edge case**: a job with **zero** tasks completes immediately as `completed` with `total_tasks: 0`.

## Task document (`job:{job_id}:task:{task_id}`)

| Field | Type | Meaning |
| --- | --- | --- |
| `status` | string | `processing`, `finished`, or `failed`. |
| `step` | string or null | Short pipeline step name while processing (e.g. `Validating`, `Packaging`); null when finished or failed. |
| `message` | string or null | Human-readable detail for the current step; null when finished or failed. |
| `error` | string or null | Error message when `status` is `failed`. |
| `updated_at` | string | ISO-8601 UTC time of the last write for this task. |

### Idempotency and ordering

After a task reaches **`finished`** or **`failed`**, later `processing` updates for that **same task id** are ignored (terminal state wins). Duplicate `finished` / `failed` notifications do not double-count on the job aggregate.

## Reading status

**CLI**

```bash
redis-cli GET "job:job-001"
redis-cli GET "job:job-001:task:1"
```

**TTL inspection**

```bash
redis-cli TTL "job:job-001"
redis-cli TTL "job:job-001:task:1"
```

**Application code** — `GET` the key, parse JSON, map `status` to your UI or state machine. Poll interval depends on your product; writes happen on pipeline steps and on task completion.

## Pre-dispatch failures

If the job cannot be dispatched (for example non-absolute `base_output`, or a task with neither `output_path` nor job `base_output`), MPE may write **`job:{job_id}`** with `status: "failed"`, `error` describing the problem, `started_at: null`, and **`completed_tasks` / `failed_tasks` left at zero** with `total_tasks` still reflecting the job size. No per-task keys are written for that failure mode.

## Related reading

- [RabbitMQ](rabbitmq.md) — `job_id` and task `id` in messages
- [Output](output.md) — correlating finished tasks with directories
