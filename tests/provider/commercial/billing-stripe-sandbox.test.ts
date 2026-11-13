import { beforeAll, it, expect } from "vitest";
import { env, applyMigrations } from "../../support/runtime-env";
import {
  billingCommand,
  billingView,
  type BillingInput,
} from "@erpx/server-billing/public/billing";
import { drainBilling } from "@erpx/server-billing/public/billing-jobs";
import type { InvoiceLine } from "@erpx/feature-billing/lib/billing";
import { billingStripe, testObject } from "@erpx/server-billing/public/billing-stripe";
const enabled =
  process.env.ERPX_TEST_STRIPE_SANDBOX === "1" &&
  process.env.STRIPE_SECRET_KEY?.startsWith("sk_test_");
beforeAll(async () => {
  if (enabled)
    await applyMigrations(env.CONTROL_DB, env.CONTROL_TEST_MIGRATIONS);
});
it.skipIf(!enabled)(
  "runs reviewed manual and auto collection against actual Stripe Sandbox, with isolated accounts and an injected clock",
  async () => {
    const stripe = billingStripe({
      STRIPE_SECRET_KEY: process.env.STRIPE_SECRET_KEY,
    });
    for (const mode of ["manual", "auto"] as const) {
      const id = `stripe_fixture_${crypto.randomUUID()}`,
        db = env.CONTROL_DB,
        now = Date.parse("2026-09-01T00:00:00+07:00");
      await db.pool.query(
        "INSERT INTO holding_bindings(holding_id,slug,display_name,write_binding,read_binding,files_binding,status) VALUES($1,$1,'Disposable Stripe billing fixture',$2,$3,$4,'active')",
        [id, `W_${id}`, `R_${id}`, `F_${id}`],
      );
      const owner = {
          userId: "fixture-owner",
          holdingId: id,
          platform: false,
          email: "erpx-billing-fixture@example.test",
        },
        admin = { ...owner, userId: "fixture-admin", platform: true };
      const view = () => billingView(db, id, now);
      const cmd = async (
        action: BillingInput["action"],
        fields: Partial<BillingInput> = {},
        at = now,
        platform = false,
      ) =>
        billingCommand(
          db,
          platform ? admin : owner,
          id,
          {
            action,
            version: (await view()).account?.version ?? 0,
            reason: "Isolated Sandbox acceptance fixture",
            ...fields,
          },
          crypto.randomUUID(),
          stripe,
          "https://console-dev-erpx.jesadakorn.com",
          at,
        );
      const seedCost = async (
        periodId: string,
        fields: {
          category: "hosting" | "storage" | "provider_ai" | "adjustment";
          amountSatang: number;
          note: string;
        },
      ) => {
        const period = (await view()).periods.find(
          (row) => row.id === periodId,
        )!;
        const costId = `cost_${crypto.randomUUID()}`;
        await db.pool.query(
          `INSERT INTO billing_costs
            (id,holding_id,period_id,category,amount_satang,service_start,service_end,
             note,simulated,evidence_id,original_invoice_id,created_at)
           VALUES($1,$2,$3,$4,$5,$6,$7,$8,TRUE,NULL,NULL,$9)`,
          [
            costId,
            id,
            periodId,
            fields.category,
            fields.amountSatang,
            period.start_at,
            period.end_at,
            fields.note,
            now,
          ],
        );
        await db.pool.query(
          "UPDATE billing_periods SET costs_complete=FALSE WHERE id=$1",
          [periodId],
        );
      };
      const completeCosts = (periodId: string) =>
        db.pool.query(
          "UPDATE billing_periods SET costs_complete=TRUE WHERE id=$1",
          [periodId],
        );
      // Native `approve` owns the invoice freeze, cost attachment and `issue`
      // outbox. Seed exactly that native state so this oracle exercises only
      // the retained jobs/Stripe drain against real Sandbox.
      const approve = async (periodId: string, at: number) => {
        const current = await view();
        const period = current.periods.find((row) => row.id === periodId)!;
        const previous = current.invoices.find(
          (row) =>
            row.period_id === periodId &&
            row.kind === "main" &&
            row.status !== "void",
        );
        const costs = current.costs
          .filter((row) => row.period_id === periodId && !row.invoice_id)
          .sort((a, b) => (a.id < b.id ? -1 : 1));
        const lines: InvoiceLine[] = costs.map((cost) => ({
          description: `${cost.simulated ? "[Simulated] " : ""}${cost.category}: ${cost.note}`,
          amount: Number(cost.amount_satang),
          start: Number(cost.service_start),
          end: Number(cost.service_end),
          costId: cost.id,
          simulated: cost.simulated,
        }));
        if (!previous)
          lines.unshift({
            description: `Sandbox ${period.plan_id} · revision ${period.plan_revision}`,
            amount: Number(period.support_satang),
            start: Number(period.start_at),
            end: Number(period.end_at),
          });
        const total = lines.reduce((sum, line) => sum + line.amount, 0);
        const invoiceId = `invoice_${crypto.randomUUID()}`;
        await db.pool.query(
          `INSERT INTO billing_invoices
            (id,holding_id,period_id,kind,total_satang,lines_json,payment_mode,created_at,
             approved_by,original_invoice_id,updated_at)
           VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$8)`,
          [
            invoiceId,
            id,
            periodId,
            previous ? "adjustment" : "main",
            total,
            JSON.stringify(lines),
            current.account!.payment_mode,
            at,
            "fixture-admin",
            previous?.id ?? null,
          ],
        );
        for (const cost of costs)
          await db.pool.query(
            "UPDATE billing_costs SET invoice_id=$2 WHERE id=$1 AND holding_id=$3",
            [cost.id, invoiceId, id],
          );
        await db.pool.query(
          "INSERT INTO billing_jobs(id,holding_id,invoice_id,kind,next_at,created_at) VALUES($1,$2,$3,'issue',$4,$4)",
          [`issue-${invoiceId}`, id, invoiceId, at],
        );
      };
      await cmd("enroll", { planId: "standard" });
      let v = await view();
      const card = testObject(
        await stripe(
          "payment_methods",
          { type: "card", "card[token]": "tok_visa" },
          `fixture-card-${id}`,
        ),
      );
      await stripe(
        `payment_methods/${card.id}/attach`,
        { customer: v.account!.customer_id! },
        `fixture-attach-${id}`,
      );
      if (mode === "auto")
        await cmd("payment-mode", { paymentMode: "auto", consent: true });
      const p = v.periods[0];
      for (const [category, amountSatang] of [
        ["hosting", 30000],
        ["storage", 5000],
        ["provider_ai", 7500],
      ] as const)
        await seedCost(p.id, {
          category,
          amountSatang,
          note: "จำลอง — isolated acceptance test",
        });
      await completeCosts(p.id);
      await approve(p.id, p.end_at + 2);
      await drainBilling({ ...env }, p.end_at + 3, stripe);
      v = await view();
      expect(v.jobs, JSON.stringify(v.jobs)).toHaveLength(0);
      const invoice = v.invoices[0];
      expect(invoice.total_satang).toBe(92500);
      expect(invoice.status).toBe("open");
      const before = testObject(await stripe(`invoices/${invoice.stripe_id}`));
      expect(before.collection_method).toBe(
        mode === "auto" ? "charge_automatically" : "send_invoice",
      );
      expect(before.auto_advance).toBe(mode === "auto");
      if (mode === "manual")
        expect(
          Number(before.due_date) -
            Number(
              (before.status_transitions as { finalized_at: number })
                .finalized_at,
            ),
        ).toBe(7 * 86400);
      if (mode === "manual") {
        const declined = testObject(
          await stripe(
            "payment_methods",
            { type: "card", "card[token]": "tok_chargeCustomerFail" },
            `fixture-decline-${id}`,
          ),
        );
        await stripe(
          `payment_methods/${declined.id}/attach`,
          { customer: v.account!.customer_id! },
          `fixture-attach-decline-${id}`,
        );
        await expect(
          stripe(
            `invoices/${invoice.stripe_id}/pay`,
            { payment_method: declined.id },
            `fixture-failed-pay-${invoice.id}`,
          ),
        ).rejects.toMatchObject({ code: "STRIPE_RETRY_REQUIRED" });
        await drainBilling({ ...env }, p.end_at + 360000, stripe);
        expect((await view()).invoices[0].payment_state).toBe(
          "requires_payment_method",
        );
      }
      // Trigger the test payment without waiting for Stripe's scheduled automatic collection window.
      // This /pay call belongs only to the acceptance harness; application workers never retry card payments.
      await stripe(
        `invoices/${invoice.stripe_id}/pay`,
        { payment_method: card.id },
        `fixture-pay-${invoice.id}`,
      );
      await drainBilling({ ...env }, p.end_at + 720001, stripe);
      expect((await view()).invoices[0].status).toBe("paid");
      console.log(
        JSON.stringify({
          sandbox: true,
          mode,
          invoice: invoice.stripe_id,
          totalSatang: invoice.total_satang,
          status: "paid",
        }),
      );
    }
  },
  120000,
);
