import "server-only";
import { createHash } from "node:crypto";
import { ApiError } from "./http";
import type { PlatformContext } from "./authorization";
import type { PostgresDatabase } from "./runtime/postgres";
import type { SqlDatabase } from "./runtime/types";

export type PlatformHoldingOperation = {
  id: string;
  slug: string;
  displayName: string;
  status: "healthy" | "pending" | "attention" | "unavailable";
  pendingEvents: number;
  deadLetterEvents: number;
  claimedEvents: number;
  oldestPendingAt: number | null;
  jobHeartbeatAt: number | null;
  error: string | null;
  deadLetters: PlatformDeadLetter[];
};

export type PlatformDeadLetter = {
  id: string;
  eventType: string;
  aggregateId: string;
  aggregateVersion: number;
  attempts: number;
  lastError: string | null;
  deadLetterAt: number;
};

export type PlatformOperationsSummary = {
  generatedAt: number;
  actorRole: string;
  controlHealth: { service: string; heartbeatAt: number; details: string }[];
  holdings: PlatformHoldingOperation[];
  totals: {
    holdings: number;
    pendingEvents: number;
    deadLetterEvents: number;
    claimedEvents: number;
  };
};

export type PlatformHoldingOperationInput = {
  id: string;
  slug: string;
  displayName: string;
  db: SqlDatabase;
};

type EventCounts = {
  pending_events: number | string;
  dead_letter_events: number | string;
  claimed_events: number | string;
  oldest_pending_at: number | string | null;
};

type HealthRow = { service: string; heartbeat_at: number; details_json: string };
type DeadLetterRow = {
  id: string;
  event_type: string;
  aggregate_id: string;
  aggregate_version: number;
  attempts: number;
  last_error: string | null;
  dead_letter_at: number;
};

/**
 * Read the control-plane and Holding job state used by the Platform overview.
 * The Holding query is deliberately isolated per binding: one unavailable
 * database is displayed as unavailable while the rest of the control plane
 * remains observable.
 */
export async function summarizePlatformOperations(
  actor: Pick<PlatformContext, "role">,
  control: SqlDatabase,
  holdings: PlatformHoldingOperationInput[],
  now = Date.now(),
): Promise<PlatformOperationsSummary> {
  const controlHealth = (
    await control
      .prepare("SELECT service,heartbeat_at,details_json FROM runtime_health ORDER BY service")
      .all<HealthRow>()
  ).results.map((row) => ({
    service: row.service,
    heartbeatAt: Number(row.heartbeat_at),
    details: row.details_json,
  }));
  const jobsHeartbeatAt = controlHealth.find((health) => health.service === "jobs")?.heartbeatAt ?? null;

  const rows = await Promise.all(
    holdings.map(async (holding): Promise<PlatformHoldingOperation> => {
      try {
        const counts = await holding.db
          .prepare(
            `SELECT
               COUNT(*) FILTER (WHERE processed_at IS NULL AND dead_letter_at IS NULL AND (claim_until IS NULL OR claim_until<=?)) pending_events,
               COUNT(*) FILTER (WHERE dead_letter_at IS NOT NULL) dead_letter_events,
               COUNT(*) FILTER (WHERE processed_at IS NULL AND dead_letter_at IS NULL AND claim_until>?) claimed_events,
               MIN(created_at) FILTER (WHERE processed_at IS NULL AND dead_letter_at IS NULL) oldest_pending_at
             FROM outbox_events`,
          )
          .bind(now, now)
          .first<EventCounts>();
        const pendingEvents = Number(counts?.pending_events ?? 0);
        const deadLetterEvents = Number(counts?.dead_letter_events ?? 0);
        const claimedEvents = Number(counts?.claimed_events ?? 0);
        const deadLetters = (await holding.db.prepare(
          `SELECT id,event_type,aggregate_id,aggregate_version,attempts,last_error,dead_letter_at
             FROM outbox_events WHERE dead_letter_at IS NOT NULL
             ORDER BY dead_letter_at ASC,id ASC LIMIT 20`,
        ).all<DeadLetterRow>()).results.map((row) => ({
          id: row.id,
          eventType: row.event_type,
          aggregateId: row.aggregate_id,
          aggregateVersion: Number(row.aggregate_version),
          attempts: Number(row.attempts),
          lastError: row.last_error,
          deadLetterAt: Number(row.dead_letter_at),
        }));
        return {
          id: holding.id,
          slug: holding.slug,
          displayName: holding.displayName,
          status: deadLetterEvents > 0
            ? "attention"
            : pendingEvents > 0 || claimedEvents > 0
              ? "pending"
              : "healthy",
          pendingEvents,
          deadLetterEvents,
          claimedEvents,
          oldestPendingAt: counts?.oldest_pending_at == null ? null : Number(counts.oldest_pending_at),
          jobHeartbeatAt: jobsHeartbeatAt,
          error: null,
          deadLetters,
        };
      } catch {
        return {
          id: holding.id,
          slug: holding.slug,
          displayName: holding.displayName,
          status: "unavailable",
          pendingEvents: 0,
          deadLetterEvents: 0,
          claimedEvents: 0,
          oldestPendingAt: null,
          jobHeartbeatAt: jobsHeartbeatAt,
          error: "Holding database or operational tables are unavailable.",
          deadLetters: [],
        };
      }
    }),
  );
  return {
    generatedAt: now,
    actorRole: actor.role,
    controlHealth,
    holdings: rows,
    totals: {
      holdings: rows.length,
      pendingEvents: rows.reduce((sum, row) => sum + row.pendingEvents, 0),
      deadLetterEvents: rows.reduce((sum, row) => sum + row.deadLetterEvents, 0),
      claimedEvents: rows.reduce((sum, row) => sum + row.claimedEvents, 0),
    },
  };
}

