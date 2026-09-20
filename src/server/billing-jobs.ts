import "server-only";
import type { PoolClient } from "pg";
import type { PostgresDatabase } from "./runtime/postgres";
import type { InvoiceLine } from "@/lib/billing";
import { advanceBilling } from "./billing";
import {
  billingStripe,
  stripeUrl,
  stripeList,
  testObject,
  type BillingStripe,
  type StripeObject,
} from "./billing-stripe";
import type { BetaEnvironment } from "./beta-config";
import { applyLegacyStripeEvent } from "./beta-billing";
import type { SqlDatabase } from "./runtime/types";
export type BillingJobEnv = BetaEnvironment & {
  CONTROL_DB: SqlDatabase;
  BETTER_AUTH_URL: string;
};
export async function mirrorInvoice(
  c: PoolClient,
  invoiceId: string,
  stripe: BillingStripe,
  now: number,
) {
  const local = (
    await c.query("SELECT * FROM billing_invoices WHERE id=$1", [invoiceId])
  ).rows[0];
  if (!local?.stripe_id) return;
  const s = testObject(
    await stripe(`invoices/${local.stripe_id}?expand[]=payment_intent`),
  );
  if (
    !["draft", "open", "paid", "void", "uncollectible"].includes(
      String(s.status),
    )
  )
    throw new Error("INVALID_INVOICE_STATUS");
  const pi = s.payment_intent as StripeObject | null;
  // Current Stripe retrieval is serialized per Holding, so delayed webhooks never replay their old payload.
  await c.query(
    "UPDATE billing_invoices SET status=$2,hosted_url=$3,pdf_url=$4,due_at=$5,payment_state=$6,updated_at=$7 WHERE id=$1",
    [
      invoiceId,
      s.status,
      stripeUrl(s.hosted_invoice_url, ["invoice.stripe.com"]),
      stripeUrl(s.invoice_pdf, ["pay.stripe.com", "invoice.stripe.com"]),
      typeof s.due_date === "number" ? s.due_date * 1000 : null,
      pi?.status ?? null,
      now,
    ],
  );
  if (s.status === "void" && local.status !== "void") {
    await c.query(
      "UPDATE billing_costs SET invoice_id=NULL WHERE invoice_id=$1",
      [invoiceId],
    );
    await c.query(
      "UPDATE billing_periods SET costs_complete=FALSE WHERE id=$1",
      [local.period_id],
    );
  }
}
async function processJob(
  c: PoolClient,
  job: Record<string, unknown>,
  stripe: BillingStripe,
  now: number,
) {
  const invoice = (
    await c.query("SELECT * FROM billing_invoices WHERE id=$1", [
      job.invoice_id,
    ])
  ).rows[0];
  const a = (
    await c.query("SELECT * FROM billing_accounts WHERE holding_id=$1", [
      job.holding_id,
    ])
  ).rows[0];
  const progress = JSON.parse(String(job.progress_json)) as {
    created?: boolean;
    items?: number;
    finalized?: boolean;
    sent?: boolean;
    operationAt?: number;
  };
  const save = async () => {
    await c.query("UPDATE billing_jobs SET progress_json=$2 WHERE id=$1", [
      job.id,
      JSON.stringify(progress),
    ]);
  };
  if (job.kind === "issue") {
    const mode =
      a.payment_mode === "auto" && a.consent_at && a.card_json
        ? "auto"
        : "manual";
    if (!invoice.stripe_id) {
      // Stripe retains keys at least 24h. Stop ambiguous creation before that window expires.
      if (progress.operationAt && now - progress.operationAt > 23 * 3600000)
        throw new Error("IDEMPOTENCY_WINDOW_EXPIRED");
      if (!progress.operationAt) {
        progress.operationAt = now;
        await save();
      }
      const s = testObject(
        await stripe(
          "invoices",
          {
            customer: a.customer_id,
            auto_advance: "false",
            collection_method:
              invoice.payment_mode === "auto"
                ? "charge_automatically"
                : "send_invoice",
            ...(invoice.payment_mode === "manual"
              ? { days_until_due: "7" }
              : {}),
            "metadata[erpx_invoice_id]": invoice.id,
            "metadata[holding_id]": a.holding_id,
            description: "ERPX Sandbox — reviewed actual costs + Support",
            pending_invoice_items_behavior: "exclude",
          },
          `erpx-invoice-${invoice.id}`,
        ),
      );
      invoice.stripe_id = s.id;
      await c.query(
        "UPDATE billing_invoices SET stripe_id=$2,status='draft' WHERE id=$1",
        [invoice.id, s.id],
      );
    }
    const lines = JSON.parse(invoice.lines_json) as InvoiceLine[];
    for (let i = progress.items ?? 0; i < lines.length; i++) {
      if (progress.operationAt && now - progress.operationAt > 23 * 3600000)
        throw new Error("IDEMPOTENCY_WINDOW_EXPIRED");
      const line = lines[i];
      testObject(
        await stripe(
          "invoiceitems",
          {
            customer: a.customer_id,
            invoice: invoice.stripe_id,
            currency: "thb",
            amount: String(line.amount),
            description: line.description,
            "period[start]": String(Math.floor(line.start / 1000)),
            "period[end]": String(Math.floor(line.end / 1000)),
          },
          `erpx-invoice-${invoice.id}-line-${i}`,
        ),
      );
      progress.items = i + 1;
      await save();
    }
    // Recheck withdrawal before finalization; draft collection method can still change.
    const current = testObject(await stripe(`invoices/${invoice.stripe_id}`));
    if (current.status === "draft") {
      if (Number(current.total) !== Number(invoice.total_satang))
        throw new Error("STRIPE_TOTAL_MISMATCH");
      await stripe(
        `invoices/${invoice.stripe_id}`,
        {
          collection_method:
            mode === "auto" ? "charge_automatically" : "send_invoice",
          ...(mode === "manual" ? { days_until_due: "7" } : {}),
          auto_advance: "false",
        },
        `erpx-configure-${invoice.id}-${mode}`,
      );
      await stripe(
        `invoices/${invoice.stripe_id}/finalize`,
        { auto_advance: "false" },
        `erpx-finalize-${invoice.id}`,
      );
    }
    progress.finalized = true;
    await save();
    await c.query("UPDATE billing_invoices SET payment_mode=$2 WHERE id=$1", [
      invoice.id,
      mode,
    ]);
    // Stripe owns payment retries. ERPX never calls /pay repeatedly.
    const finalized = testObject(await stripe(`invoices/${invoice.stripe_id}`));
    if (finalized.status === "open" && mode === "auto")
      await stripe(
        `invoices/${invoice.stripe_id}`,
        { auto_advance: "true" },
        `erpx-collect-${invoice.id}`,
      );
    else if (finalized.status === "open" && !progress.sent) {
      await stripe(
        `invoices/${invoice.stripe_id}/send`,
        {},
        `erpx-send-${invoice.id}`,
      );
      progress.sent = true;
      await save();
    }
  } else if (job.kind === "void") {
    const s = testObject(await stripe(`invoices/${invoice.stripe_id}`));
    if (s.status === "paid") throw new Error("PAID_INVOICE_IMMUTABLE");
    if (s.status !== "void")
      await stripe(
        `invoices/${invoice.stripe_id}/void`,
        {},
        `erpx-void-${invoice.id}`,
      );
  } else if (job.kind === "manual") {
    const s = testObject(await stripe(`invoices/${invoice.stripe_id}`));
    if (s.status === "open")
      await stripe(
        `invoices/${invoice.stripe_id}`,
        { auto_advance: "false" },
        `erpx-manual-${job.id}`,
      );
  }
  await mirrorInvoice(c, invoice.id, stripe, now);
  await c.query(
    "UPDATE billing_jobs SET state='done',last_error=NULL WHERE id=$1",
    [job.id],
  );
}
export async function drainBilling(
  env: BillingJobEnv,
  now = Date.now(),
  stripe: BillingStripe = billingStripe(env),
) {
  const db = env.CONTROL_DB as PostgresDatabase;
  const ids = (await db.pool.query("SELECT holding_id FROM billing_accounts"))
    .rows;
  for (const { holding_id: id } of ids) {
    const c = await db.pool.connect();
    let locked = false;
    try {
      locked = (
        await c.query(
          "SELECT pg_try_advisory_lock(hashtextextended($1,0)) AS locked",
          [`billing:${id}`],
        )
      ).rows[0].locked;
      if (!locked) continue;
      await c.query("BEGIN");
      await advanceBilling(c, id, now);
      await c.query("COMMIT");
      // Portal changes and detached cards are reflected without storing full card data.
      const account = (
        await c.query("SELECT * FROM billing_accounts WHERE holding_id=$1", [
          id,
        ])
      ).rows[0];
      if (account.customer_id) {
        try {
          const customer = testObject(
            await stripe(
              `customers/${account.customer_id}?expand[]=invoice_settings.default_payment_method`,
            ),
          );
          const settings = customer.invoice_settings as
            | { default_payment_method?: StripeObject | string | null }
            | undefined;
          const preferred = settings?.default_payment_method;
          const card =
            typeof preferred === "object" && preferred
              ? preferred
              : typeof preferred === "string"
                ? testObject(await stripe(`payment_methods/${preferred}`))
                : stripeList(
                    await stripe(
                      `payment_methods?customer=${encodeURIComponent(account.customer_id)}&type=card&limit=1`,
                    ),
                  )[0];
          const details = card?.card as Record<string, unknown> | undefined;
          const masked = details
            ? JSON.stringify({
                id: card.id,
                brand: details.brand,
                last4: details.last4,
                exp_month: details.exp_month,
                exp_year: details.exp_year,
              })
            : null;
          if (masked !== account.card_json)
            await c.query(
              "UPDATE billing_accounts SET card_json=$2 WHERE holding_id=$1",
              [id, masked],
            );
        } catch {
          /* A temporary Stripe outage must not erase the previously verified card. */
        }
      }
      const jobs = (
        await c.query(
          "SELECT * FROM billing_jobs WHERE holding_id=$1 AND state='pending' AND next_at<=$2 ORDER BY created_at,id LIMIT 10",
          [id, now],
        )
      ).rows;
      for (const job of jobs) {
        try {
          await processJob(c, job, stripe, now);
        } catch (error) {
          const code =
            error instanceof Error ? error.message : "STRIPE_UNAVAILABLE";
          await c.query(
            "UPDATE billing_jobs SET attempts=attempts+1,state=$2,next_at=$3,last_error=$4 WHERE id=$1",
            [
              job.id,
              job.attempts >= 5 ||
              [
                "IDEMPOTENCY_WINDOW_EXPIRED",
                "STRIPE_TOTAL_MISMATCH",
                "PAID_INVOICE_IMMUTABLE",
              ].includes(code)
                ? "attention"
                : "pending",
              now + Math.min(3600000, 30000 * 2 ** job.attempts),
              /^[A-Z_]+$/.test(code) ? code : "STRIPE_RETRY_REQUIRED",
            ],
          );
        }
      }
      const events = (
        await c.query(
          "SELECT * FROM stripe_inbox WHERE state='pending' AND next_at<=$1 AND payload_json::jsonb #>> '{data,object,customer}'=(SELECT customer_id FROM billing_accounts WHERE holding_id=$2) ORDER BY created_at,id LIMIT 50",
          [now, id],
        )
      ).rows;
      for (const event of events) {
        try {
          const e = JSON.parse(event.payload_json),
            obj = e.data.object;
          const matches = (
            await c.query(
              "SELECT id FROM billing_invoices WHERE holding_id=$1 AND stripe_id=$2",
              [
                id,
                e.type.startsWith("invoice.")
                  ? obj.id
                  : typeof obj.invoice === "string"
                    ? obj.invoice
                    : null,
              ],
            )
          ).rows;
          for (const m of matches) await mirrorInvoice(c, m.id, stripe, now);
          if (e.type.startsWith("setup_intent.")) {
            const setup = testObject(await stripe(`setup_intents/${obj.id}`));
            if (
              setup.customer !==
              (
                await c.query(
                  "SELECT customer_id FROM billing_accounts WHERE holding_id=$1",
                  [id],
                )
              ).rows[0].customer_id
            )
              throw new Error("SETUP_CUSTOMER_MISMATCH");
            await c.query(
              "UPDATE billing_accounts SET setup_state=$2 WHERE holding_id=$1",
              [id, String(setup.status)],
            );
            if (
              setup.status === "succeeded" &&
              setup.customer ===
                (
                  await c.query(
                    "SELECT customer_id FROM billing_accounts WHERE holding_id=$1",
                    [id],
                  )
                ).rows[0].customer_id &&
              typeof setup.payment_method === "string"
            ) {
              const card = testObject(
                await stripe(`payment_methods/${setup.payment_method}`),
              );
              const details = card.card as Record<string, unknown> | undefined;
              if (details)
                await c.query(
                  "UPDATE billing_accounts SET card_json=$2 WHERE holding_id=$1",
                  [
                    id,
                    JSON.stringify({
                      id: card.id,
                      brand: details.brand,
                      last4: details.last4,
                      exp_month: details.exp_month,
                      exp_year: details.exp_year,
                    }),
                  ],
                );
            }
          }
          if (e.type.startsWith("customer.subscription."))
            await applyLegacyStripeEvent(env, e);
          await c.query(
            "UPDATE stripe_inbox SET state='applied',applied_at=$2,last_error=NULL WHERE id=$1",
            [event.id, now],
          );
        } catch {
          await c.query(
            "UPDATE stripe_inbox SET attempts=attempts+1,next_at=$2,last_error=$3 WHERE id=$1",
            [event.id, now + 60000, "STRIPE_RECONCILIATION_REQUIRED"],
          );
        }
      }
      // Recover missed deliveries and keep payment state current, independent of redirects.
      const stale = (
        await c.query(
          "SELECT id FROM billing_invoices WHERE holding_id=$1 AND status IN ('open','draft') AND stripe_id IS NOT NULL AND updated_at<$2 LIMIT 10",
          [id, now - 300000],
        )
      ).rows;
      for (const row of stale) {
        try {
          await mirrorInvoice(c, row.id, stripe, now);
        } catch {
          /* Next scheduled pass retries. */
        }
      }
    } catch (error) {
      await c.query("ROLLBACK").catch(() => {});
      throw error;
    } finally {
      if (locked)
        await c.query("SELECT pg_advisory_unlock(hashtextextended($1,0))", [
          `billing:${id}`,
        ]);
      c.release();
    }
  }
  // Legacy-only customers also need durable reconciliation after the endpoint acknowledges.
  const legacy = (
    await db.pool.query(
      "SELECT * FROM stripe_inbox WHERE state='pending' AND next_at<=$1 AND NOT EXISTS(SELECT 1 FROM billing_accounts a WHERE a.customer_id=stripe_inbox.payload_json::jsonb #>> '{data,object,customer}') LIMIT 20",
      [now],
    )
  ).rows;
  for (const event of legacy) {
    try {
      await applyLegacyStripeEvent(env, JSON.parse(event.payload_json));
    } catch {
      await db
        .prepare(
          "UPDATE stripe_inbox SET attempts=attempts+1,next_at=?,last_error=? WHERE id=?",
        )
        .bind(now + 60000, "STRIPE_RECONCILIATION_REQUIRED", event.id)
        .run();
    }
  }
}
