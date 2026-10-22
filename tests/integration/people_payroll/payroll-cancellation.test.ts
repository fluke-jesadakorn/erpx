import { afterEach, beforeAll, beforeEach, expect, it, vi } from "vitest";
import { applyMigrations, env } from "../../support/runtime-env";
import { expenseDb as db, expenseFixture, key } from "../../support/helpers/expense-fixture";
import {
  approvePayrollRun, calculatePayrollRun, createPayrollRun, exportPayrollReview, getPayrollReviewFile,
  payrollRunSchema, payrollPostSchema, postPayroll, savePayrollAccountMapping,
  payrollAccountMappingSchema, approvePayrollAccountMapping,
} from "@erpx/server-people_payroll/public/people-payroll";
import { cancelPayrollRun, payrollCancelSchema } from "@erpx/server-people_payroll/public/payroll-cancellation";
import { payrollReadiness, submitPayrollReview } from "@erpx/server-people_payroll/public/payroll-readiness";

vi.mock("@erpx/server-runtime/adapters/runtime", () => ({ holdingResources: async () => ({ write: env.H001_DB, files: env.H001_FILES }), runtimeEnv: async () => env }));

let f: Awaited<ReturnType<typeof expenseFixture>>;
let group: string;
const reason = "Future work period entered accidentally; retain the original draft evidence.";
beforeAll(async () => { await applyMigrations(db, env.STAFF_TEST_MIGRATIONS); });
beforeEach(async () => {
  f = await expenseFixture();
  f.owner.role = "payroll_preparer"; f.owner.assignmentId = await f.grant(f.owner, "payroll_preparer");
  f.manager.role = "payroll_approver"; f.manager.assignmentId = await f.grant(f.manager, "payroll_approver");
  group = `cancellation-group-${key()}`;
  const engagement = await db.prepare("SELECT e.id FROM hr_engagements e JOIN hr_people p ON p.id=e.person_id WHERE e.company_id=? AND p.user_id=?")
    .bind(f.company, f.claimant.userId).first<string>("id");
  // Declared fictional approved source; all cancellation, review and posting actions are native commands.
  await db.batch([
    db.prepare("INSERT INTO hr_pay_groups(id,company_id,name,country_code,currency,frequency,timezone,created_by,created_at,updated_at) VALUES(?,?,'Cancellation proof','TH','THB','monthly','Asia/Bangkok',?,1,1)").bind(group, f.company, f.owner.userId),
    db.prepare("UPDATE hr_engagements SET pay_group_id=?,version=version+1 WHERE id=?").bind(group, engagement),
    db.prepare("INSERT INTO hr_compensation_revisions(id,engagement_id,effective_from,pay_frequency,base_amount_minor,hourly_rate_minor,expected_hours,currency,components_json,status,approved_by,created_by,created_at,updated_at) VALUES(?,?,'2026-09-01','monthly',100000,1000,160,'THB','[]','approved',?,?,1,1)").bind(`cancellation-comp-${key()}`, engagement, f.manager.userId, f.reviewer.userId),
    ...[["calculate", f.owner.userId], ["preview", f.owner.userId], ["in_review", f.manager.userId], ["approved", f.reviewer.userId]].map(([step, person]) =>
      db.prepare("INSERT INTO flow_defaults(company_id,kind,step_key,primary_user_id,created_by,reason,created_at) VALUES(?,'payroll',?,?,?,'Cancellation fixture responsibility',1)").bind(f.company, step, person, f.owner.userId)),
  ]);
});
afterEach(() => vi.useRealTimers());

async function create(periodStart = "2026-10-01", periodEnd = "2026-10-31", payDate = "2026-11-01") {
  return createPayrollRun(f.owner, payrollRunSchema.parse({ payGroupId: group, periodStart, periodEnd, payDate, currency: "THB" }), key());
}
async function source(runId: string) { return db.prepare("SELECT to_jsonb(r)::text snapshot FROM hr_payroll_runs r WHERE id=?").bind(runId).first<string>("snapshot"); }
async function cancel(runId: string, expectedVersion = 1, command = key()) {
  return cancelPayrollRun(f.owner, runId, payrollCancelSchema.parse({ expectedVersion, reason }), command);
}
async function counts(runId: string) {
  return db.prepare(`SELECT (SELECT COUNT(*) FROM hr_payroll_run_cancellations WHERE run_id=?) receipts,
    (SELECT COUNT(*) FROM audit_events WHERE record_id=? AND action='people.payroll.cancel') audits,
    (SELECT COUNT(*) FROM outbox_events WHERE aggregate_id=? AND event_type='people.payroll.cancel') outbox,
    (SELECT COUNT(*) FROM flow_events WHERE record_id=? AND kind='payroll' AND action='cancel') flow`).bind(runId, runId, runId, runId).first();
}

