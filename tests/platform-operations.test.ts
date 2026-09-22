import { beforeAll, beforeEach, expect, it } from "vitest";
import { applyMigrations, env } from "./runtime-env";
import { recoverDeadLetterJob, runHoldingMigrations, summarizePlatformOperations, platformProvisioningSnapshot, provisionHolding } from "../src/server/platform-operations";

const now = 1_800_000_000_000;

beforeAll(async () => {
  await applyMigrations(env.CONTROL_DB, env.CONTROL_TEST_MIGRATIONS);
  await applyMigrations(env.H001_DB, env.STAFF_TEST_MIGRATIONS);
});

beforeEach(async () => {
  await env.CONTROL_DB.prepare("DELETE FROM runtime_health WHERE service='platform-operations-test'").run();
  await env.CONTROL_DB.prepare("DELETE FROM runtime_health WHERE service='jobs'").run();
  await env.H001_DB.prepare("DELETE FROM outbox_events WHERE id LIKE 'platform-operations-test-%'").run();
  await env.H001_DB.prepare("DELETE FROM command_receipts WHERE command_id LIKE 'platform-job-recovery-test-%'").run();
  await env.H001_DB.prepare("DELETE FROM audit_events WHERE command_id LIKE 'platform-job-recovery-test-%'").run();
  await env.CONTROL_DB.prepare("DELETE FROM platform_migration_runs WHERE idempotency_key LIKE 'platform-migration-test-%'").run();
  await env.CONTROL_DB.prepare("DELETE FROM holding_bindings WHERE holding_id='h003'").run();
  await env.CONTROL_DB.prepare("DELETE FROM holding_bindings WHERE holding_id='h004'").run();
});

async function event(suffix: string, createdAt: number) {
  await env.H001_DB
    .prepare(
      "INSERT INTO outbox_events(id,command_id,event_type,aggregate_type,aggregate_id,aggregate_version,payload_json,created_at,next_attempt_at) VALUES(?,?,?,?,?,?,?,?,?)",
    )
    .bind(
      `platform-operations-test-${suffix}`,
      `command-${suffix}`,
      "test.operation",
      "test",
      suffix,
      1,
      "{}",
      createdAt,
      createdAt,
    )
    .run();
}

it("summarizes real control and Holding queue state without mixing scopes", async () => {
  await env.CONTROL_DB
    .prepare("INSERT INTO runtime_health(service,heartbeat_at,details_json) VALUES(?,?,?)")
    .bind("platform-operations-test", now, '{"maintenance":false}')
    .run();
  await env.CONTROL_DB
    .prepare("INSERT INTO runtime_health(service,heartbeat_at,details_json) VALUES(?,?,?)")
    .bind("jobs", now - 500, "{}")
    .run();
  await event("pending", now - 20_000);
  await event("dead", now - 10_000);
  await env.H001_DB
    .prepare("UPDATE outbox_events SET dead_letter_at=? WHERE id=?")
    .bind(now - 1_000, "platform-operations-test-dead")
    .run();
  const summary = await summarizePlatformOperations(
    { role: "platform_admin" },
    env.CONTROL_DB,
    [{ id: "h001", slug: "acme", displayName: "Acme Demo", db: env.H001_DB }],
    now,
  );
  expect(summary.actorRole).toBe("platform_admin");
  expect(summary.controlHealth).toContainEqual({
    service: "platform-operations-test",
    heartbeatAt: now,
    details: '{"maintenance":false}',
  });
  expect(summary.holdings).toEqual([
    expect.objectContaining({
      id: "h001",
      status: "attention",
      pendingEvents: 1,
      deadLetterEvents: 1,
      claimedEvents: 0,
      oldestPendingAt: now - 20_000,
      jobHeartbeatAt: now - 500,
    }),
  ]);
  expect(summary.totals).toMatchObject({
    holdings: 1,
    pendingEvents: 1,
    deadLetterEvents: 1,
    claimedEvents: 0,
  });
});

