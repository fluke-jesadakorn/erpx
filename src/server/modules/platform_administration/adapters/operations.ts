import {platformHoldingBindingEffects} from "../../runtime/public/app-native-sources";
import { platformJobRecoveryFacts } from "../../workflow/public/platform-job-recovery";
import { platformOwnerProjectionSources as runtimePlatformSources } from "../../runtime/public/platform-owner-projection-sources";
import "server-only";
import { createHash } from "node:crypto";
import { ApiError } from "@/server/modules/runtime/public/http";
import type { PlatformContext } from "@/server/modules/identity_access/public/authorization";
import type { PostgresDatabase } from "@/server/modules/runtime/public/runtime/postgres";
import type { SqlDatabase } from "@/server/modules/runtime/public/runtime/types";
import { inspectMigrations, migrateDatabase } from "@/server/modules/runtime/public/runtime/migrations";
import type { ResourceSlot } from "@/lib/holding-registry";

import {createPlatformOperationsApplication} from '../application/operations-summary';
import type {PlatformOperationsSummary} from '../domain/operations-summary';
export type {PlatformHoldingOperation,PlatformDeadLetter,PlatformOperationsSummary} from '../domain/operations-summary';
export type PlatformHoldingOperationInput = {
  id: string;
  slug: string;
  displayName: string;
  db: SqlDatabase;
};

type HealthRow = { service: string; heartbeat_at: number; details_json: string };

export async function summarizePlatformOperations(actor:Pick<PlatformContext,'role'>,control:SqlDatabase,holdings:PlatformHoldingOperationInput[],now=Date.now()):Promise<PlatformOperationsSummary>{
 const database=(id:string,index:number)=>{const holding=holdings[index];if(!holding||holding.id!==id)throw new Error('HOLDING_OPERATION_DATABASE_REQUIRED');return holding.db;};
 return createPlatformOperationsApplication({
  async controlHealth(){return (await control.prepare(`SELECT service,heartbeat_at,details_json FROM (${runtimePlatformSources.runtime_health}) runtime_health ORDER BY service`).all<HealthRow>()).results.map(row=>({service:row.service,heartbeatAt:Number(row.heartbeat_at),details:row.details_json}));},
  async jobs(id,at,index){const facts=platformJobRecoveryFacts(database(id,index)),counts=await facts.counts(at);
   const deadLetters=(await facts.deadLetters()).results.map(row=>({id:row.id,eventType:row.event_type,aggregateId:row.aggregate_id,aggregateVersion:Number(row.aggregate_version),attempts:Number(row.attempts),lastError:row.last_error,deadLetterAt:Number(row.dead_letter_at)}));
   return {pendingEvents:Number(counts?.pending_events??0),deadLetterEvents:Number(counts?.dead_letter_events??0),claimedEvents:Number(counts?.claimed_events??0),oldestPendingAt:counts?.oldest_pending_at==null?null:Number(counts.oldest_pending_at),deadLetters};},
  async migrations(id,index){const status=await inspectMigrations(database(id,index) as PostgresDatabase,'holding-write');return {...status,track:'holding-write',error:null};},
 },{now:()=>now}).summarize(actor.role,holdings.map(({id,slug,displayName})=>({id,slug,displayName})));
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
  const prior = await platformJobRecoveryFacts(db).receipt(idempotencyKey);
  if (prior) {
    if (prior.request_hash !== digest) throw new ApiError(409, "IDEMPOTENCY_CONFLICT", "This command key belongs to a different recovery request.");
    if (!prior.result_json) throw new ApiError(409, "COMMAND_INCOMPLETE", "The previous recovery is still being finalized. Retry the same command.");
    return JSON.parse(prior.result_json) as RecoverDeadLetterResult;
  }
  const postgres = db as PostgresDatabase & { transaction?: <T>(action: (tx: SqlDatabase) => Promise<T>) => Promise<T> };
  if (typeof postgres.transaction !== "function") throw new ApiError(503, "DATABASE_UNAVAILABLE", "Job recovery is unavailable while the Holding database is offline.");
  try {
    return await postgres.transaction(async (tx) => {
      const changed = await platformJobRecoveryFacts(tx).requeue(now,input.eventId,input.expectedAttempts,input.expectedLastError);
      if (!changed) throw new ApiError(409, "VERSION_CONFLICT", "This failed job changed. Refresh the operations page before retrying.");
      const result: RecoverDeadLetterResult = {
        eventId: input.eventId,
        holdingId,
        status: "queued",
        previousAttempts: input.expectedAttempts,
        previousError: input.expectedLastError,
        recoveredAt: now,
      };
      await tx.batch(platformJobRecoveryFacts(tx).recoveryJournal({auditId:crypto.randomUUID(),key:idempotencyKey,operation,actorId:actor.userId,assignmentId:actor.assignmentId,digest,eventId:input.eventId,resultJson:JSON.stringify(result),now,receiptAuthorityJson:JSON.stringify({role:actor.role,assignmentId:actor.assignmentId,holdingId,reason:input.reason}),aggregateVersion:Number(changed.aggregate_version),reason:input.reason.trim(),auditAuthorityJson:JSON.stringify({role:actor.role,assignmentId:actor.assignmentId,holdingId})}));
      return result;
    });
  } catch (error) {
    if (error instanceof ApiError) throw error;
    const committed = await platformJobRecoveryFacts(db).receipt(idempotencyKey);
    if (committed?.request_hash === digest && committed.result_json) return JSON.parse(committed.result_json) as RecoverDeadLetterResult;
    throw error;
  }
}