it("cancels a future draft with retained owner evidence and permits exact-period and completed non-overlap retries", async () => {
  vi.useFakeTimers({ toFake: ["Date"] }); vi.setSystemTime(new Date("2026-10-03T00:00:00Z"));
  const created = await create(), runId = String(created.id), original = await source(runId);
  expect((await payrollReadiness(f.owner, runId)).blockers).toEqual(expect.arrayContaining([expect.objectContaining({ code: "PERIOD_OPEN" })]));
  const result = await cancel(runId);
  expect(result).toMatchObject({ status: "cancelled", version: 2 });
  expect(await db.prepare("SELECT source_snapshot_json,prior_status,prior_version,run_revision,reason,cancelled_by FROM hr_payroll_run_cancellations WHERE run_id=?").bind(runId).first()).toEqual({ source_snapshot_json: original, prior_status: "calculating", prior_version: 1, run_revision: 1, reason, cancelled_by: f.owner.userId });
  expect(await counts(runId)).toEqual({ receipts: 1, audits: 1, outbox: 1, flow: 1 });
  const retained = JSON.parse((await source(runId))!); const prior = JSON.parse(original!);
  expect({ ...retained, status: prior.status, version: prior.version, updated_at: prior.updated_at }).toEqual(prior);
  const samePeriod = await create(); expect(samePeriod).toMatchObject({ status: "calculating", version: 1 });
  await cancel(String(samePeriod.id));
  const completed = await create("2026-10-01", "2026-10-02", "2026-10-03");
  expect((await payrollReadiness(f.owner, String(completed.id))).blockers.some(blocker => blocker.code === "PERIOD_OPEN")).toBe(false);
  expect(await calculatePayrollRun(f.owner, String(completed.id), key())).toMatchObject({ status: "preview", totals: { gross: 100000, net: 100000, headcount: 1 } });
  expect(JSON.parse((await source(runId))!)).toEqual(retained);
  await expect(db.prepare("UPDATE hr_payroll_runs SET status='calculating',version=version+1 WHERE id=?").bind(runId).run()).rejects.toMatchObject({ code: "23514" });
  await expect(db.prepare("DELETE FROM hr_payroll_run_cancellations WHERE run_id=?").bind(runId).run()).rejects.toMatchObject({ code: "23514" });
});

it("replays a lost cancellation response once and rejects stale or conflicting keys without extra evidence", async () => {
  const created = await create(), runId = String(created.id), command = key();
  await expect(cancel(runId, 2)).rejects.toMatchObject({ code: "PAYROLL_RUN_CHANGED" });
  expect(await counts(runId)).toEqual({ receipts: 0, audits: 0, outbox: 0, flow: 0 });
  const result = await cancel(runId, 1, command);
  expect(await cancel(runId, 1, command)).toEqual(result);
  await expect(cancelPayrollRun(f.owner, runId, { expectedVersion: 1, reason: reason + " changed" }, command)).rejects.toMatchObject({ code: "IDEMPOTENCY_CONFLICT" });
  await expect(cancel(runId, 1)).rejects.toMatchObject({ code: "PAYROLL_RUN_CHANGED" });
  await expect(cancel(runId, 2)).rejects.toMatchObject({ code: "PAYROLL_CANCELLATION_UNAVAILABLE" });
  expect(await counts(runId)).toEqual({ receipts: 1, audits: 1, outbox: 1, flow: 1 });
});

