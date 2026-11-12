import { beforeAll, it, expect } from "vitest";
import { env, applyMigrations } from "../../support/runtime-env";
import {
  billingCommand,
  billingView,
  type BillingInput,
} from "@erpx/server-billing/public/billing";
import { drainBilling } from "@erpx/server-billing/public/billing-jobs";
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
        await cmd(
          "cost",
          {
            periodId: p.id,
            category,
            amountSatang,
            serviceStart: p.start_at,
            serviceEnd: p.end_at,
            note: "จำลอง — isolated acceptance test",
            simulated: true,
          },
          now,
          true,
        );
      await cmd(
        "complete-costs",
        { periodId: p.id, complete: true },
        p.end_at + 1,
        true,
      );
      await cmd("approve", { periodId: p.id }, p.end_at + 2, true);
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
