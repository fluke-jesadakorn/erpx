import "server-only";
import { createHash, randomUUID } from "node:crypto";
import type { PoolClient } from "pg";
import { z } from "zod";
import { ApiError } from "./http";
import type { PostgresDatabase } from "./runtime/postgres";
import {
  billingMonth,
  monthBounds,
  proratedSupport,
  type BillingView,
  type InvoiceLine,
} from "@/lib/billing";
import {
  stripeList,
  stripeUrl,
  testObject,
  type BillingStripe,
} from "./billing-stripe";
import { calculateTargetBillingPeriod, type TargetBillingPeriodView } from "./billing-target";
import { monthBounds as targetMonthBounds } from "@/lib/billing";
import { roundSatang } from "@/lib/billing-target-pricing";
import { calculateTargetRolloutReadiness } from "./billing-target-rollout";
export type BillingActor = {
  userId: string;
  platform: boolean;
  holdingId?: string;
  email: string;
};
function fail(code: string, message: string): never {
  throw new ApiError(409, code, message);
}
const amount = z.number().int().min(0).max(100000000);
export const billingInput = z
  .object({
    action: z.enum([
      "enroll",
      "change-plan",
      "cancel-hosted",
      "withdraw",
      "payment-mode",
      "portal",
      "setup",
      "cost",
      "delete-cost",
      "complete-costs",
      "approve",
      "void",
      "revise-plan",
      "retry",
      "resolve-legacy",
      "assign-target",
      "verify-provider-cost",
      "verify-statement",
      "approve-target",
      "rate-target",
      "record-operating-cost",
      "review-target-capacity",
      "approve-target-rollout",
      "approve-target-extra",
    ]),
    version: z.number().int().nonnegative(),
    reason: z.string().trim().min(3).max(1000),
    planId: z.enum(["infrastructure", "standard", "priority"]).optional(),
    supportSatang: amount.optional(),
    planRevision: z.number().int().positive().optional(),
    paymentMode: z.enum(["manual", "auto"]).optional(),
    consent: z.boolean().optional(),
    periodId: z.string().max(100).optional(),
    costId: z.string().max(100).optional(),
    invoiceId: z.string().max(100).optional(),
    jobId: z.string().max(100).optional(),
    category: z
      .enum(["hosting", "storage", "provider_ai", "adjustment"])
      .optional(),
    amountSatang: z.number().int().min(-100000000).max(100000000).optional(),
    serviceStart: z.number().int().optional(),
    serviceEnd: z.number().int().optional(),
    note: z.string().trim().max(2000).optional(),
    simulated: z.boolean().optional(),
    evidenceId: z.string().max(100).nullable().optional(),
    originalInvoiceId: z.string().max(100).optional(),
    complete: z.boolean().optional(),
    targetMonth: z.string().regex(/^\d{4}-(0[1-9]|1[0-2])$/).optional(),
    targetMode: z.enum(["shadow", "sandbox_invoice"]).optional(),
    priceId: z.string().max(100).optional(),
    sourceKey: z.string().regex(/^[a-f0-9]{64}$/).optional(),
    provider: z.string().trim().min(1).max(100).optional(),
    statementId: z.string().trim().min(1).max(200).optional(),
    originalCurrency: z.string().regex(/^[A-Z]{3}$/).optional(),
    originalMinor: z.number().int().min(0).max(Number.MAX_SAFE_INTEGER).optional(),
    providerTotalMinor: z.number().int().min(0).max(Number.MAX_SAFE_INTEGER).optional(),
    fxNumerator: z.number().int().positive().max(Number.MAX_SAFE_INTEGER).optional(),
    fxDenominator: z.number().int().positive().max(Number.MAX_SAFE_INTEGER).optional(),
    providerCostSatang: z.number().int().min(0).max(100000000).optional(),
    operatingCategory: z.enum(["infrastructure", "backup", "stripe", "support"]).optional(),
    operatingCostSatang: amount.optional(),
    projectedOriginalBytes: z.number().int().min(0).max(Number.MAX_SAFE_INTEGER).optional(),
    primaryAvailableBytes: z.number().int().min(0).max(Number.MAX_SAFE_INTEGER).optional(),
    backupAvailableBytes: z.number().int().min(0).max(Number.MAX_SAFE_INTEGER).optional(),
    reservedBytes: z.number().int().min(0).max(Number.MAX_SAFE_INTEGER).optional(),
    companyLimitBytes: z.number().int().min(0).max(Number.MAX_SAFE_INTEGER).optional(),
    minimumMarginBps: z.number().int().min(0).max(10000).optional(),
    extraKind: z.enum(["setup", "support"]).optional(),
  })
  .strict();