it("serializes concurrent cancellation commands and an identical lost-response retry into one owner transition", async () => {
  const runId = String((await create()).id);
  const results = await Promise.allSettled([cancel(runId), cancel(runId)]);
  expect(results.filter(result => result.status === "fulfilled")).toHaveLength(1);
  const rejected = results.find(result => result.status === "rejected") as PromiseRejectedResult;
  expect(rejected.reason).toMatchObject({ status: 409 });
  expect(await counts(runId)).toEqual({ receipts: 1, audits: 1, outbox: 1, flow: 1 });
  const second = String((await create()).id), command = key();
  const replays = await Promise.all([cancel(second, 1, command), cancel(second, 1, command)]);
  expect(replays[0]).toEqual(replays[1]);
  expect(await counts(second)).toEqual({ receipts: 1, audits: 1, outbox: 1, flow: 1 });
});

it("retains calculated item snapshots and issued review bytes after preview cancellation", async () => {
  const runId = String((await create("2026-09-01", "2026-09-30", "2026-10-01")).id);
  const calculated = await calculatePayrollRun(f.owner, runId, key());
  const review = await exportPayrollReview(f.owner, runId, key());
  const original = await (await getPayrollReviewFile(f.owner, runId, Number(review.revision))).object.text();
  const items = (await db.prepare("SELECT * FROM hr_payroll_items WHERE run_id=? ORDER BY id").bind(runId).all()).results;
  await cancel(runId, Number(calculated.version));
  expect((await db.prepare("SELECT * FROM hr_payroll_items WHERE run_id=? ORDER BY id").bind(runId).all()).results).toEqual(items);
  const retained = await getPayrollReviewFile(f.owner, runId, Number(review.revision));
  expect(await retained.object.text()).toEqual(original); expect(retained.checksum).toEqual(review.checksum);
  await expect(db.prepare("UPDATE hr_payroll_items SET gross_minor=gross_minor+1 WHERE run_id=?").bind(runId).run()).rejects.toMatchObject({ code: "23514" });
  await expect(db.prepare("DELETE FROM hr_payroll_items WHERE run_id=?").bind(runId).run()).rejects.toMatchObject({ code: "23514" });
  await expect(calculatePayrollRun(f.owner, runId, key())).rejects.toMatchObject({ code: "PAYROLL_LOCKED" });
  expect(await db.prepare("SELECT COUNT(*) n FROM hr_payment_batches WHERE source_type='payroll' AND source_id=?").bind(runId).first("n")).toBe(0);
});

it("guards a concurrent preview calculation against cancellation without orphaned calculation items", async () => {
  const runId = String((await create("2026-09-01", "2026-09-30", "2026-10-01")).id);
  const results = await Promise.allSettled([cancel(runId), calculatePayrollRun(f.owner, runId, key())]);
  expect(results.filter(result => result.status === "fulfilled")).toHaveLength(1);
  const current = await db.prepare("SELECT status,version FROM hr_payroll_runs WHERE id=?").bind(runId).first<{ status: string; version: number }>();
  expect(current!.version).toBe(2);
  const itemCount = await db.prepare("SELECT COUNT(*) n FROM hr_payroll_items WHERE run_id=?").bind(runId).first("n");
  if (current!.status === "cancelled") { expect(itemCount).toBe(0); expect((await counts(runId))!.receipts).toBe(1); }
  else { expect(current!.status).toBe("preview"); expect(itemCount).toBe(1); expect((await counts(runId))!.receipts).toBe(0); await cancel(runId, 2); }
});

it("requires current creator or named Company authority and denies cached revocation, expiry and cross-Company cancellation", async () => {
  const runId = String((await create()).id);
  f.reviewer.role = "payroll_preparer"; f.reviewer.assignmentId = await f.grant(f.reviewer, "payroll_preparer");
  await expect(cancelPayrollRun(f.reviewer, runId, { expectedVersion: 1, reason }, key())).rejects.toMatchObject({ code: "PAYROLL_CANCELLATION_OWNER_REQUIRED" });
  const other = await expenseFixture(); other.owner.role = "payroll_preparer"; other.owner.assignmentId = await other.grant(other.owner, "payroll_preparer");
  await expect(cancelPayrollRun(other.owner, runId, { expectedVersion: 1, reason }, key())).rejects.toMatchObject({ code: "PAYROLL_RUN_NOT_FOUND" });
  await db.prepare("UPDATE assignments SET valid_until=1 WHERE id=?").bind(f.owner.assignmentId).run();
  await expect(cancel(runId)).rejects.toMatchObject({ code: "PAYROLL_CANCELLATION_FORBIDDEN" });
  await db.prepare("UPDATE assignments SET valid_until=9999999999999 WHERE id=?").bind(f.owner.assignmentId).run();
  const command = key(); await cancel(runId, 1, command);
  await db.prepare("UPDATE assignments SET revoked_at=? WHERE id=?").bind(Date.now(), f.owner.assignmentId).run();
  await expect(cancel(runId, 1, command)).rejects.toMatchObject({ code: "PAYROLL_CANCELLATION_FORBIDDEN" });
  expect(await counts(runId)).toEqual({ receipts: 1, audits: 1, outbox: 1, flow: 1 });
});