export type RunHoldingMigrationInput = {
  holdingId: string;
  track: "holding-write";
  reason: string;
};

export type RunHoldingMigrationResult = {
  runId: string;
  holdingId: string;
  track: "holding-write";
  status: "succeeded";
  appliedBefore: number;
  appliedAfter: number;
  pending: string[];
  finishedAt: number;
};

function migrationDigest(
  actor: Pick<PlatformContext, "userId" | "assignmentId" | "role">,
  key: string,
  input: RunHoldingMigrationInput,
) {
  return createHash("sha256")
    .update(JSON.stringify({ actor: actor.userId, assignment: actor.assignmentId, role: actor.role, key, input }))
    .digest("hex");
}

/** Run the shipped Holding migrations through the same locked runner used at startup. */
export async function runHoldingMigrations(
  actor: Pick<PlatformContext, "userId" | "assignmentId" | "role">,
  control: PostgresDatabase,
  holding: PostgresDatabase,
  input: RunHoldingMigrationInput,
  idempotencyKey: string,
  now = Date.now(),
): Promise<RunHoldingMigrationResult> {
  if (actor.role !== "platform_admin") throw new ApiError(403, "FORBIDDEN", "Only a Platform administrator can run Holding migrations.");
  if (!/^[a-zA-Z0-9:_.-]{8,180}$/.test(idempotencyKey)) throw new ApiError(422, "COMMAND_KEY_REQUIRED", "Use a valid Idempotency-Key for this action.");
  if (!/^[a-z0-9][a-z0-9._:-]{1,180}$/.test(input.holdingId)) throw new ApiError(422, "VALIDATION_FAILED", "Choose a valid Holding.");
  if (input.track !== "holding-write") throw new ApiError(422, "VALIDATION_FAILED", "Only Holding write migrations can be run from this console.");
  if (input.reason.trim().length < 3 || input.reason.length > 1000) throw new ApiError(422, "VALIDATION_FAILED", "Explain why this migration run is safe.");
  const digest = migrationDigest(actor, idempotencyKey, input);
  const prior = await control.prepare(
    "SELECT id,request_hash,status,result_json,error FROM platform_migration_runs WHERE idempotency_key=?",
  ).bind(idempotencyKey).first<{ id: string; request_hash: string; status: string; result_json: string | null; error: string | null }>();
  if (prior) {
    if (prior.request_hash !== digest) throw new ApiError(409, "IDEMPOTENCY_CONFLICT", "This command key belongs to a different migration request.");
    if (prior.status === "running" || !prior.result_json) throw new ApiError(409, "MIGRATION_IN_PROGRESS", "The previous migration run is still being finalized. Retry the same command later.");
    if (prior.status === "failed") throw new ApiError(409, "MIGRATION_FAILED", "The previous migration run failed. Review the platform receipt before retrying.");
    return JSON.parse(prior.result_json) as RunHoldingMigrationResult;
  }

  const before = await inspectMigrations(holding, input.track);
  const runId = `platform-migration-${crypto.randomUUID()}`;
  try {
    await control.prepare(
      `INSERT INTO platform_migration_runs
         (id,idempotency_key,request_hash,holding_id,track,status,requested_by,assignment_id,reason,applied_before,applied_after,requested_at,started_at)
       VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?)`,
    ).bind(runId, idempotencyKey, digest, input.holdingId, input.track, "running", actor.userId, actor.assignmentId, input.reason.trim(), before.applied, before.applied, now, now).run();
  } catch (error) {
    if ((error as { code?: string }).code === "23505") {
      const raced = await control.prepare(
        "SELECT request_hash,status,result_json FROM platform_migration_runs WHERE idempotency_key=?",
      ).bind(idempotencyKey).first<{ request_hash: string; status: string; result_json: string | null }>();
      if (raced?.request_hash !== digest) throw new ApiError(409, "IDEMPOTENCY_CONFLICT", "This command key belongs to a different migration request.");
      if (raced?.status === "succeeded" && raced.result_json) return JSON.parse(raced.result_json) as RunHoldingMigrationResult;
      throw new ApiError(409, "MIGRATION_IN_PROGRESS", "Another migration run already owns this command key.");
    }
    throw error;
  }

  try {
    await migrateDatabase(holding, input.track);
    const after = await inspectMigrations(holding, input.track);
    const finishedAt = Date.now();
    const result: RunHoldingMigrationResult = {
      runId,
      holdingId: input.holdingId,
      track: input.track,
      status: "succeeded",
      appliedBefore: before.applied,
      appliedAfter: after.applied,
      pending: after.pending,
      finishedAt,
    };
    await control.prepare(
      "UPDATE platform_migration_runs SET status='succeeded',applied_after=?,result_json=?,finished_at=?,version=version+1 WHERE id=? AND status='running'",
    ).bind(after.applied, JSON.stringify(result), finishedAt, runId).run();
    return result;
  } catch (error) {
    const finishedAt = Date.now();
    const message = (error instanceof Error ? error.message : "Migration failed").slice(0, 1000);
    await control.prepare(
      "UPDATE platform_migration_runs SET status='failed',error=?,finished_at=?,version=version+1 WHERE id=? AND status='running'",
    ).bind(message, finishedAt, runId).run();
    throw new ApiError(500, "MIGRATION_FAILED", "The Holding migration failed. Review the platform migration receipt and database logs.");
  }
}

