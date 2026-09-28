import { beforeAll, expect, it } from "vitest";
import { applyMigrations, env } from "./runtime-env";
import { billingCommand, type BillingActor } from "../src/server/billing";
import { readTargetBillingPeriod } from "../src/server/billing-target";
import { readTargetRolloutReadiness } from "../src/server/billing-target-rollout";
import { monthBounds } from "../src/lib/billing";

const db = env.CONTROL_DB;
const month = "2026-12";
const bounds = monthBounds(month);
const now = bounds.end + 1000;
const key = (char: string) => char.repeat(64);
const admin: BillingActor = { userId: "target-admin", email: "admin@example.test", platform: true };

beforeAll(() => applyMigrations(db, env.CONTROL_TEST_MIGRATIONS));

async function fixture(assigned = true) {
  const holdingId = `target_${crypto.randomUUID()}`;
  await db.pool.query(
    "INSERT INTO holding_bindings(holding_id,slug,display_name,write_binding,read_binding,files_binding,status) VALUES($1,$1,'Target test',$2,$3,$4,'active')",
    [holdingId, `W${holdingId}`, `R${holdingId}`, `F${holdingId}`],
  );
  await db.pool.query(
    "INSERT INTO billing_accounts(holding_id,customer_id,created_at) VALUES($1,$2,$3)",
    [holdingId, `cus_${crypto.randomUUID()}`, bounds.start],
  );
  await db.pool.query(
    "INSERT INTO billing_target_sandbox_holdings VALUES($1,'test','Explicit test designation',$2)",
    [holdingId, bounds.start],
  );
  if (assigned) await db.pool.query(
    `INSERT INTO billing_target_assignments
      (holding_id,effective_month,price_id,mode,service_started_at,assigned_by,reason,created_at)
     VALUES($1,$2,'holding-usage-2026-12-v3','sandbox_invoice',$3,'test','Test only',$3)`,
    [holdingId, month, bounds.start],
  );
  await db.pool.query(
    `INSERT INTO billing_usage_snapshots
      (holding_id,month,snapshot_id,refreshed_at,source_count,ai_coverage,storage_coverage,
       unreconciled_originals,seat_coverage,seat_source_count)
     VALUES($1,$2,'snapshot-1',$3,5,'measured','measured',0,'measured',10)`,
    [holdingId, month, now],
  );
  for (let index = 0; index < 10; index++)
    await db.pool.query(
      `INSERT INTO billing_seat_usage(holding_id,month,user_key,intervals_json,snapshot_id)
       VALUES($1,$2,$3,$4,'snapshot-1')`,
      [holdingId, month, index.toString(16).padStart(64, "0"),
        JSON.stringify([[bounds.start, bounds.end]])],
    );
  const common = [holdingId, month, "company", "Company", "branch", "Branch", "snapshot-1"];
  await db.pool.query(
    `INSERT INTO billing_usage_meters
      (holding_id,month,source_key,company_id,company_name,branch_id,branch_name,
       meter_kind,funding_source,pricing_status,quantity,input_tokens,output_tokens,
       average_daily_bytes,active_at_period_end,snapshot_id,active_from,active_until,byte_milliseconds)
     VALUES($1,$2,$3,$4,$5,$6,$7,'storage','unknown','unpriced',28000000000,0,0,
       28000000000,TRUE,$8,$9,$10,$11)`,
    [common[0], common[1], key("a"), common[2], common[3], common[4], common[5],
      common[6], bounds.start, bounds.end,
      (BigInt(28_000_000_000) * BigInt(bounds.end - bounds.start)).toString()],
  );
  for (const [source, funding, status, cost] of [
    ["b", "erpx", "succeeded", "reconciled"],
    ["c", "byok", "succeeded", "not_applicable"],
    ["d", "erpx", "failed", "not_applicable"],
    ["e", "simulation", "succeeded", "not_applicable"],
  ] as const) {
    await db.pool.query(
      `INSERT INTO billing_usage_meters
        (holding_id,month,source_key,company_id,company_name,branch_id,branch_name,
         meter_kind,funding_source,pricing_status,quantity,input_tokens,output_tokens,
         average_daily_bytes,active_at_period_end,snapshot_id,request_status,provider,model,
         gateway_status,cost_status,verified_cost_satang,cost_reconciled_at)
       VALUES($1,$2,$3,$4,$5,$6,$7,'ai',$8,'unpriced',30,20,10,0,FALSE,$9,$10,
         'openai','test-model',$14,$11,$12,$13)`,
      [common[0], common[1], key(source), common[2], common[3], common[4], common[5],
        funding, common[6], status, cost, source === "b" ? 7500 : null,
        source === "b" ? now : null, source === "d" ? "succeeded" : status],
    );
  }
  return holdingId;
}

