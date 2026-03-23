# Redis Status Communication Spec

## Context

MPE uses RabbitMQ for job intake but has no request/response channel back to the caller.
Redis keys serve as the communication layer for job and task status. Callers poll these keys
to observe progress and detect completion or failure.

---

## Key Schema

### Job key

```
job:{job_id}
```

Represents the aggregate state of the entire job. Written by the worker coordinating the job,
not by individual task threads.

### Task key

```
job:{job_id}:task:{task_id}
```

Represents the state of a single task within a job. Written by the task's pipeline thread
as it progresses through steps.

---

## Value Format

All values are JSON objects. Plain strings are explicitly rejected to allow future extension
without breaking callers.

### Job value

```json
{
  "status": "queued" | "processing" | "completed" | "failed" | "partially_failed",
  "total_tasks": 5,
  "completed_tasks": 3,
  "failed_tasks": 1,
  "started_at": "2026-03-22T04:00:00Z" | null,
  "finished_at": "2026-03-22T04:01:00Z" | null,
  "error": "human-readable reason" | null
}
```

`started_at` is null until the first `Processing` message is received. `finished_at` is null
until the job reaches a terminal state. `error` is only set for pre-dispatch failures. It
signals the job never ran. It is never set when individual tasks fail; task keys carry those
errors instead.

### Task value

```json
{
  "status": "processing" | "finished" | "failed",
  "step": "Validating" | "Normalizing" | "GeneratingChildren" | "Packaging" | "Persisting" | null,
  "message": "human-readable description of current step",
  "error": "error message if failed" | null,
  "updated_at": "2026-03-22T04:00:14Z"
}
```

`step` reflects the active pipeline step during `processing`. It is null on `finished` or `failed`.
`error` is null unless `status` is `failed`.

---

## State Machine

### Task states

```
                +--------+
                | queued |   (implicit, key does not exist yet)
                +--------+
                     |
                     v
              +------------+
              | processing |  <-- written on each pipeline step report
              +------------+
               /           \
              v             v
        +----------+    +--------+
        | finished |    | failed |
        +----------+    +--------+
```

Transitions:

- `queued` -> `processing`: first `ProcessMessage::Processing` received
- `queued` -> `failed`: `ProcessMessage::Failed` received before any `Processing` (e.g. early validation failure)
- `processing` -> `processing`: subsequent step reports (step field changes)
- `processing` -> `finished`: `ProcessMessage::Finished` received
- `processing` -> `failed`: `ProcessMessage::Failed` received

Terminal states (`finished`, `failed`) must not be overwritten.

Any terminal message (`Finished` or `Failed`) for a task_id that has already reached a terminal
state must be silently ignored. This prevents duplicate messages from double-counting counters
and corrupting the job terminal state.

### Job states

```
queued -> processing -> completed
                     -> failed
                     -> partially_failed
```

- `queued`: job key written when the job is accepted, before any task is dispatched
- `processing`: first task begins
- `completed`: all tasks finished successfully
- `failed`: all tasks failed
- `partially_failed`: at least one task failed, at least one succeeded

---

## Lifecycle

### Happy path

1. Consumer receives RabbitMQ delivery and parses the job payload
2. Job key is written with `status: queued`, `total_tasks: N`, `completed_tasks: 0`, `failed_tasks: 0`, `error: null`
3. Tasks are validated and dispatched to the thread pool
4. Job key updated to `status: processing` on first `Processing` message; `started_at` is set
5. Each task writes its task key as it progresses through pipeline steps
6. On task completion or failure, the job key counters are incremented
7. When `completed_tasks + failed_tasks == total_tasks`, job key is updated to terminal state with `finished_at`
8. Worker acknowledges the RabbitMQ delivery

### Pre-dispatch failure

If the job fails after a valid `job_id` is available but before any task is dispatched
(e.g. invalid output path, malformed task list), the consumer writes:

```json
{
  "status": "failed",
  "total_tasks": N,
  "completed_tasks": 0,
  "failed_tasks": 0,
  "started_at": null,
  "finished_at": "<timestamp>",
  "error": "<reason>"
}
```

No task keys are written. The RabbitMQ delivery is rejected.

If the failure occurs before a valid `job_id` is extracted (e.g. JSON parse error),
nothing is written to Redis. The delivery is rejected and the caller receives no Redis signal.

---

## TTL

All keys expire after 24 hours. This covers reasonable polling windows and prevents unbounded
Redis growth. TTL is reset on every write so long-running jobs do not expire mid-flight.

Proposed values:

- Task keys: 24h
- Job key: 24h

---

## Decisions

1. **Task writes are full overwrites.** Each `ProcessMessage` replaces the entire task key value.
   `updated_at` is sufficient to order events; no partial update needed.

2. **Timestamps are job-level only.** Task keys carry `updated_at` per write.
   Job key carries `started_at` and `finished_at` for the job as a whole.

3. **Errors are plain strings.** The pipeline is linear; a single message is sufficient
   to describe why a task failed. No error codes or structured error objects.

4. **The status-forwarding thread owns all job key writes.** It is the single serialized
   point receiving all `ProcessMessage` events. It increments counters and transitions
   job state. Task threads never touch the job key directly.

5. **RabbitMQ ack/nack behavior on failure is out of scope for this spec.**

6. **Settled task tracking is in-memory.** The `StatusForwarder` maintains a `HashSet<u64>`
   of task_ids that have reached a terminal state. Before processing any terminal message,
   it checks this set. If the task_id is already present, the message is ignored. If not,
   the task_id is inserted and the counter is incremented. This is safe because the forwarder
   is single-threaded. No concurrent access, no Redis round-trips needed.

7. **Redis write failure is fatal.** If any Redis write fails, the forwarder logs the error
   and panics. Continuing without a healthy Redis is considered impossible. The caller
   would have no visibility into job state.

8. **Duplicate `job_id` overwrites existing state.** The application is stateless between
   operations. If `init()` is called for a `job_id` that already exists in Redis, the
   existing key is overwritten. No idempotency check is performed.

9. **A task may reach a terminal state without a prior `Processing` message.** For example,
   validation may fail before any step is reported. The task key is created directly with
   `failed` status. The only invalid transition is returning to `queued`. Once any message
   is written for a task, it cannot go back.

10. **`started_at` is set on the first `Processing` message received.** The queued write
    at `init()` does not set `started_at`. This reflects when actual work began, not when
    the job was accepted.

11. **Duplicate `task_id` within a job is handled by last-write-wins.** `Processing`
    messages for a duplicate `task_id` overwrite each other freely. Terminal deduplication
    via the settled set prevents double-counting of counters.