export type BillingInput = z.infer<typeof billingInput>;
const adminActions = new Set([
  "cost",
  "delete-cost",
  "complete-costs",
  "approve",
  "void",
  "revise-plan",
  "retry",
  "assign-target",
  "verify-provider-cost",
  "verify-statement",
  "approve-target",
  "rate-target",
  "record-operating-cost",
  "review-target-capacity",
  "approve-target-rollout",
  "approve-target-extra",
]);
async function storeTargetRating(
  c: PoolClient,
  target: TargetBillingPeriodView,
  actor: BillingActor,
  reason: string,
  now: number,
) {
  if (!target.assignment || !target.price)
    fail("TARGET_ASSIGNMENT_REQUIRED", "Assign a target price before rating this month.");
  const bounds = monthBounds(target.month);
  const state = target.completeness.ready ? "ready" : "held";
  const snapshotId = target.completeness.meterSnapshotId ?? "missing";
  await c.query(
    `INSERT INTO billing_target_rating_runs
     (id,holding_id,month,price_id,meter_snapshot_id,state,missing_json,
      lines_json,total_satang,actor,reason,created_at)
     VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12)`,
    [randomUUID(), target.holdingId, target.month, target.price.id, snapshotId,
      state, JSON.stringify(target.completeness.missing), JSON.stringify(target.lines),
      target.totalSatang, actor.userId, reason, now],
  );
  await c.query(
    `INSERT INTO billing_target_rated_periods
     (holding_id,month,price_id,assignment_month,mode,meter_snapshot_id,
      service_start,service_end,state,missing_json,lines_json,total_satang,rated_at)
     VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13)
     ON CONFLICT(holding_id,month) DO UPDATE SET
       price_id=excluded.price_id, assignment_month=excluded.assignment_month,
       mode=excluded.mode, meter_snapshot_id=excluded.meter_snapshot_id,
       service_start=excluded.service_start, service_end=excluded.service_end,
       state=excluded.state, missing_json=excluded.missing_json,
       lines_json=excluded.lines_json, total_satang=excluded.total_satang,
       rated_at=excluded.rated_at
     WHERE billing_target_rated_periods.state<>'approved'`,
    [target.holdingId, target.month, target.price.id, target.assignment.effectiveMonth,
      target.assignment.mode, snapshotId,
      Math.max(bounds.start, target.assignment.serviceStartedAt), bounds.end,
      state, JSON.stringify(target.completeness.missing), JSON.stringify(target.lines),
      target.totalSatang, now],
  );
}
export async function advanceBilling(
  c: PoolClient,
  holdingId: string,
  now: number,
) {
  const e = (
    await c.query(
      "SELECT * FROM billing_enrollments WHERE holding_id=$1 AND ended_at IS NULL FOR UPDATE",
      [holdingId],
    )
  ).rows[0];
  if (!e) return;
  let last = (
    await c.query(
      "SELECT * FROM billing_periods WHERE enrollment_id=$1 ORDER BY month DESC LIMIT 1",
      [e.id],
    )
  ).rows[0];
  let start = last ? Number(last.end_at) : Number(e.started_at);
  for (let safety = 0; start <= now && safety < 1200; safety++) {
    if (e.effective_at && start >= Number(e.effective_at)) {
      await c.query("INSERT INTO billing_audit VALUES($1,$2,$3,$4,$5,$6,$7)", [
        randomUUID(),
        holdingId,
        "system",
        e.pending_end ? "hosted-ended" : "plan-effective",
        "Scheduled month-end change applied",
        JSON.stringify({
          enrollmentId: e.id,
          plan: e.pending_plan,
          effectiveAt: e.effective_at,
          sandbox: true,
        }),
        now,
      ]);

      if (e.pending_end) {
        await c.query(
          "UPDATE billing_enrollments SET ended_at=effective_at,pending_end=FALSE,effective_at=NULL WHERE id=$1",
          [e.id],
        );
        break;
      }
      if (e.pending_plan) e.plan_id = e.pending_plan;
      await c.query(
        "UPDATE billing_enrollments SET plan_id=$2,pending_plan=NULL,effective_at=NULL WHERE id=$1",
        [e.id, e.plan_id],
      );
      e.effective_at = null;
    }
    const month = billingMonth(start),
      bounds = monthBounds(month);
    const plan = (
      await c.query(
        "SELECT * FROM billing_plans WHERE id=$1 AND effective_month<=$2 ORDER BY revision DESC LIMIT 1",
        [e.plan_id, month],
      )
    ).rows[0];
    if (!plan) throw new Error("BILLING_PLAN_MISSING");
    await c.query(
      "INSERT INTO billing_periods(id,holding_id,enrollment_id,month,start_at,end_at,plan_id,plan_revision,support_satang) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9) ON CONFLICT DO NOTHING",
      [
        randomUUID(),
        holdingId,
        e.id,
        month,
        start,
        bounds.end,
        plan.id,
        plan.revision,
        proratedSupport(Number(plan.support_satang), start, month),
      ],
    );
    last = { end_at: bounds.end };
    start = bounds.end;
  }
  await c.query(
    "UPDATE billing_periods SET closed_at=$2 WHERE holding_id=$1 AND end_at<=$2 AND closed_at IS NULL",
    [holdingId, now],
  );
}
export async function billingView(
  db: PostgresDatabase,
  holdingId: string,
  now = Date.now(),
): Promise<BillingView> {
  const query = async (table: string, order = "") =>
    (
      await db.pool.query(
        `SELECT * FROM ${table} WHERE holding_id=$1 ${order}`,
        [holdingId],
      )
    ).rows;
  return {
    sandbox: true,
    now,
    account: (await query("billing_accounts"))[0] ?? null,
    enrollment:
      (
        await query("billing_enrollments", "ORDER BY started_at DESC LIMIT 1")
      )[0] ?? null,
    plans: (
      await db.pool.query(
        "SELECT * FROM billing_plans WHERE id IN ('infrastructure','standard','priority') ORDER BY CASE id WHEN 'infrastructure' THEN 0 WHEN 'standard' THEN 1 ELSE 2 END,revision DESC",
      )
    ).rows,
    periods: await query("billing_periods", "ORDER BY month DESC"),
    costs: await query("billing_costs", "ORDER BY created_at DESC"),
    invoices: await query("billing_invoices", "ORDER BY created_at DESC"),
    audit: await query("billing_audit", "ORDER BY created_at DESC LIMIT 100"),
    jobs: [
      ...(await query("billing_jobs", "AND state<>'done' ORDER BY created_at")),
      ...(
        await db.pool.query(
          "SELECT i.id,'attention' AS state,i.last_error FROM stripe_inbox i JOIN billing_accounts a ON a.customer_id=i.payload_json::jsonb #>> '{data,object,customer}' WHERE a.holding_id=$1 AND i.state='pending' AND i.last_error IS NOT NULL ORDER BY i.created_at LIMIT 50",
          [holdingId],
        )
      ).rows,
    ],
    legacy: (await query("beta_billing"))[0] ?? null,
  };
}
export async function billingCommand(
  db: PostgresDatabase,
  actor: BillingActor,
  holdingId: string,
  input: BillingInput,
  key: string,
  stripe: BillingStripe,
  returnUrl: string,
  now = Date.now(),
) {
  input = billingInput.parse(input);
  if (!/^[a-zA-Z0-9:_.-]{8,180}$/.test(key))
    throw new ApiError(
      422,
      "COMMAND_KEY_REQUIRED",
      "Idempotency-Key required.",
    );
  if (
    actor.platform
      ? !adminActions.has(input.action)
      : adminActions.has(input.action) || actor.holdingId !== holdingId
  )
    throw new ApiError(
      403,
      "FORBIDDEN",
      "This billing action is unavailable for this role.",
    );
  const digest = createHash("sha256")
    .update(JSON.stringify({ actor: actor.userId, holdingId, input }))
    .digest("hex");
  const c = await db.pool.connect();
  try {
    await c.query("BEGIN");
    await c.query("SELECT pg_advisory_xact_lock(hashtextextended($1,0))", [
      `billing:${holdingId}`,
    ]);
    await c.query("SELECT pg_advisory_xact_lock(hashtextextended($1,0))", [
      `billing-command:${key}`,
    ]);
    const receipt = (
      await c.query("SELECT * FROM billing_receipts WHERE id=$1", [key])
    ).rows[0];
    if (receipt) {
      if (receipt.digest !== digest)
        fail("KEY_CONFLICT", "This key belongs to another request.");
      await c.query("COMMIT");
      return JSON.parse(receipt.result_json) as { ok: boolean; url?: string };
    }
    const exists = (
      await c.query("SELECT 1 FROM holding_bindings WHERE holding_id=$1", [
        holdingId,
      ])
    ).rowCount;
    if (!exists) throw new ApiError(404, "NOT_FOUND", "Holding not found.");
    await c.query(
      "INSERT INTO billing_accounts(holding_id,customer_id,created_at) SELECT $1,(SELECT customer_id FROM beta_billing WHERE holding_id=$1),$2 ON CONFLICT DO NOTHING",
      [holdingId, now],
    );
    const a = (
      await c.query(
        "SELECT * FROM billing_accounts WHERE holding_id=$1 FOR UPDATE",
        [holdingId],
      )
    ).rows[0];
    if (Number(a.version) !== input.version)
      fail("VERSION_CONFLICT", "Billing changed. Refresh and review again.");
    await advanceBilling(c, holdingId, now);
    const e = (
      await c.query(
        "SELECT * FROM billing_enrollments WHERE holding_id=$1 AND ended_at IS NULL",
        [holdingId],
      )
    ).rows[0];
    const result: { ok: boolean; url?: string } = { ok: true };
    const customer = async () => {
      if (!a.customer_id) {
        const s = testObject(
          await stripe(
            "customers",
            { email: actor.email, "metadata[holding_id]": holdingId },
            `erpx-billing-customer-${holdingId}`,
          ),
        );
        a.customer_id = s.id;
        await c.query(
          "UPDATE billing_accounts SET customer_id=$2 WHERE holding_id=$1",
          [holdingId, s.id],
        );
      }
      return a.customer_id as string;
    };
    const period = async () => {
      const p = (
        await c.query(
          "SELECT * FROM billing_periods WHERE id=$1 AND holding_id=$2",
          [input.periodId, holdingId],
        )
      ).rows[0];
      if (!p) fail("PERIOD_REQUIRED", "Choose a billing period.");
      return p;
    };
    switch (input.action) {
      case "enroll": {
        if (e) fail("ALREADY_ENROLLED", "A Hosted enrollment already exists.");
        if (!input.planId) fail("PLAN_REQUIRED", "Choose a Support plan.");
        const subscriptionList = await stripe(
          `subscriptions?customer=${encodeURIComponent(await customer())}&status=all&limit=100`,
        );
        if (subscriptionList.has_more)
          fail(
            "LEGACY_REVIEW_REQUIRED",
            "Review the full legacy Subscription history in Stripe before enrolling.",
          );
        const subscriptions = stripeList(subscriptionList);
        if (
          subscriptions.some(
            (s) =>
              !["canceled", "incomplete_expired"].includes(String(s.status)),
          )
        )
          fail(
            "LEGACY_ACTIVE",
            "Cancel the legacy test Subscription through Stripe before enrolling.",
          );
        await c.query(
          "INSERT INTO billing_enrollments(id,holding_id,plan_id,started_at) VALUES($1,$2,$3,$4)",
          [randomUUID(), holdingId, input.planId, now],
        );
        await advanceBilling(c, holdingId, now);
        break;
      }
      case "change-plan":
      case "cancel-hosted":
      case "withdraw": {
        if (!e) fail("NOT_ENROLLED", "No active Hosted service.");
        if (input.action === "change-plan" && !input.planId)
          fail("PLAN_REQUIRED", "Choose a plan.");
        await c.query(
          "UPDATE billing_enrollments SET pending_plan=$2,pending_end=$3,effective_at=$4 WHERE id=$1",
          [
            e.id,
            input.action === "change-plan" ? input.planId : null,
            input.action === "cancel-hosted",
            input.action === "withdraw"
              ? null
              : monthBounds(billingMonth(now)).end,
          ],
        );
        break;
      }
      case "portal":
      case "resolve-legacy":
      case "setup": {
        const id = await customer();
        const setup = input.action === "setup";
        const s = await stripe(
          setup ? "checkout/sessions" : "billing_portal/sessions",
          setup
            ? {
                customer: id,
                mode: "setup",
                currency: "thb",
                "payment_method_types[0]": "card",
                success_url: returnUrl,
                cancel_url: returnUrl,
                "setup_intent_data[metadata][holding_id]": holdingId,
              }
            : { customer: id, return_url: returnUrl },
          `erpx-billing-link-${key}`,
        );
        result.url =
          stripeUrl(s.url, ["checkout.stripe.com", "billing.stripe.com"]) ??
          undefined;
        break;
      }
      case "payment-mode": {
        if (!input.paymentMode)
          fail("MODE_REQUIRED", "Choose manual or auto payment.");
        if (input.paymentMode === "auto") {
          if (input.consent !== true)
            fail("CONSENT_REQUIRED", "Explicit Auto-pay consent is required.");
          const card = stripeList(
            await stripe(
              `payment_methods?customer=${encodeURIComponent(await customer())}&type=card&limit=1`,
            ),
          )[0];
          if (!card)
            fail("CARD_REQUIRED", "Save a test card with Stripe first.");
          testObject(card);
          await stripe(
            `customers/${a.customer_id}`,
            { "invoice_settings[default_payment_method]": card.id },
            `erpx-billing-card-${key}`,
          );
          const details = z
            .object({
              brand: z.string(),
              last4: z.string(),
              exp_month: z.number(),
              exp_year: z.number(),
            })
            .parse(card.card);
          await c.query(
            "UPDATE billing_accounts SET card_json=$2,consent_at=$3,consent_actor=$4 WHERE holding_id=$1",
            [
              holdingId,
              JSON.stringify({ id: card.id, ...details }),
              now,
              actor.userId,
            ],
          );
        } else {
          // Pause each open automatic invoice before acknowledging withdrawal. Stable keys survive retries.
          const open = (
            await c.query(
              "SELECT * FROM billing_invoices WHERE holding_id=$1 AND status='open' AND payment_mode='auto'",
              [holdingId],
            )
          ).rows;
          for (const inv of open) {
            await stripe(
              `invoices/${inv.stripe_id}`,
              { auto_advance: "false" },
              `erpx-stop-auto-${key}-${inv.id}`,
            );
            await c.query(
              "INSERT INTO billing_jobs(id,holding_id,invoice_id,kind,next_at,created_at) VALUES($1,$2,$3,'manual',$4,$4) ON CONFLICT DO NOTHING",
              [`manual-${inv.id}-${key}`, holdingId, inv.id, now],
            );
          }
          await c.query(
            "UPDATE billing_accounts SET consent_at=NULL,consent_actor=NULL WHERE holding_id=$1",
            [holdingId],
          );
        }
        await c.query(
          "UPDATE billing_accounts SET payment_mode=$2 WHERE holding_id=$1",
          [holdingId, input.paymentMode],
        );
        break;
      }
      case "cost": {
        const p = await period();
        if (
          input.amountSatang === undefined ||
          !input.category ||
          !input.note ||
          input.serviceStart === undefined ||
          input.serviceEnd === undefined
        )
          fail(
            "COST_REQUIRED",
            "Amount, category, service dates and notes are required.",
          );
        if (input.category !== "adjustment" && input.amountSatang < 0)
          fail("INVALID_COST", "Only adjustments may be negative.");
        if (
          input.serviceStart < Number(p.start_at) ||
          input.serviceEnd > Number(p.end_at) ||
          input.serviceEnd <= input.serviceStart
        )
          fail(
            "INVALID_SERVICE_DATES",
            "Service dates must be inside the original service period.",
          );
        if (input.category === "adjustment" && !input.originalInvoiceId)
          fail(
            "ORIGINAL_REQUIRED",
            "An adjustment must reference the original invoice.",
          );
        if (
          input.originalInvoiceId &&
          !(
            await c.query(
              "SELECT 1 FROM billing_invoices WHERE id=$1 AND holding_id=$2 AND period_id=$3",
              [input.originalInvoiceId, holdingId, p.id],
            )
          ).rowCount
        )
          fail(
            "INVALID_ORIGINAL",
            "Original invoice must belong to this Holding and period.",
          );
        if (
          input.evidenceId &&
          !(
            await c.query(
              "SELECT 1 FROM billing_evidence WHERE id=$1 AND holding_id=$2 AND source_state='available'",
              [input.evidenceId, holdingId],
            )
          ).rowCount
        )
          fail("INVALID_EVIDENCE", "Evidence not found.");
        if (!input.simulated && !input.evidenceId)
          fail(
            "EVIDENCE_REQUIRED",
            "Attach private evidence for an actual provider cost, or explicitly label a simulated fixture.",
          );
        const id = input.costId ?? randomUUID();
        if (input.costId) {
          const old = (
            await c.query(
              "SELECT * FROM billing_costs WHERE id=$1 AND holding_id=$2 AND period_id=$3",
              [id, holdingId, p.id],
            )
          ).rows[0];
          if (!old || old.invoice_id)
            fail("COST_FROZEN", "Issued or missing costs cannot be changed.");
        }
        await c.query(
          `INSERT INTO billing_costs(id,holding_id,period_id,category,amount_satang,service_start,service_end,note,simulated,evidence_id,original_invoice_id,created_at) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12) ON CONFLICT(id) DO UPDATE SET category=EXCLUDED.category,amount_satang=EXCLUDED.amount_satang,service_start=EXCLUDED.service_start,service_end=EXCLUDED.service_end,note=EXCLUDED.note,simulated=EXCLUDED.simulated,evidence_id=EXCLUDED.evidence_id,original_invoice_id=EXCLUDED.original_invoice_id`,
          [
            id,
            holdingId,
            p.id,
            input.category,
            input.amountSatang,
            input.serviceStart,
            input.serviceEnd,
            input.note,
            input.simulated ?? false,
            input.evidenceId ?? null,
            input.originalInvoiceId ?? null,
            now,
          ],
        );
        await c.query(
          "UPDATE billing_periods SET costs_complete=FALSE WHERE id=$1",
          [p.id],
        );
        break;
      }
      case "delete-cost": {
        const old = (
          await c.query(
            "DELETE FROM billing_costs WHERE id=$1 AND holding_id=$2 AND invoice_id IS NULL RETURNING period_id",
            [input.costId, holdingId],
          )
        ).rows[0];
        if (!old)
          fail("COST_FROZEN", "Issued or missing costs cannot be removed.");
        await c.query(
          "UPDATE billing_periods SET costs_complete=FALSE WHERE id=$1",
          [old.period_id],
        );
        break;
      }
      case "complete-costs": {
        const p = await period();
        await c.query(
          "UPDATE billing_periods SET costs_complete=$2 WHERE id=$1",
          [p.id, input.complete === true],
        );
        break;
      }
      case "approve": {
        const p = await period();
        if (!p.closed_at)
          fail(
            "MONTH_NOT_CLOSED",
            "Approval is only available after month end.",
          );
        if (!p.costs_complete)
          fail(
            "COSTS_INCOMPLETE",
            "Review and confirm complete costs, including an explicit zero-cost month.",
          );
        const previous = (
          await c.query(
            "SELECT * FROM billing_invoices WHERE period_id=$1 AND kind='main' AND status<>'void'",
            [p.id],
          )
        ).rows[0];
        if (previous && ["queued", "draft"].includes(previous.status))
          fail("INVOICE_PENDING", "Wait for the existing invoice to finish.");
        const costs = (
          await c.query(
            "SELECT * FROM billing_costs WHERE period_id=$1 AND invoice_id IS NULL ORDER BY created_at,id",
            [p.id],
          )
        ).rows;
        if (previous && !costs.length)
          fail("ALREADY_ISSUED", "No new reviewed costs to invoice.");
        const plan = (
          await c.query(
            "SELECT * FROM billing_plans WHERE id=$1 AND revision=$2",
            [p.plan_id, p.plan_revision],
          )
        ).rows[0];
        const lines: InvoiceLine[] = costs.map((v) => ({
          description: `${v.simulated ? "[Simulated] " : ""}${v.category}: ${v.note}`,
          amount: Number(v.amount_satang),
          start: Number(v.service_start),
          end: Number(v.service_end),
          costId: v.id,
          simulated: v.simulated,
        }));
        if (!previous)
          lines.unshift({
            description: `Sandbox ${plan.name} · revision ${plan.revision}`,
            amount: Number(p.support_satang),
            start: Number(p.start_at),
            end: Number(p.end_at),
          });
        const total = lines.reduce((s, v) => s + v.amount, 0);
        if (lines.length > 250)
          fail(
            "TOO_MANY_LINES",
            "Consolidate the reviewed draft to at most 250 lines before approval.",
          );
        if (total < 0)
          fail(
            "CREDIT_REVIEW_REQUIRED",
            "A net credit requires a separate reviewed Stripe credit note; automatic refunds are not enabled.",
          );
        if (a.payment_mode === "auto" && (!a.consent_at || !a.card_json))
          fail("CONSENT_REQUIRED", "Auto-pay needs a saved card and consent.");
        await customer();
        const id = randomUUID();
        await c.query(
          "INSERT INTO billing_invoices(id,holding_id,period_id,kind,total_satang,lines_json,payment_mode,created_at,approved_by,original_invoice_id,updated_at) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$8)",
          [
            id,
            holdingId,
            p.id,
            previous ? "adjustment" : "main",
            total,
            JSON.stringify(lines),
            a.payment_mode,
            now,
            actor.userId,
            previous?.id ?? null,
          ],
        );
        for (const cost of costs)
          await c.query("UPDATE billing_costs SET invoice_id=$2 WHERE id=$1", [
            cost.id,
            id,
          ]);
        await c.query(
          "INSERT INTO billing_jobs(id,holding_id,invoice_id,kind,next_at,created_at) VALUES($1,$2,$3,'issue',$4,$4)",
          [`issue-${id}`, holdingId, id, now],
        );
        break;
      }
      case "void": {
        const inv = (
          await c.query(
            "SELECT * FROM billing_invoices WHERE id=$1 AND holding_id=$2 AND status IN ('open','uncollectible')",
            [input.invoiceId, holdingId],
          )
        ).rows[0];
        if (!inv)
          fail(
            "VOID_UNAVAILABLE",
            "Only unpaid issued invoices may be voided.",
          );
        await c.query(
          "INSERT INTO billing_jobs(id,holding_id,invoice_id,kind,next_at,created_at) VALUES($1,$2,$3,'void',$4,$4) ON CONFLICT DO NOTHING",
          [`void-${inv.id}`, holdingId, inv.id, now],
        );
        break;
      }
      case "revise-plan": {
        if (input.planId === "infrastructure" && input.supportSatang !== 0)
          fail(
            "INFRASTRUCTURE_SUPPORT_ZERO",
            "Infrastructure Only must have zero Support.",
          );
        if (!input.planId || input.supportSatang === undefined)
          fail("PLAN_REQUIRED", "Choose the plan and new Support price.");
        await c.query(
          "SELECT pg_advisory_xact_lock(hashtextextended('billing-plans',0))",
        );
        const old = (
          await c.query(
            "SELECT * FROM billing_plans WHERE id=$1 ORDER BY revision DESC LIMIT 1",
            [input.planId],
          )
        ).rows[0];
        if (Number(old.revision) !== input.planRevision)
          fail(
            "PLAN_VERSION_CONFLICT",
            "The plan revision changed. Refresh and review the latest price.",
          );
        await c.query("INSERT INTO billing_plans VALUES($1,$2,$3,$4,$5,$6)", [
          input.planId,
          Number(old.revision) + 1,
          old.name,
          input.supportSatang,
          billingMonth(monthBounds(billingMonth(now)).end),
          now,
        ]);
        break;
      }
      case "assign-target": {
        if (!input.targetMonth || !input.targetMode)
          fail("TARGET_ASSIGNMENT_REQUIRED", "Choose a target month and mode.");
        const targetBounds = targetMonthBounds(input.targetMonth);
        const price = input.priceId
          ? (await c.query("SELECT * FROM billing_target_prices WHERE id=$1 AND effective_month<=$2",
              [input.priceId, input.targetMonth])).rows[0]
          : (await c.query("SELECT * FROM billing_target_prices WHERE effective_month<=$1 ORDER BY effective_month DESC,id DESC LIMIT 1",
              [input.targetMonth])).rows[0];
        if (!price) fail("TARGET_PRICE_MISSING", "No approved Sandbox price version applies to this month.");
        if (input.targetMode === "sandbox_invoice") {
          if (input.targetMonth < billingMonth(now))
            fail("TARGET_BACKDATE_FORBIDDEN", "Sandbox invoice assignments cannot be backdated.");
          const designated = (await c.query(
            "SELECT 1 FROM billing_target_sandbox_holdings WHERE holding_id=$1", [holdingId],
          )).rowCount;
          const rollout = input.targetMonth > billingMonth(now) && (await c.query(
            "SELECT 1 FROM billing_target_rollout_approvals WHERE holding_id=$1 AND effective_month<=$2",
            [holdingId, input.targetMonth],
          )).rowCount;
          if (!designated && !rollout)
            fail("TARGET_ROLLOUT_NOT_READY", "This Holding needs explicit Sandbox test designation or approved future-month rollout evidence.");
        }
        const serviceStartedAt = input.targetMonth < billingMonth(now)
          ? targetBounds.start : Math.max(now, targetBounds.start);
        const inserted = await c.query(
          `INSERT INTO billing_target_assignments
             (holding_id,effective_month,price_id,mode,service_started_at,assigned_by,reason,created_at)
           VALUES($1,$2,$3,$4,$5,$6,$7,$8) ON CONFLICT DO NOTHING RETURNING holding_id`,
          [holdingId, input.targetMonth, price.id, input.targetMode,
            serviceStartedAt, actor.userId, input.reason, now],
        );
        if (!inserted.rowCount) fail("TARGET_ASSIGNMENT_EXISTS", "This month's assignment is immutable. Choose a later month.");
        break;
      }
      case "verify-provider-cost": {
        if (!input.targetMonth || !input.sourceKey || !input.provider || !input.statementId ||
            !input.originalCurrency || input.originalMinor === undefined ||
            input.fxNumerator === undefined || input.fxDenominator === undefined ||
            input.providerCostSatang === undefined || !input.evidenceId)
          fail("PROVIDER_COST_REQUIRED", "Source, provider, statement, currency, FX, amount and evidence are required.");
        const meter = (await c.query(
          `SELECT * FROM billing_usage_meters WHERE holding_id=$1 AND month=$2 AND source_key=$3
           AND meter_kind='ai' AND funding_source='erpx' AND request_status='succeeded'`,
          [holdingId, input.targetMonth, input.sourceKey],
        )).rows[0];
        if (!meter || meter.provider !== input.provider ||
            meter.cost_status !== "reconciled" || meter.cost_reconciled_at === null ||
            meter.verified_cost_satang === null ||
            Number(meter.verified_cost_satang) !== input.providerCostSatang)
          fail("PROVIDER_COST_UNVERIFIED", "Match a successful Platform-key meter to verified provider cost first.");
        const expectedSatang = roundSatang(
          BigInt(input.originalMinor) * BigInt(input.fxNumerator),
          BigInt(input.fxDenominator),
        );
        if (expectedSatang !== input.providerCostSatang)
          fail("FX_MISMATCH", "The reviewed THB amount does not match original units and the FX ratio.");
        const evidence = (await c.query(
          "SELECT 1 FROM billing_evidence WHERE id=$1 AND holding_id=$2 AND source_state='available'",
          [input.evidenceId, holdingId],
        )).rowCount;
        if (!evidence) fail("EVIDENCE_REQUIRED", "Attach available private cost evidence.");
        const prior = (await c.query(
          "SELECT 1 FROM billing_target_rated_periods WHERE holding_id=$1 AND month=$2 AND state='approved'",
          [holdingId, input.targetMonth],
        )).rowCount;
        if (prior) fail("TARGET_PERIOD_FROZEN", "Approved target charges cannot be changed.");
        const existing = (await c.query(
          "SELECT * FROM billing_target_provider_costs WHERE holding_id=$1 AND month=$2 AND source_key=$3",
          [holdingId, input.targetMonth, input.sourceKey],
        )).rows[0];
        if (existing) fail("PROVIDER_COST_ALREADY_VERIFIED", "The source already has verified cost; preserve its audit trail.");
        await c.query(
          `INSERT INTO billing_target_provider_costs
           (holding_id,month,source_key,provider,statement_id,original_currency,original_minor,
            fx_numerator,fx_denominator,provider_cost_satang,evidence_id,verified_by,verified_at)
           VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13)`,
          [holdingId, input.targetMonth, input.sourceKey, input.provider,
            input.statementId, input.originalCurrency, input.originalMinor,
            input.fxNumerator, input.fxDenominator, input.providerCostSatang,
            input.evidenceId, actor.userId, now],
        );
        break;
      }
      case "verify-statement": {
        if (!input.targetMonth || !input.provider || !input.statementId ||
            !input.originalCurrency || input.providerTotalMinor === undefined || !input.evidenceId)
          fail("STATEMENT_REQUIRED", "Month, provider, statement, currency, total and evidence are required.");
        const evidence = (await c.query(
          "SELECT 1 FROM billing_evidence WHERE id=$1 AND holding_id=$2 AND source_state='available'",
          [input.evidenceId, holdingId],
        )).rowCount;
        if (!evidence) fail("EVIDENCE_REQUIRED", "Attach an available provider statement.");
        const prior = (await c.query(
          "SELECT 1 FROM billing_target_rated_periods WHERE holding_id=$1 AND month=$2 AND state='approved'",
          [holdingId, input.targetMonth],
        )).rowCount;
        if (prior) fail("TARGET_PERIOD_FROZEN", "Approved target charges cannot be changed.");
        const count = (await c.query(
          `SELECT COUNT(*)::int AS count, COALESCE(SUM(original_minor),0)::text AS total
           FROM billing_target_provider_costs WHERE holding_id=$1 AND month=$2 AND provider=$3
             AND statement_id=$4 AND original_currency=$5`,
          [holdingId, input.targetMonth, input.provider, input.statementId,
            input.originalCurrency],
        )).rows[0];
        if (!count.count || BigInt(count.total) !== BigInt(input.providerTotalMinor))
          fail("STATEMENT_MISMATCH", "Statement total must equal the matched provider source costs.");
        const inserted = await c.query(
          `INSERT INTO billing_target_statements
           (holding_id,month,provider,statement_id,evidence_id,provider_total_minor,
            original_currency,reconciled_total_minor,verified_by,verified_at)
           VALUES($1,$2,$3,$4,$5,$6,$7,$7,$8,$9) ON CONFLICT DO NOTHING RETURNING holding_id`,
          [holdingId, input.targetMonth, input.provider, input.statementId,
            input.evidenceId, input.providerTotalMinor, input.originalCurrency,
            actor.userId, now],
        );
        if (!inserted.rowCount) fail("STATEMENT_ALREADY_VERIFIED", "The provider statement was already verified.");
        break;
      }
      case "record-operating-cost": {
        if (!input.targetMonth || !input.operatingCategory ||
            input.operatingCostSatang === undefined || !input.evidenceId)
          fail("OPERATING_COST_REQUIRED", "Month, category, actual cost and evidence are required.");
        if (now < monthBounds(input.targetMonth).end)
          fail("MONTH_NOT_CLOSED", "Record actual operating costs only after the month ends.");
        if (!(await c.query(
          "SELECT 1 FROM billing_evidence WHERE id=$1 AND holding_id=$2 AND source_state='available'",
          [input.evidenceId, holdingId],
        )).rowCount) fail("EVIDENCE_REQUIRED", "Attach available operating cost evidence.");
        const inserted = await c.query(
          `INSERT INTO billing_target_operating_costs
           (holding_id,month,category,amount_satang,evidence_id,verified_by,verified_at)
           VALUES($1,$2,$3,$4,$5,$6,$7) ON CONFLICT DO NOTHING RETURNING holding_id`,
          [holdingId, input.targetMonth, input.operatingCategory,
            input.operatingCostSatang, input.evidenceId, actor.userId, now],
        );
        if (!inserted.rowCount) fail("OPERATING_COST_ALREADY_VERIFIED", "The actual cost is immutable; add a future adjustment with new evidence.");
        break;
      }
      case "review-target-capacity": {
        if (!input.targetMonth || input.projectedOriginalBytes === undefined ||
            input.primaryAvailableBytes === undefined || input.backupAvailableBytes === undefined ||
            input.reservedBytes === undefined || input.companyLimitBytes === undefined ||
            !input.evidenceId)
          fail("CAPACITY_REVIEW_REQUIRED", "Month, primary and backup capacity, Company cap and evidence are required.");
        if (input.targetMonth <= billingMonth(now))
          fail("FUTURE_MONTH_REQUIRED", "Capacity approval must precede a future effective month.");
        if (!(await c.query(
          "SELECT 1 FROM billing_evidence WHERE id=$1 AND holding_id=$2 AND source_state='available'",
          [input.evidenceId, holdingId],
        )).rowCount) fail("EVIDENCE_REQUIRED", "Attach available capacity evidence.");
        const inserted = await c.query(
          `INSERT INTO billing_target_capacity_reviews
           (holding_id,effective_month,projected_original_bytes,primary_available_bytes,
            backup_available_bytes,reserved_bytes,company_limit_bytes,evidence_id,
            approved_by,reason,approved_at)
           VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11)
           ON CONFLICT DO NOTHING RETURNING holding_id`,
          [holdingId, input.targetMonth, input.projectedOriginalBytes,
            input.primaryAvailableBytes, input.backupAvailableBytes, input.reservedBytes,
            input.companyLimitBytes, input.evidenceId, actor.userId, input.reason, now],
        );
        if (!inserted.rowCount) fail("CAPACITY_ALREADY_REVIEWED", "Capacity evidence for this effective month is immutable.");
        break;
      }
      case "approve-target-rollout": {
        if (!input.targetMonth || input.minimumMarginBps === undefined)
          fail("ROLLOUT_REVIEW_REQUIRED", "Future month and required gross-margin threshold are required.");
        if (input.targetMonth <= billingMonth(now))
          fail("FUTURE_MONTH_REQUIRED", "An existing customer may migrate only in a future month.");
        const review = await calculateTargetRolloutReadiness(
          c, holdingId, input.targetMonth, input.minimumMarginBps, now,
        );
        if (!review.ready)
          fail("ROLLOUT_EVIDENCE_INCOMPLETE", `Migration remains held: ${review.blockers.join(", ")}.`);
        const inserted = await c.query(
          `INSERT INTO billing_target_rollout_approvals
           (holding_id,effective_month,shadow_month_1,shadow_month_2,
            minimum_margin_bps,approved_by,reason,approved_at)
           VALUES($1,$2,$3,$4,$5,$6,$7,$8) ON CONFLICT DO NOTHING RETURNING holding_id`,
          [holdingId, input.targetMonth, review.shadowMonths[0], review.shadowMonths[1],
            input.minimumMarginBps, actor.userId, input.reason, now],
        );
        if (!inserted.rowCount) fail("ROLLOUT_ALREADY_APPROVED", "This rollout decision is already recorded.");
        break;
      }
      case "approve-target-extra": {
        if (!input.targetMonth || !input.extraKind || input.amountSatang === undefined ||
            input.serviceStart === undefined || input.serviceEnd === undefined ||
            !input.note || !input.evidenceId)
          fail("EXTRA_APPROVAL_REQUIRED", "Service kind, amount, dates, description and approved quote evidence are required.");
        const bounds = monthBounds(input.targetMonth);
        if (input.serviceStart < bounds.start || input.serviceEnd > bounds.end ||
            input.serviceEnd <= input.serviceStart)
          fail("EXTRA_SERVICE_DATES", "Extra service dates must fall inside the chosen month.");
        if (!(await c.query(
          "SELECT 1 FROM billing_target_assignments WHERE holding_id=$1 AND effective_month<=$2",
          [holdingId, input.targetMonth],
        )).rowCount) fail("TARGET_ASSIGNMENT_REQUIRED", "Assign the target offering before approving extra services.");
        if ((await c.query(
          "SELECT 1 FROM billing_target_rated_periods WHERE holding_id=$1 AND month=$2 AND state='approved'",
          [holdingId, input.targetMonth],
        )).rowCount) fail("TARGET_PERIOD_FROZEN", "Approved target charges cannot be changed.");
        if (!(await c.query(
          "SELECT 1 FROM billing_evidence WHERE id=$1 AND holding_id=$2 AND source_state='available'",
          [input.evidenceId, holdingId],
        )).rowCount) fail("EVIDENCE_REQUIRED", "Attach the separately approved quote as available private evidence.");
        await c.query(
          `INSERT INTO billing_target_extras
           (id,holding_id,month,kind,description,amount_satang,service_start,service_end,
            evidence_id,approved_by,approval_reason,approved_at)
           VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12)`,
          [randomUUID(), holdingId, input.targetMonth, input.extraKind, input.note,
            input.amountSatang, input.serviceStart, input.serviceEnd,
            input.evidenceId, actor.userId, input.reason, now],
        );
        break;
      }
      case "rate-target": {
        if (!input.targetMonth) fail("TARGET_MONTH_REQUIRED", "Choose a target billing month.");
        const target = await calculateTargetBillingPeriod(c, holdingId, input.targetMonth, now);
        if (target.state === "approved")
          fail("TARGET_PERIOD_FROZEN", "Approved target charges cannot be recalculated.");
        await storeTargetRating(c, target, actor, input.reason, now);
        break;
      }
      case "approve-target": {
        if (!input.targetMonth) fail("TARGET_MONTH_REQUIRED", "Choose a target billing month.");
        // Projection refresh locks this snapshot row before replacing source
        // meters and seats. Hold the same row through invoice freeze so a
        // concurrent refresh cannot change the reviewed billing inputs.
        await c.query(
          "SELECT snapshot_id FROM billing_usage_snapshots WHERE holding_id=$1 AND month=$2 FOR UPDATE",
          [holdingId, input.targetMonth],
        );
        const target = await calculateTargetBillingPeriod(c, holdingId, input.targetMonth, now);
        if (target.assignment?.mode !== "sandbox_invoice")
          fail("TARGET_SHADOW_ONLY", "This Holding is configured for shadow rating only.");
        if (!(await c.query(
          `SELECT 1 FROM billing_target_sandbox_holdings WHERE holding_id=$1
           UNION ALL SELECT 1 FROM billing_target_rollout_approvals
           WHERE holding_id=$1 AND effective_month<=$2 LIMIT 1`,
          [holdingId, input.targetMonth],
        )).rowCount)
          fail("TARGET_ROLLOUT_NOT_READY", "This Holding needs Sandbox test designation or approved rollout evidence.");
        if (!target.completeness.ready || target.totalSatang === null)
          fail("TARGET_PERIOD_INCOMPLETE", `Hold the entire invoice: ${target.completeness.missing.join(", ")}.`);
        if (target.invoice || (await c.query(
          "SELECT 1 FROM billing_target_rated_periods WHERE holding_id=$1 AND month=$2 AND state='approved'",
          [holdingId, input.targetMonth],
        )).rowCount) fail("ALREADY_ISSUED", "The target period is already approved.");
        await storeTargetRating(c, target, actor, input.reason, now);
        const bounds = targetMonthBounds(input.targetMonth);
        await advanceBilling(c, holdingId, now);
        const legacy = (await c.query(
          `SELECT 1 FROM billing_enrollments WHERE holding_id=$1 AND plan_id<>'platform_usage'
           AND started_at<$3 AND COALESCE(ended_at,effective_at,9223372036854775807)>$2`,
          [holdingId, bounds.start, bounds.end],
        )).rowCount;
        if (legacy) fail("LEGACY_ENROLLMENT_ACTIVE", "Close the grandfathered Support enrollment before switching a month to target billing.");
        const oldInvoices = (await c.query(
          `SELECT 1 FROM billing_invoices i JOIN billing_periods p ON p.id=i.period_id
           WHERE i.holding_id=$1 AND p.month=$2 AND i.status<>'void'`,
          [holdingId, input.targetMonth],
        )).rowCount;
        if (oldInvoices) fail("MONTH_ALREADY_INVOICED", "This Holding already has an invoice for the month.");
        let enrollment = (await c.query(
          "SELECT * FROM billing_enrollments WHERE holding_id=$1 AND plan_id='platform_usage' AND ended_at IS NULL",
          [holdingId],
        )).rows[0];
        if (!enrollment) {
          await c.query(
            "INSERT INTO billing_enrollments(id,holding_id,plan_id,started_at) VALUES($1,$2,'platform_usage',$3)",
            [randomUUID(), holdingId, Math.max(bounds.start, target.assignment.serviceStartedAt)],
          );
          await advanceBilling(c, holdingId, now);
          enrollment = (await c.query(
            "SELECT * FROM billing_enrollments WHERE holding_id=$1 AND plan_id='platform_usage' AND ended_at IS NULL",
            [holdingId],
          )).rows[0];
        }
        const p = (await c.query(
          "SELECT * FROM billing_periods WHERE enrollment_id=$1 AND month=$2",
          [enrollment.id, input.targetMonth],
        )).rows[0];
        if (!p || !p.closed_at) fail("TARGET_PERIOD_MISSING", "The target service period is not closed.");
        if (a.payment_mode === "auto" && (!a.consent_at || !a.card_json))
          fail("CONSENT_REQUIRED", "Auto-pay needs a saved card and consent.");
        await customer();
        const lines = target.lines;
        const aiAmount = lines.filter(line => line.kind === "ai")
          .reduce((sum, line) => sum + (line.amountSatang ?? 0), 0);
        const extraAmount = lines.filter(line => line.kind === "extra")
          .reduce((sum, line) => sum + (line.amountSatang ?? 0), 0);
        const serviceStart = Math.max(bounds.start, target.assignment.serviceStartedAt);
        const invoiceLines: InvoiceLine[] = [
          { description: `Sandbox Holding platform fee · ${input.targetMonth}`,
            amount: lines.find(line => line.kind === "base")?.amountSatang ?? 0,
            start: serviceStart, end: bounds.end },
          { description: "Active named seats · prorated seat-months",
            amount: lines.find(line => line.kind === "seat" && line.sourceKey === null)?.amountSatang ?? 0,
            start: serviceStart, end: bounds.end },
          { description: "Original file storage over included allowance",
            amount: lines.find(line => line.kind === "storage" && line.sourceKey === null)?.amountSatang ?? 0,
            start: serviceStart, end: bounds.end },
          { description: "Verified Platform-key AI usage plus 10%",
            amount: aiAmount, start: serviceStart, end: bounds.end },
          { description: "Separately approved setup and Support services",
            amount: extraAmount, start: serviceStart, end: bounds.end },
        ].filter(line => line.amount !== 0);
        if (!invoiceLines.length) invoiceLines.push({ description: "Sandbox Holding service", amount: 0,
          start: serviceStart, end: bounds.end });
        const invoiceId = randomUUID();
        await c.query(
          `INSERT INTO billing_invoices
           (id,holding_id,period_id,kind,total_satang,lines_json,payment_mode,created_at,
            approved_by,original_invoice_id,updated_at,rating_model)
           VALUES($1,$2,$3,'main',$4,$5,$6,$7,$8,NULL,$7,'target')`,
          [invoiceId, holdingId, p.id, target.totalSatang, JSON.stringify(invoiceLines),
            a.payment_mode, now, actor.userId],
        );
        await c.query(
          `UPDATE billing_target_rated_periods SET state='approved', approved_by=$3,
           approved_at=$4,approval_reason=$5,invoice_id=$6
           WHERE holding_id=$1 AND month=$2 AND state='ready'`,
          [holdingId, input.targetMonth, actor.userId, now, input.reason, invoiceId],
        );
        await c.query(
          "INSERT INTO billing_jobs(id,holding_id,invoice_id,kind,next_at,created_at) VALUES($1,$2,$3,'issue',$4,$4)",
          [`issue-${invoiceId}`, holdingId, invoiceId, now],
        );
        break;
      }
      case "retry": {
        const job = (
          await c.query(
            "SELECT * FROM billing_jobs WHERE id=$1 AND holding_id=$2",
            [input.jobId, holdingId],
          )
        ).rows[0];
        if (!job) {
          const event = await c.query(
            "UPDATE stripe_inbox SET next_at=$2 WHERE id=$1 AND state='pending' AND payload_json::jsonb #>> '{data,object,customer}'=(SELECT customer_id FROM billing_accounts WHERE holding_id=$3) RETURNING id",
            [input.jobId, now, holdingId],
          );
          if (!event.rowCount) fail("JOB_MISSING", "Job not found.");
          break;
        }
        if (job.last_error === "IDEMPOTENCY_WINDOW_EXPIRED")
          fail(
            "MANUAL_RECONCILIATION_REQUIRED",
            "Reconcile the ambiguous Stripe operation before retrying. Do not create another invoice.",
          );
        await c.query(
          "UPDATE billing_jobs SET state='pending',next_at=$2 WHERE id=$1",
          [job.id, now],
        );
        break;
      }
    }
    await c.query(
      "UPDATE billing_accounts SET version=version+1 WHERE holding_id=$1",
      [holdingId],
    );
    await c.query("INSERT INTO billing_audit VALUES($1,$2,$3,$4,$5,$6,$7)", [
      randomUUID(),
      holdingId,
      actor.userId,
      input.action,
      input.reason,
      JSON.stringify(input),
      now,
    ]);
    await c.query("INSERT INTO billing_receipts VALUES($1,$2,$3,$4)", [
      key,
      digest,
      JSON.stringify(result),
      now,
    ]);
    await c.query("COMMIT");
    return result;
  } catch (error) {
    await c.query("ROLLBACK");
    throw error;
  } finally {
    c.release();
  }
}
