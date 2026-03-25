# RabbitMQ

MPE consumes **raw JSON** messages from a single **durable queue**. The body must deserialize to `ProcessJob` (see `src/jobs.rs` in this repository for the exact Rust types).

## What MPE expects

- **Content**: UTF-8 JSON, one job per message.
- **Consumer**: `basic_consume` on the queue name from `--queue` / `CONSUMER_QUEUE` (default `mpe_default_queue`).
- **Success**: message **ack** after all tasks in the job complete without failures.
- **Failure**: message **reject** with **requeue = false** if any task ultimately fails (after retries). Configure a **dead-letter exchange** on the work queue so those messages are not lost.

## Recommended topology

Use a **direct** dead-letter exchange (DLX) and a **dead-letter queue** (DLQ). Point the work queue’s `x-dead-letter-*` arguments at that exchange and a stable routing key; bind the DLQ to the DLX with the same routing key.

Example names (defaults used in code comments / docs):

| Object | Example name |
| --- | --- |
| DLX (direct, durable) | `mpe.dlx` |
| DLQ (durable queue) | `mpe_default_queue.dlq` |
| Work queue (durable) | `mpe_default_queue` |

**Work queue arguments**

- `x-dead-letter-exchange` → `mpe.dlx`
- `x-dead-letter-routing-key` → `mpe_default_queue.dlq` (must match the binding routing key to the DLQ)

**Bindings**

- DLQ bound to `mpe.dlx` with routing key `mpe_default_queue.dlq`.

You can create these in the RabbitMQ management UI (**Queues** / **Exchanges**) or via your infra-as-code. Integration tests in `tests/rabbitmq.rs` show a programmatic declare/bind pattern you can adapt.

### Operations note

When a job still fails after in-process retries, the worker rejects without requeue, so the message should land in the DLQ. Monitor DLQ depth in production; replay only after fixing the root cause.

## Job message format

MPE accepts **Composed** jobs only. The JSON must include `"type": "Composed"`, a `job_id`, and a `tasks_to_process` array.

| Field | Type | Required | Description |
| --- | --- | --- | --- |
| `type` | string | yes | Must be `"Composed"`. |
| `job_id` | string | yes | Logical job id; used in Redis keys and default output layout. |
| `tasks_to_process` | array | yes | List of tasks (may be empty — see Redis docs for behavior). |
| `base_output` | string or omitted | no | Absolute base directory; task dirs become `{base_output}/{job_id}/{task_id}/` when task has no `output_path`. |

Each **task** object:

| Field | Type | Required | Description |
| --- | --- | --- | --- |
| `id` | number | yes | Stable numeric id for this task within the job (used in paths and Redis). |
| `filepath` | string | yes | Input media path (must be readable by the worker). |
| `task_type` | string | yes | `"Video"` or `"Image"`. |
| `output_path` | string or omitted | no | Absolute output directory for this task; mutually exclusive with using `base_output` for that task’s resolution rules. |

**Example** — single video task with layout under `/var/mpe/out`:

```json
{
  "type": "Composed",
  "job_id": "job-001",
  "base_output": "/var/mpe/out",
  "tasks_to_process": [
    {
      "id": 1,
      "filepath": "/absolute/path/to/source.mp4",
      "task_type": "Video"
    }
  ]
}
```

**Example** — explicit output path (no `base_output`):

```json
{
  "type": "Composed",
  "job_id": "job-002",
  "tasks_to_process": [
    {
      "id": 1,
      "filepath": "/media/clip.mp4",
      "task_type": "Video",
      "output_path": "/var/mpe/out/job-002/1"
    }
  ]
}
```

## Publishing

- **Management UI** — open the work queue, **Publish message**, paste JSON, set payload type if needed.
- **Application code** — publish to the **queue name** (default exchange, routing key = queue name) with a JSON UTF-8 body.

Invalid JSON or wrong shape: the consumer logs an error and **rejects** the message (requeue false), so dead-lettering applies if configured.

## Related reading

- [Getting started](getting-started.md) — ports and `CONSUMER_QUEUE`
- [Redis](redis.md) — how `job_id` / task ids appear in keys
- [Output](output.md) — where files land for a given job/task