it("shows measured provider cost, storage and seat evidence without assigning a target offering", async () => {
  const holdingId = await fixture(false);
  const currentMonth = "2027-01";
  await db.pool.query(
    `INSERT INTO billing_usage_snapshots
      (holding_id,month,snapshot_id,refreshed_at,active_seat_count,active_seat_observed_at)
     VALUES($1,$2,'current-seat-snapshot',$3,8,$3)`,
    [holdingId, currentMonth, now],
  );
  const report = await readTargetBillingPeriod(db, holdingId, month, now);
  expect(report.state).toBe("unassigned");
  expect(report.totalSatang).toBeNull();
  expect(report.invoice).toBeNull();
  expect(report.measuredEvidence.snapshot?.sourceCount).toBe(5);
  expect(report.measuredEvidence.ai.requests).toBe(4);
  expect(report.measuredEvidence.ai.platformVerified).toBe(1);
  expect(report.measuredEvidence.ai.verifiedProviderCostSatang).toBe(7500);
  expect(report.measuredEvidence.ai.chargeableVerifiedProviderCostSatang).toBe(7500);
  expect(report.measuredEvidence.ai.totalProviderCostSatang).toBeNull();
  expect(report.measuredEvidence.ai.sources.find(source => source.sourceKey === key("b"))?.billableStatus)
    .toBe("verified_cost");
  expect(report.measuredEvidence.storage.originalCount).toBe(1);
  expect(report.measuredEvidence.storage.activeBytesAtEnd).toBe(28_000_000_000);
  expect(report.measuredEvidence.storage.byteMilliseconds)
    .toBe((BigInt(28_000_000_000) * BigInt(bounds.end - bounds.start)).toString());
  expect(report.measuredEvidence.seats.distinctUsers).toBe(10);
  expect(report.measuredEvidence.seats.activeSeatMilliseconds)
    .toBe((BigInt(10) * BigInt(bounds.end - bounds.start)).toString());
  expect(report.measuredEvidence.seats.activeNow).toEqual({status: "measured", count: 8, asOf: now});
  expect(report.measuredEvidence.operatingCosts.categories.backup.status).toBe("unavailable");
  expect(report.measuredEvidence.operatingCosts.totalVerifiedSatang).toBeNull();
  await db.pool.query(
    `UPDATE billing_usage_meters SET cost_status='reconciled',verified_cost_satang=1000,
       cost_reconciled_at=$4 WHERE holding_id=$1 AND month=$2 AND source_key=$3`,
    [holdingId, month, key("d"), now],
  );
  const withFailedProviderExpense = await readTargetBillingPeriod(db, holdingId, month, now);
  expect(withFailedProviderExpense.measuredEvidence.ai.totalProviderCostSatang).toBe(8500);
  expect(withFailedProviderExpense.measuredEvidence.ai.chargeableVerifiedProviderCostSatang).toBe(7500);
  expect(withFailedProviderExpense.measuredEvidence.ai.sources.find(source => source.sourceKey === key("d"))?.billableStatus)
    .toBe("excluded_failed");
});

it("does not turn incomplete Platform cost into a zero total", async () => {
  const holdingId = await fixture(false);
  await db.pool.query(
    `UPDATE billing_usage_meters SET cost_status='unavailable',verified_cost_satang=NULL,
       cost_reconciled_at=NULL WHERE holding_id=$1 AND month=$2 AND source_key=$3`,
    [holdingId, month, key("b")],
  );
  const report = await readTargetBillingPeriod(db, holdingId, month, now);
  expect(report.measuredEvidence.ai.platformPending).toBe(1);
  expect(report.measuredEvidence.ai.verifiedProviderCostSatang).toBe(0);
  expect(report.measuredEvidence.ai.totalProviderCostSatang).toBeNull();
  expect(report.measuredEvidence.ai.sources.find(source => source.sourceKey === key("b"))?.billableStatus)
    .toBe("price_unavailable");
  expect(report.totalSatang).toBeNull();
});

it("treats a failed gateway with an explicit non-billable cost state as no provider expense", async () => {
  const holdingId = await fixture(false);
  await db.pool.query(
    `UPDATE billing_usage_meters SET gateway_status='failed'
     WHERE holding_id=$1 AND month=$2 AND source_key=$3`,
    [holdingId, month, key("d")],
  );
  const report = await readTargetBillingPeriod(db, holdingId, month, now);
  expect(report.measuredEvidence.ai.totalProviderCostSatang).toBe(7500);
  expect(report.measuredEvidence.ai.sources.find(source => source.sourceKey === key("d"))?.billableStatus)
    .toBe("excluded_failed");
});

