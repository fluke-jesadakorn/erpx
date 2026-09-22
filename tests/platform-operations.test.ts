import { beforeAll, beforeEach, expect, it } from "vitest";
import { applyMigrations, env } from "./runtime-env";
import { recoverDeadLetterJob, summarizePlatformOperations } from "../src/server/platform-operations";

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