it("contains an unavailable Holding as an actionable operational state", async () => {
  const summary = await summarizePlatformOperations(
    { role: "platform_auditor" },
    env.CONTROL_DB,
    [{ id: "missing", slug: "missing", displayName: "Missing", db: { prepare() { throw new Error("unavailable"); } } as never }],
    now,
  );
  expect(summary.holdings[0]).toMatchObject({
    id: "missing",
    status: "unavailable",
    error: "Holding database or operational tables are unavailable.",
  });
});

it("recovers a dead-letter job with a reviewed, idempotent command", async () => {
  const id = "platform-operations-test-recover";
  await event("recover", now - 2_000);
  await env.H001_DB.prepare("UPDATE outbox_events SET attempts=6,last_error=?,dead_letter_at=? WHERE id=?")
    .bind("PROJECTOR_TIMEOUT", now - 1_000, id).run();
  const input = { eventId: id, expectedAttempts: 6, expectedLastError: "PROJECTOR_TIMEOUT", reason: "Provider is healthy and the projection is safe to replay." };
  const actor = { userId: "platform-operator", assignmentId: "platform-assignment", role: "platform_admin" } as const;
  const first = await recoverDeadLetterJob(actor, env.H001_DB, "h001", input, "platform-job-recovery-test-1", now);
  expect(first).toMatchObject({ eventId: id, status: "queued", previousAttempts: 6 });
  expect(await env.H001_DB.prepare("SELECT attempts,last_error,dead_letter_at,next_attempt_at FROM outbox_events WHERE id=?").bind(id).first()).toEqual({ attempts: 0, last_error: null, dead_letter_at: null, next_attempt_at: now });
  expect(await env.H001_DB.prepare("SELECT action,purpose FROM audit_events WHERE command_id=?").bind("platform-job-recovery-test-1").first()).toEqual({ action: "platform.job.retry", purpose: input.reason });
  expect(await recoverDeadLetterJob(actor, env.H001_DB, "h001", input, "platform-job-recovery-test-1", now + 1)).toEqual(first);
});

it("does not allow auditors or stale failed-job snapshots to recover work", async () => {
  const id = "platform-operations-test-recover-guard";
  await event("recover-guard", now - 2_000);
  await env.H001_DB.prepare("UPDATE outbox_events SET attempts=6,last_error='OLD_ERROR',dead_letter_at=? WHERE id=?").bind(now - 1_000, id).run();
  const input = { eventId: id, expectedAttempts: 5, expectedLastError: "OLD_ERROR", reason: "Review the failed provider response before retrying." };
  await expect(recoverDeadLetterJob({ userId: "auditor", assignmentId: "platform-auditor", role: "platform_auditor" }, env.H001_DB, "h001", input, "platform-job-recovery-test-2", now)).rejects.toMatchObject({ code: "FORBIDDEN" });
  await expect(recoverDeadLetterJob({ userId: "admin", assignmentId: "platform-admin", role: "platform_admin" }, env.H001_DB, "h001", input, "platform-job-recovery-test-3", now)).rejects.toMatchObject({ code: "VERSION_CONFLICT" });
});