it("holds the entire target invoice until each Platform AI cost and statement is verified", async () => {
  const holdingId = await fixture();
  const report = await readTargetBillingPeriod(db, holdingId, month, now);
  expect(report.completeness.ready).toBe(false);
  expect(report.totalSatang).toBeNull();
  expect(report.approval.eligible).toBe(false);
  expect(report.completeness.missing).toContain(`provider_cost_missing:${key("b")}`);
  expect(report.sources.find(line => line.sourceKey === key("c"))?.amountSatang).toBe(0);
  expect(report.sources.find(line => line.sourceKey === key("d"))?.amountSatang).toBe(0);
  expect(report.sources.find(line => line.sourceKey === key("e"))?.amountSatang).toBe(0);
});

it("rejects a backdated Sandbox invoice assignment even for a designated test Holding", async () => {
  const holdingId = await fixture();
  await expect(billingCommand(db, admin, holdingId, {
    action: "assign-target", version: 0, reason: "Try to backdate Sandbox charges",
    targetMonth: month, targetMode: "sandbox_invoice",
  }, crypto.randomUUID(), async () => { throw new Error("Unexpected Stripe request"); },
  "https://example.test/", now)).rejects.toMatchObject({ code: "TARGET_BACKDATE_FORBIDDEN" });
});

it("keeps existing-customer rollout blocked when two shadow cycles, costs and capacity are unavailable", async () => {
  const holdingId = await fixture();
  const review = await readTargetRolloutReadiness(db, holdingId, "2027-03", 2000,
    Date.parse("2027-03-01T00:00:00+07:00"));
  expect(review.ready).toBe(false);
  expect(review.margins.every(value => value.grossMarginBps === null)).toBe(true);
  expect(review.blockers).toContain("capacity_review_missing");
  expect(review.blockers.some(value => value.startsWith("complete_shadow_cycle_missing"))).toBe(true);
  expect(review.blockers.some(value => value.startsWith("operating_costs_missing"))).toBe(true);
});

it("rates 1,490 + 8 GB-month + verified AI at 10%, and freezes the approved Sandbox invoice", async () => {
  const holdingId = await fixture();
  const evidence = `evidence_${crypto.randomUUID()}`;
  await db.pool.query(
    `INSERT INTO billing_evidence
      (id,holding_id,filename,mime,size,sha256,actor,created_at,object_key,
       detected_mime,source_state,scanned_at,scanner_definition_at)
     VALUES($1,$2,'statement.pdf','application/pdf',100,$3,'test',$4,$5,
       'application/pdf','available',$4,$4)`,
    [evidence, holdingId, key("f"), now, `${holdingId}/${evidence}`],
  );
  await db.pool.query(
    `INSERT INTO billing_target_provider_costs
      (holding_id,month,source_key,provider,statement_id,original_currency,original_minor,
       fx_numerator,fx_denominator,provider_cost_satang,evidence_id,verified_by,verified_at)
     VALUES($1,$2,$3,'openai','statement-oct','THB',7500,1,1,7500,$4,'test',$5)`,
    [holdingId, month, key("b"), evidence, now],
  );
  await db.pool.query(
    `INSERT INTO billing_target_statements
      (holding_id,month,provider,statement_id,evidence_id,provider_total_minor,
       original_currency,reconciled_total_minor,verified_by,verified_at)
     VALUES($1,$2,'openai','statement-oct',$3,7500,'THB',7500,'test',$4)`,
    [holdingId, month, evidence, now],
  );
  await billingCommand(db, admin, holdingId, {
    action: "approve-target-extra", version: 0,
    reason: "Separate approved setup quote", targetMonth: month,
    extraKind: "setup", amountSatang: 10_000,
    serviceStart: bounds.start, serviceEnd: bounds.end,
    note: "Initial setup", evidenceId: evidence,
  }, crypto.randomUUID(), async () => { throw new Error("Unexpected Stripe request"); },
  "https://example.test/", now);
  let report = await readTargetBillingPeriod(db, holdingId, month, now);
  expect(report.approval.eligible).toBe(true);
  expect(report.totalSatang).toBe(521_250);
  expect(report.lines.find(line => line.kind === "seat" && line.sourceKey === null)?.amountSatang).toBe(499_000);
  expect(report.lines.find(line => line.kind === "extra")?.amountSatang).toBe(10_000);
  await billingCommand(db, admin, holdingId, {
    action: "approve-target", version: 1, reason: "Verified statement and complete meters",
    targetMonth: month,
  }, crypto.randomUUID(), async () => { throw new Error("Stripe should not be called before job drain"); },
  "https://example.test/", now);
  report = await readTargetBillingPeriod(db, holdingId, month, now + 1);
  expect(report.state).toBe("approved");
  expect(report.totalSatang).toBe(521_250);
  expect(report.invoice?.totalSatang).toBe(521_250);
  expect(report.invoice?.status).toBe("queued");
  await db.pool.query(
    "UPDATE billing_usage_meters SET quantity=0 WHERE holding_id=$1 AND month=$2 AND source_key=$3",
    [holdingId, month, key("a")],
  );
  expect((await readTargetBillingPeriod(db, holdingId, month, now + 2)).totalSatang).toBe(521_250);
});