export type PlatformProvisioningRow = {
  id: string;
  holdingId: string;
  slug: string;
  displayName: string;
  writeBinding: string;
  readBinding: string;
  filesBinding: string;
  status: "provisioning" | "active" | "failed" | "suspended";
  requestedBy: string;
  reason: string;
  error: string | null;
  version: number;
  requestedAt: number;
  activatedAt: number | null;
  updatedAt: number;
};

export type PlatformProvisioningSnapshot = {
  rows: PlatformProvisioningRow[];
  availableSlots: ResourceSlot[];
};

export type ProvisionHoldingInput = {
  holdingId: string;
  slug: string;
  displayName: string;
  writeBinding: string;
  readBinding: string;
  filesBinding: string;
  reason: string;
};

export type ProvisionHoldingResult = {
  id: string;
  holdingId: string;
  slug: string;
  displayName: string;
  status: "active";
  version: number;
  activatedAt: number;
};

type ProvisioningDbRow = {
  id: string;
  holding_id: string;
  slug: string;
  display_name: string;
  write_binding: string;
  read_binding: string;
  files_binding: string;
  status: PlatformProvisioningRow["status"];
  requested_by: string;
  reason: string;
  error: string | null;
  version: number;
  requested_at: number;
  activated_at: number | null;
  updated_at: number;
};