it("allows the normally named preparer to cancel a scheduled draft without taking over its original creator", async () => {
  const runId = String((await create()).id);
  await db.prepare("UPDATE hr_payroll_runs SET created_by='system:payroll-schedule' WHERE id=?").bind(runId).run();
  await cancel(runId);
  expect(await db.prepare("SELECT created_by FROM hr_payroll_runs WHERE id=?").bind(runId).first("created_by")).toBe("system:payroll-schedule");
  expect(JSON.parse(String(await db.prepare("SELECT source_snapshot_json FROM hr_payroll_run_cancellations WHERE run_id=?").bind(runId).first("source_snapshot_json"))).created_by).toBe("system:payroll-schedule");
});

it("rejects submitted, approved and posted payroll cancellation while retaining its exact GL and immutable employee payslip", async () => {
  vi.useFakeTimers({ toFake: ["Date"] }); vi.setSystemTime(new Date("2026-10-03T00:00:00Z"));
  const runId = String((await create("2026-09-01", "2026-09-30", "2026-10-01")).id);
  const calculated = await calculatePayrollRun(f.owner, runId, key());
  const digest = await db.prepare("SELECT input_digest FROM hr_payroll_runs WHERE id=?").bind(runId).first<string>("input_digest");
  const submitted = await submitPayrollReview(f.owner, runId, { expectedVersion: Number(calculated.version), inputDigest: digest! }, key());
  await expect(cancel(runId, Number(submitted.version))).rejects.toMatchObject({ code: "PAYROLL_CANCELLATION_UNAVAILABLE" });
  const approved = await approvePayrollRun(f.manager, runId, Number(submitted.version), key());
  await expect(cancel(runId, Number(approved.version))).rejects.toMatchObject({ code: "PAYROLL_CANCELLATION_UNAVAILABLE" });
  f.reviewer.role = "finance_approver"; f.reviewer.assignmentId = await f.grant(f.reviewer, "finance_approver");
  f.payer.role = "finance_approver"; f.payer.assignmentId = await f.grant(f.payer, "finance_approver");
  const mapping = await savePayrollAccountMapping(f.payer, group, payrollAccountMappingSchema.parse({ chartId: f.chart, expenseAccountId: f.accounts.expense, netPayableAccountId: f.accounts.payable }), key());
  await approvePayrollAccountMapping(f.reviewer, String(mapping.id), 1, key());
  const posted = await postPayroll(f.reviewer, runId, payrollPostSchema.parse({ expectedVersion: approved.version, accountMappingId: mapping.id, periodId: f.period, entryDate: "2026-09-30" }), key());
  const original = await source(runId);
  const slips = (await db.prepare("SELECT * FROM hr_issued_payslips WHERE run_id=?").bind(runId).all()).results;
  expect(slips).toHaveLength(1);
  expect(await db.prepare("SELECT SUM(debit_minor) debit,SUM(credit_minor) credit FROM finance_journal_lines WHERE journal_id=?").bind(posted.journalId).first()).toEqual({ debit: 100000, credit: 100000 });
  await expect(cancel(runId, Number(posted.version))).rejects.toMatchObject({ code: "PAYROLL_CANCELLATION_UNAVAILABLE" });
  await expect(db.prepare("UPDATE hr_payroll_runs SET status='cancelled',version=version+1 WHERE id=?").bind(runId).run()).rejects.toMatchObject({ code: "23514" });
  expect(await source(runId)).toEqual(original);
  expect((await db.prepare("SELECT * FROM hr_issued_payslips WHERE run_id=?").bind(runId).all()).results).toEqual(slips);
  expect(await counts(runId)).toEqual({ receipts: 0, audits: 0, outbox: 0, flow: 0 });
});
