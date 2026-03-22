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
  "started_at": "2026-03-22T04:00:00Z",
  "finished_at": "2026-03-22T04:01:00Z" | null
}
```

`finished_at` is null until the job reaches a terminal state.

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
- `processing` -> `processing`: subsequent step reports (step field changes)
- `processing` -> `finished`: `ProcessMessage::Finished` received
- `processing` -> `failed`: `ProcessMessage::Failed` received

Terminal states (`finished`, `failed`) must not be overwritten.

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

1. Consumer receives RabbitMQ delivery
2. Job key is written with `status: queued`, `total_tasks: N`, `completed_tasks: 0`, `failed_tasks: 0`
3. Tasks are dispatched to the thread pool
4. Job key updated to `status: processing`
5. Each task writes its task key as it progresses through pipeline steps
6. On task completion or failure, the job key counters are atomically incremented
7. When `completed_tasks + failed_tasks == total_tasks`, job key is updated to terminal state
8. Worker acknowledges the RabbitMQ delivery

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