function provisioningResult(row: ProvisioningDbRow): ProvisionHoldingResult {
  if (row.status !== "active" || row.activated_at == null)
    throw new ApiError(409, "PROVISIONING_INCOMPLETE", "Holding provisioning is still being prepared. Refresh the Platform operations page before continuing.");
  return {
    id: row.id,
    holdingId: row.holding_id,
    slug: row.slug,
    displayName: row.display_name,
    status: "active",
    version: Number(row.version),
    activatedAt: Number(row.activated_at),
  };
}

function provisioningDigest(
  actor: Pick<PlatformContext, "userId" | "assignmentId" | "role">,
  idempotencyKey: string,
  input: ProvisionHoldingInput,
) {
  return createHash("sha256")
    .update(JSON.stringify({ actor: actor.userId, assignment: actor.assignmentId, role: actor.role, idempotencyKey, input }))
    .digest("hex");
}

function mapProvisioningRow(row: ProvisioningDbRow): PlatformProvisioningRow {
  return {
    id: row.id,
    holdingId: row.holding_id,
    slug: row.slug,
    displayName: row.display_name,
    writeBinding: row.write_binding,
    readBinding: row.read_binding,
    filesBinding: row.files_binding,
    status: row.status,
    requestedBy: row.requested_by,
    reason: row.reason,
    error: row.error,
    version: Number(row.version),
    requestedAt: Number(row.requested_at),
    activatedAt: row.activated_at == null ? null : Number(row.activated_at),
    updatedAt: Number(row.updated_at),
  };
}

/** Read provisioning receipts and the pre-provisioned resource slots not bound to a Holding. */
export async function platformProvisioningSnapshot(
  control: SqlDatabase,
  slots: ResourceSlot[],
): Promise<PlatformProvisioningSnapshot> {
  const rows = (await control.prepare(
    `SELECT id,holding_id,slug,display_name,write_binding,read_binding,files_binding,status,
       requested_by,reason,error,version,requested_at,activated_at,updated_at
       FROM platform_holding_provisioning ORDER BY updated_at DESC,id DESC`,
  ).all<ProvisioningDbRow>()).results.map(mapProvisioningRow);
  const used = new Set(
    (await control.prepare(`SELECT holding_id,write_binding,read_binding,files_binding FROM (${runtimePlatformSources.holding_bindings}) WHERE status IN ('active','suspended')`).all<{ holding_id: string; write_binding: string; read_binding: string; files_binding: string }>()).results.flatMap((row) => [row.holding_id, row.write_binding, row.read_binding, row.files_binding]),
  );
  const pending = new Set(rows.filter((row) => row.status === "provisioning").flatMap((row) => [row.holdingId, row.writeBinding, row.readBinding, row.filesBinding]));
  return { rows, availableSlots: slots.filter((slot) => !slot.internal && !used.has(slot.id) && !used.has(slot.write) && !used.has(slot.read) && !used.has(slot.files) && !pending.has(slot.id)) };
}

/**
 * Activate a pre-provisioned Holding resource in one control-plane transaction.
 * The database and private file namespace are validated by the route before
 * this command is called; this function owns the durable receipt and binding.
 */