export type RecoverDeadLetterInput = {
  eventId: string;
  expectedAttempts: number;
  expectedLastError: string | null;
  reason: string;
};

export type RecoverDeadLetterResult = {
  eventId: string;
  holdingId: string;
  status: "queued";
  previousAttempts: number;
  previousError: string | null;
  recoveredAt: number;
};

function recoveryDigest(actor: Pick<PlatformContext, "userId" | "assignmentId" | "role">, operation: string, key: string, holdingId: string, input: RecoverDeadLetterInput) {
  return createHash("sha256").update(JSON.stringify({ actor: actor.userId, assignment: actor.assignmentId, role: actor.role, operation, key, holdingId, input })).digest("hex");
}

/**
 * Requeue one dead-lettered projection job after an explicit, audited review.
 * The optimistic snapshot prevents an operator from reviving a job that changed
 * while the review form was open. The command receipt makes retries safe.
 */
export async function recoverDeadLetterJob(
  actor: Pick<PlatformContext, "userId" | "assignmentId" | "role">,
  db: SqlDatabase,
  holdingId: string,
  input: RecoverDeadLetterInput,
  idempotencyKey: string,
  now = Date.now(),
): Promise<RecoverDeadLetterResult> {
  if (actor.role !== "platform_admin") throw new ApiError(403, "FORBIDDEN", "Only a Platform administrator can recover failed jobs.");
  if (!/^[a-zA-Z0-9:_.-]{8,180}$/.test(idempotencyKey)) throw new ApiError(422, "COMMAND_KEY_REQUIRED", "Use a valid Idempotency-Key for this action.");
  if (!/^[a-z0-9][a-z0-9._:-]{1,180}$/.test(input.eventId)) throw new ApiError(422, "VALIDATION_FAILED", "Choose a valid failed job.");
  if (!Number.isSafeInteger(input.expectedAttempts) || input.expectedAttempts < 1) throw new ApiError(422, "VALIDATION_FAILED", "The failed job attempt count is invalid.");
  if (input.expectedLastError !== null && input.expectedLastError.length > 64) throw new ApiError(422, "VALIDATION_FAILED", "The failed job error is invalid.");
  if (input.reason.trim().length < 3 || input.reason.length > 1000) throw new ApiError(422, "VALIDATION_FAILED", "Explain why this failed job is safe to retry.");
  const operation = "platform.job.retry";
  const digest = recoveryDigest(actor, operation, idempotencyKey, holdingId, input);
  const prior = await db.prepare("SELECT request_hash,result_json FROM command_receipts WHERE command_id=?").bind(idempotencyKey).first<{ request_hash: string; result_json: string | null }>();
  if (prior) {
    if (prior.request_hash !== digest) throw new ApiError(409, "IDEMPOTENCY_CONFLICT", "This command key belongs to a different recovery request.");
    if (!prior.result_json) throw new ApiError(409, "COMMAND_INCOMPLETE", "The previous recovery is still being finalized. Retry the same command.");
    return JSON.parse(prior.result_json) as RecoverDeadLetterResult;
  }
  const postgres = db as PostgresDatabase & { transaction?: <T>(action: (tx: SqlDatabase) => Promise<T>) => Promise<T> };
  if (typeof postgres.transaction !== "function") throw new ApiError(503, "DATABASE_UNAVAILABLE", "Job recovery is unavailable while the Holding database is offline.");
  try {
    return await postgres.transaction(async (tx) => {
      const changed = await tx.prepare(
        `UPDATE outbox_events SET processed_at=NULL,published_at=NULL,claim_until=NULL,lease_token=NULL,
             next_attempt_at=?,last_error=NULL,dead_letter_at=NULL,attempts=0
           WHERE id=? AND dead_letter_at IS NOT NULL AND attempts=? AND last_error IS NOT DISTINCT FROM ?
           RETURNING id,aggregate_version`,
      ).bind(now, input.eventId, input.expectedAttempts, input.expectedLastError).first<{ id: string; aggregate_version: number }>();
      if (!changed) throw new ApiError(409, "VERSION_CONFLICT", "This failed job changed. Refresh the operations page before retrying.");
      const result: RecoverDeadLetterResult = {
        eventId: input.eventId,
        holdingId,
        status: "queued",
        previousAttempts: input.expectedAttempts,
        previousError: input.expectedLastError,
        recoveredAt: now,
      };
      await tx.batch([
        tx.prepare("INSERT INTO command_receipts(command_id,operation,actor_id,request_hash,record_id,result_json,created_at,authority_json) VALUES(?,?,?,?,?,?,?,?)")
          .bind(idempotencyKey, operation, actor.userId, digest, input.eventId, JSON.stringify(result), now, JSON.stringify({ role: actor.role, assignmentId: actor.assignmentId, holdingId, reason: input.reason })),
        tx.prepare("INSERT INTO audit_events(id,command_id,actor_id,assignment_id,action,record_type,record_id,record_version,purpose,created_at,authority_json) VALUES(?,?,?,?,?,?,?,?,?,?,?)")
          .bind(crypto.randomUUID(), idempotencyKey, actor.userId, actor.assignmentId, operation, "runtime_job", input.eventId, Number(changed.aggregate_version), input.reason.trim(), now, JSON.stringify({ role: actor.role, assignmentId: actor.assignmentId, holdingId })),
      ]);
      return result;
    });
  } catch (error) {
    if (error instanceof ApiError) throw error;
    const committed = await db.prepare("SELECT request_hash,result_json FROM command_receipts WHERE command_id=?").bind(idempotencyKey).first<{ request_hash: string; result_json: string | null }>();
    if (committed?.request_hash === digest && committed.result_json) return JSON.parse(committed.result_json) as RecoverDeadLetterResult;
    throw error;
  }
}