it("runs Holding migrations with a durable receipt and idempotent replay", async () => {
  const actor = { userId: "platform-operator", assignmentId: "platform-assignment", role: "platform_admin" } as const;
  const input = { holdingId: "h001", track: "holding-write" as const, reason: "Apply the reviewed release schema before enabling the next workflow." };
  const before = await summarizePlatformOperations(actor, env.CONTROL_DB, [{ id: "h001", slug: "acme", displayName: "Acme Demo", db: env.H001_DB }], now);
  expect(before.holdings[0]?.migrations.error).toBeNull();
  expect(before.holdings[0]?.migrations.pending).toEqual([]);
  const first = await runHoldingMigrations(actor, env.CONTROL_DB, env.H001_DB, input, "platform-migration-test-1", now);
  expect(first).toMatchObject({ holdingId: "h001", track: "holding-write", status: "succeeded", pending: [] });
  expect(first.appliedAfter).toBeGreaterThanOrEqual(first.appliedBefore);
  expect(await env.CONTROL_DB.prepare("SELECT status,holding_id,reason,applied_before,applied_after FROM platform_migration_runs WHERE idempotency_key=?").bind("platform-migration-test-1").first()).toEqual({
    status: "succeeded",
    holding_id: "h001",
    reason: input.reason,
    applied_before: first.appliedBefore,
    applied_after: first.appliedAfter,
  });
  expect(await runHoldingMigrations(actor, env.CONTROL_DB, env.H001_DB, input, "platform-migration-test-1", now + 1)).toEqual(first);
  await expect(runHoldingMigrations({ userId: "platform-auditor", assignmentId: "platform-auditor", role: "platform_auditor" }, env.CONTROL_DB, env.H001_DB, input, "platform-migration-test-2", now)).rejects.toMatchObject({ code: "FORBIDDEN" });
  await expect(runHoldingMigrations(actor, env.CONTROL_DB, env.H001_DB, { ...input, reason: "A different request using the same key." }, "platform-migration-test-1", now + 2)).rejects.toMatchObject({ code: "IDEMPOTENCY_CONFLICT" });
});

it("provisions a healthy resource slot with a durable activation history", async () => {
  const actor = { userId: "platform-operator", assignmentId: "platform-assignment", role: "platform_admin" } as const;
  const slot = { id: "h003", write: "H003_DB", read: "H003_READ_DB", files: "H003_FILES" } as const;
  const before = await platformProvisioningSnapshot(env.CONTROL_DB, [slot]);
  expect(before.availableSlots).toEqual([slot]);
  const input = {
    holdingId: slot.id,
    slug: "new-company",
    displayName: "New Company",
    writeBinding: slot.write,
    readBinding: slot.read,
    filesBinding: slot.files,
    reason: "Reviewed onboarding request and verified the isolated resource health.",
  };
  const first = await provisionHolding(actor, env.CONTROL_DB, slot, input, "platform-provisioning-test-1", now);
  expect(first).toMatchObject({ holdingId: "h003", slug: "new-company", status: "active" });
  expect(await env.CONTROL_DB.prepare("SELECT status,slug,write_binding FROM holding_bindings WHERE holding_id='h003'").first()).toEqual({ status: "active", slug: "new-company", write_binding: "H003_DB" });
  expect(await env.CONTROL_DB.prepare("SELECT action FROM platform_holding_provisioning_events WHERE provisioning_id=? ORDER BY created_at,id").bind(first.id).all()).toMatchObject({ results: [{ action: "started" }, { action: "validated" }, { action: "activated" }] });
  expect(await provisionHolding(actor, env.CONTROL_DB, slot, input, "platform-provisioning-test-1", now + 1)).toEqual(first);
  expect((await platformProvisioningSnapshot(env.CONTROL_DB, [slot])).availableSlots).toEqual([]);
});

it("does not allow auditors or a different slug to activate a slot", async () => {
  const slot = { id: "h004", write: "H004_DB", read: "H004_READ_DB", files: "H004_FILES" } as const;
  const input = { holdingId: slot.id, slug: "restricted-company", displayName: "Restricted Company", writeBinding: slot.write, readBinding: slot.read, filesBinding: slot.files, reason: "A sufficiently detailed reason for this controlled activation." };
  await expect(provisionHolding({ userId: "auditor", assignmentId: "platform-auditor", role: "platform_auditor" }, env.CONTROL_DB, slot, input, "platform-provisioning-test-2", now)).rejects.toMatchObject({ code: "FORBIDDEN" });
  const actor = { userId: "platform-operator", assignmentId: "platform-assignment", role: "platform_admin" } as const;
  await provisionHolding(actor, env.CONTROL_DB, slot, input, "platform-provisioning-test-3", now);
  await expect(provisionHolding(actor, env.CONTROL_DB, slot, { ...input, slug: "different-company" }, "platform-provisioning-test-3", now + 1)).rejects.toMatchObject({ code: "IDEMPOTENCY_CONFLICT" });
});