export async function provisionHolding(
  actor: Pick<PlatformContext, "userId" | "assignmentId" | "role">,
  control: PostgresDatabase,
  slot: ResourceSlot,
  input: ProvisionHoldingInput,
  idempotencyKey: string,
  now = Date.now(),
): Promise<ProvisionHoldingResult> {
  if (actor.role !== "platform_admin") throw new ApiError(403, "FORBIDDEN", "Only a Platform administrator can provision a Holding.");
  if (!/^[a-zA-Z0-9:_.-]{8,180}$/.test(idempotencyKey)) throw new ApiError(422, "COMMAND_KEY_REQUIRED", "Use a valid Idempotency-Key for this action.");
  if (!/^[a-z0-9]{3,24}$/.test(input.holdingId) || input.holdingId !== slot.id) throw new ApiError(422, "VALIDATION_FAILED", "Choose a configured, unbound Holding resource slot.");
  if (slot.internal) throw new ApiError(403, "RESERVED_RESOURCE_SLOT", "This resource slot is reserved for ERPX.");
  if (!/^[a-z][a-z0-9-]{2,39}$/.test(input.slug)) throw new ApiError(422, "VALIDATION_FAILED", "Use a lowercase company URL slug with 3–40 characters.");
  if (input.displayName.trim().length < 2 || input.displayName.trim().length > 120) throw new ApiError(422, "VALIDATION_FAILED", "Enter a Holding display name between 2 and 120 characters.");
  if (input.reason.trim().length < 10 || input.reason.length > 1000) throw new ApiError(422, "VALIDATION_FAILED", "Explain why this Holding should be activated.");
  const expectedBindings = [slot.write, slot.read, slot.files];
  if (input.writeBinding !== expectedBindings[0] || input.readBinding !== expectedBindings[1] || input.filesBinding !== expectedBindings[2]) throw new ApiError(422, "VALIDATION_FAILED", "The selected resource slot no longer matches the operator registry.");
  const digest = provisioningDigest(actor, idempotencyKey, input);
  const prior = await control.prepare("SELECT * FROM platform_holding_provisioning WHERE idempotency_key=?").bind(idempotencyKey).first<ProvisioningDbRow & { request_hash: string }>();
  if (prior) {
    if (prior.request_hash !== digest) throw new ApiError(409, "IDEMPOTENCY_CONFLICT", "This command key belongs to a different provisioning request.");
    return provisioningResult(prior);
  }
  const result = await control.transaction(async (tx) => {
    const existing = await tx.prepare("SELECT * FROM platform_holding_provisioning WHERE holding_id=? FOR UPDATE").bind(input.holdingId).first<ProvisioningDbRow & { request_hash: string; idempotency_key: string }>();
    if (existing && existing.status === "active") {
      if (existing.request_hash !== digest || existing.slug !== input.slug || existing.display_name !== input.displayName.trim()) throw new ApiError(409, "HOLDING_ALREADY_BOUND", "This resource slot is already bound to a different active Holding or provisioning request.");
      return provisioningResult(existing);
    }
    const slugConflict = await tx.prepare(`SELECT holding_id FROM (${runtimePlatformSources.holding_bindings}) WHERE slug=? AND holding_id<>?`).bind(input.slug, input.holdingId).first<{ holding_id: string }>();
    if (slugConflict) throw new ApiError(409, "SLUG_UNAVAILABLE", "Choose another company URL; it is already assigned to a Holding.");
    const bindingConflict = await tx.prepare(`SELECT holding_id FROM (${runtimePlatformSources.holding_bindings}) WHERE holding_id<>? AND (write_binding IN (?,?,?) OR read_binding IN (?,?,?) OR files_binding IN (?,?,?)) LIMIT 1`).bind(input.holdingId, slot.write, slot.read, slot.files, slot.write, slot.read, slot.files, slot.write, slot.read, slot.files).first<{ holding_id: string }>();
    if (bindingConflict) throw new ApiError(409, "RESOURCE_ALREADY_BOUND", "This resource slot is already bound to another Holding.");
    const current = existing?.status === "failed" ? existing : null;
    const id = current?.id ?? `holding-provisioning-${crypto.randomUUID()}`;
    const version = Number(current?.version ?? 0) + 1;
    const validatedAt = now + 1;
    const activatedAt = now + 2;
    await tx.batch([
      current
        ? tx.prepare("UPDATE platform_holding_provisioning SET slug=?,display_name=?,write_binding=?,read_binding=?,files_binding=?,status='provisioning',requested_by=?,assignment_id=?,reason=?,request_hash=?,idempotency_key=?,version=?,error=NULL,requested_at=?,updated_at=? WHERE id=?").bind(input.slug, input.displayName.trim(), slot.write, slot.read, slot.files, actor.userId, actor.assignmentId, input.reason.trim(), digest, idempotencyKey, version, now, now, id)
        : tx.prepare("INSERT INTO platform_holding_provisioning(id,holding_id,slug,display_name,write_binding,read_binding,files_binding,status,requested_by,assignment_id,reason,request_hash,idempotency_key,requested_at,updated_at) VALUES(?,?,?,?,?,?,?,'provisioning',?,?,?,?,?,?,?)").bind(id, input.holdingId, input.slug, input.displayName.trim(), slot.write, slot.read, slot.files, actor.userId, actor.assignmentId, input.reason.trim(), digest, idempotencyKey, now, now),
      tx.prepare("INSERT INTO platform_holding_provisioning_events(id,provisioning_id,holding_id,action,actor_id,assignment_id,reason,detail_json,created_at) VALUES(?,?,?,?,?,?,?,?,?)").bind(crypto.randomUUID(), id, input.holdingId, current ? "retry_started" : "started", actor.userId, actor.assignmentId, input.reason.trim(), JSON.stringify({ slug: input.slug, resourceSlot: slot.id }), now),
      tx.prepare("INSERT INTO platform_holding_provisioning_events(id,provisioning_id,holding_id,action,actor_id,assignment_id,reason,detail_json,created_at) VALUES(?,?,?,?,?,?,?,?,?)").bind(crypto.randomUUID(), id, input.holdingId, "validated", actor.userId, actor.assignmentId, input.reason.trim(), JSON.stringify({ bindings: expectedBindings }), validatedAt),
      platformHoldingBindingEffects(tx).activate(input.holdingId, input.slug, input.displayName.trim(), slot.write, slot.read, slot.files),
      tx.prepare("UPDATE platform_holding_provisioning SET status='active',activated_at=?,updated_at=?,version=? WHERE id=?").bind(activatedAt, activatedAt, version + 1, id),
      tx.prepare("INSERT INTO platform_holding_provisioning_events(id,provisioning_id,holding_id,action,actor_id,assignment_id,reason,detail_json,created_at) VALUES(?,?,?,?,?,?,?,?,?)").bind(crypto.randomUUID(), id, input.holdingId, "activated", actor.userId, actor.assignmentId, input.reason.trim(), JSON.stringify({ version: version + 1 }), activatedAt),
    ]);
    return { id, holdingId: input.holdingId, slug: input.slug, displayName: input.displayName.trim(), status: "active" as const, version: version + 1, activatedAt };
  }).catch(async (error) => {
    if (error instanceof ApiError) throw error;
    if ((error as { code?: string }).code === "23505") {
      const raced = await control.prepare("SELECT * FROM platform_holding_provisioning WHERE idempotency_key=?").bind(idempotencyKey).first<ProvisioningDbRow & { request_hash: string }>();
      if (raced?.request_hash !== digest) throw new ApiError(409, "IDEMPOTENCY_CONFLICT", "This command key belongs to a different provisioning request.");
      if (raced?.status === "active") return provisioningResult(raced);
      throw new ApiError(409, "PROVISIONING_IN_PROGRESS", "Another request is already provisioning this Holding. Refresh the Platform operations page and retry later.");
    }
    const message = (error instanceof Error ? error.message : "Holding provisioning failed").slice(0, 1000);
    await control.prepare("UPDATE platform_holding_provisioning SET status='failed',error=?,updated_at=?,version=version+1 WHERE idempotency_key=? AND status='provisioning'").bind(message, Date.now(), idempotencyKey).run().catch(() => {});
    const apiError = new ApiError(500, "PROVISIONING_FAILED", "Holding provisioning failed. Review the Platform receipt and resource health before retrying.");
    apiError.cause = error;
    throw apiError;
  });
  return result;
}
