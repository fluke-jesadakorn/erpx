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
]);
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
        "SELECT * FROM billing_plans ORDER BY CASE id WHEN 'infrastructure' THEN 0 WHEN 'standard' THEN 1 ELSE 2 END,revision DESC",
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
              "SELECT 1 FROM billing_evidence WHERE id=$1 AND holding_id=$2",
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
