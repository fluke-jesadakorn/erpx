import { beforeAll, it, expect } from "vitest";
import { env, applyMigrations } from "./runtime-env";
import {
  billingCommand,
  billingView,
  type BillingActor,
  type BillingInput,
} from "../src/server/billing";
import { drainBilling } from "../src/server/billing-jobs";
import { ApiError } from "../src/server/http";
import { billingMonth, monthBounds, proratedSupport } from "../src/lib/billing";
import {
  billingStripe,
  type BillingStripe,
  type StripeObject,
} from "../src/server/billing-stripe";
const db = env.CONTROL_DB;
beforeAll(() => applyMigrations(db, env.CONTROL_TEST_MIGRATIONS));
class FakeStripe {
  objects = new Map<string, StripeObject>();
  receipts = new Map<string, StripeObject>();
  calls: { path: string; fields?: Record<string, string>; key?: string }[] = [];
  loseResponse = false;
  legacy = false;
  card = true;
  request: BillingStripe = async (path, fields, key) => {
    this.calls.push({ path, fields, key });
    if (key && this.receipts.has(key))
      return structuredClone(this.receipts.get(key)!);
    let o: StripeObject;
    if (path === "customers")
      o = { id: `cus_${crypto.randomUUID()}`, livemode: false };
    else if (path.startsWith("subscriptions?"))
      o = {
        id: "list",
        data: this.legacy
          ? [{ id: "sub_old", livemode: false, status: "active" }]
          : [],
      };
    else if (path.startsWith("payment_methods?"))
      o = {
        id: "list",
        data: this.card
          ? [
              {
                id: "pm_test",
                livemode: false,
                card: {
                  brand: "visa",
                  last4: "4242",
                  exp_month: 1,
                  exp_year: 2030,
                },
              },
            ]
          : [],
      };
    else if (path.startsWith("customers/"))
      o = { id: path.split("/")[1], livemode: false };
    else if (path === "invoices") {
      o = {
        id: `in_${crypto.randomUUID()}`,
        livemode: false,
        status: "draft",
        total: 0,
        ...fields,
      };
      this.objects.set(o.id, o);
    } else if (path === "invoiceitems") {
      const invoice = this.objects.get(fields!.invoice)!;
      invoice.total = Number(invoice.total) + Number(fields!.amount);
      o = { id: `ii_${crypto.randomUUID()}`, livemode: false };
    } else if (path.startsWith("invoices/")) {
      const id = path.split("/")[1].split("?")[0];
      o = this.objects.get(id)!;
      if (!o) throw new Error("MISSING_FAKE_INVOICE");
      if (path.endsWith("/finalize")) {
        o.status = "open";
        o.hosted_invoice_url = "https://invoice.stripe.com/i/test";
        o.invoice_pdf = "https://pay.stripe.com/invoice/test/pdf";
        o.due_date = 2000000000;
      } else if (path.endsWith("/void")) o.status = "void";
      else if (fields) Object.assign(o, fields);
    } else throw new Error(`Unexpected Stripe path ${path}`);
    if (key) this.receipts.set(key, structuredClone(o));
    if (this.loseResponse && path === "invoiceitems") {
      this.loseResponse = false;
      throw new Error("NETWORK_TIMEOUT");
    }
    return structuredClone(o);
  };
}
async function fixture(now = Date.parse("2026-09-01T00:00:00+07:00")) {
  const id = `bill_${crypto.randomUUID()}`,
    stripe = new FakeStripe();
  await db.pool.query(
    "INSERT INTO holding_bindings(holding_id,slug,display_name,write_binding,read_binding,files_binding,status) VALUES($1,$1,'Billing test',$2,$3,$4,'active')",
    [id, `W${id}`, `R${id}`, `F${id}`],
  );
  const owner: BillingActor = {
      userId: "owner",
      email: "billing@example.test",
      platform: false,
      holdingId: id,
    },
    admin: BillingActor = {
      userId: "admin",
      email: "admin@example.test",
      platform: true,
    };
  const get = () => billingView(db, id, now);
  const command = async (
    action: BillingInput["action"],
    fields: Partial<BillingInput> = {},
    at = now,
    as = owner,
    key = crypto.randomUUID(),
  ) =>
    billingCommand(
      db,
      as,
      id,
      {
        action,
        reason: "Sandbox fixture",
        version: (await get()).account?.version ?? 0,
        ...fields,
      },
      key,
      stripe.request,
      "https://erpx.example.test/holdings/test/settings?section=usage",
      at,
    );
  const drain = (at = now, promptpaySandbox = false) =>
    drainBilling(
      { ...env, ERPX_BILLING_PROMPTPAY_SANDBOX: String(promptpaySandbox) },
      at,
      stripe.request,
    );
  return { id, stripe, owner, admin, command, get, drain, now };
}
it("rolls back the approved invoice, cost links, issue job, account version and audit after a final receipt fault, then retries and replays once", async () => {
  const f = await fixture(); await f.command("enroll", { planId: "standard" });
  const period = (await f.get()).periods[0];
  await f.command("cost", { periodId: period.id, category: "hosting", amountSatang: 1000, serviceStart: period.start_at, serviceEnd: period.end_at, note: "Retained actual hosting source", simulated: true }, period.end_at + 1, f.admin);
  await f.command("complete-costs", { periodId: period.id, complete: true }, period.end_at + 1, f.admin);
  const snapshot = async () => Object.fromEntries(await Promise.all(["billing_accounts", "billing_enrollments", "billing_periods", "billing_costs", "billing_invoices", "billing_jobs", "billing_audit"].map(async table => [table, (await db.pool.query(`SELECT * FROM ${table} WHERE holding_id=$1 ORDER BY ${table === "billing_accounts" ? "holding_id" : "id"}`, [f.id])).rows])));
  const before = await snapshot(), command = crypto.randomUUID(), input: BillingInput = { action: "approve", version: (await f.get()).account!.version, periodId: period.id, reason: "Independent reviewed actual cost approval" }, at = period.end_at + 2;
  const approve = () => billingCommand(db, f.admin, f.id, input, command, f.stripe.request, "https://fixture.example/usage", at), fault = `billing_receipt_fault_${crypto.randomUUID().replaceAll("-", "")}`;
  await db.pool.query(`CREATE FUNCTION ${fault}() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN RAISE EXCEPTION 'BILLING_FINAL_RECEIPT_FAULT' USING ERRCODE='23514';END;$$;CREATE TRIGGER ${fault} AFTER INSERT ON billing_receipts FOR EACH ROW WHEN (NEW.id='${command}') EXECUTE FUNCTION ${fault}();`);
  try { await expect(approve()).rejects.toThrow(); expect(await snapshot()).toEqual(before); expect((await db.pool.query("SELECT id FROM billing_receipts WHERE id=$1", [command])).rows).toEqual([]); }
  finally { await db.pool.query(`DROP TRIGGER ${fault} ON billing_receipts;DROP FUNCTION ${fault}();`); }
  const approved = await approve(); expect(await approve()).toEqual(approved);
  expect((await db.pool.query("SELECT id FROM billing_receipts WHERE id=$1", [command])).rowCount).toBe(1);
  expect((await db.pool.query("SELECT id FROM billing_invoices WHERE holding_id=$1", [f.id])).rowCount).toBe(1);
  expect((await db.pool.query("SELECT id FROM billing_jobs WHERE holding_id=$1 AND kind='issue'", [f.id])).rowCount).toBe(1);
  expect((await db.pool.query("SELECT id FROM billing_audit WHERE holding_id=$1 AND action='approve'", [f.id])).rowCount).toBe(1);
  expect((await db.pool.query("SELECT invoice_id FROM billing_costs WHERE holding_id=$1", [f.id])).rows[0].invoice_id).not.toBeNull();
});
it("uses Bangkok calendar days, including leap February and same-day starts", () => {
  expect(billingMonth(Date.parse("2026-08-31T17:00:00Z"))).toBe("2026-09");
  expect(
    proratedSupport(50000, Date.parse("2026-09-16T23:59:00+07:00"), "2026-09"),
  ).toBe(25000);
  expect(
    proratedSupport(50000, Date.parse("2024-02-29T23:59:00+07:00"), "2024-02"),
  ).toBe(1724);
  expect(monthBounds("2026-09").end - monthBounds("2026-09").start).toBe(
    30 * 86400000,
  );
});
it("offers PromptPay only on opt-in manual Sandbox invoices", async () => {
  const f = await fixture();
  await f.command("enroll", { planId: "standard" });
  const period = (await f.get()).periods[0];
  await f.command(
    "complete-costs",
    { periodId: period.id, complete: true },
    period.end_at + 1,
    f.admin,
  );
  await f.command("approve", { periodId: period.id }, period.end_at + 2, f.admin);
  await f.drain(period.end_at + 3, true);
  const create = f.stripe.calls.find((call) => call.path === "invoices");
  expect(create?.fields?.collection_method).toBe("send_invoice");
  expect(create?.fields?.["payment_settings[payment_method_types][0]"]).toBe("card");
  expect(create?.fields?.["payment_settings[payment_method_types][1]"]).toBe("promptpay");
  const automatic = await fixture();
  await automatic.command("enroll", { planId: "standard" });
  await automatic.command("payment-mode", { paymentMode: "auto", consent: true });
  const autoPeriod = (await automatic.get()).periods[0];
  await automatic.command(
    "complete-costs",
    { periodId: autoPeriod.id, complete: true },
    autoPeriod.end_at + 1,
    automatic.admin,
  );
  await automatic.command(
    "approve",
    { periodId: autoPeriod.id },
    autoPeriod.end_at + 2,
    automatic.admin,
  );
  await automatic.drain(autoPeriod.end_at + 3, true);
  const autoCreate = automatic.stripe.calls.find((call) => call.path === "invoices");
  expect(autoCreate?.fields?.collection_method).toBe("charge_automatically");
  expect(autoCreate?.fields?.["payment_settings[payment_method_types][0]"]).toBe("card");
  expect(autoCreate?.fields?.["payment_settings[payment_method_types][1]"]).toBeUndefined();
});
it("issues exactly one 925 baht reviewed invoice across concurrent approvals and an interrupted Stripe response", async () => {
  const f = await fixture();
  await f.command("enroll", { planId: "standard" });
  let v = await f.get();
  const p = v.periods[0];
  for (const [category, amountSatang] of [
    ["hosting", 30000],
    ["storage", 5000],
    ["provider_ai", 7500],
  ] as const)
    await f.command(
      "cost",
      {
        periodId: p.id,
        category,
        amountSatang,
        serviceStart: p.start_at,
        serviceEnd: p.end_at,
        note: "จำลอง",
        simulated: true,
      },
      f.now,
      f.admin,
    );
  await expect(
    f.command("approve", { periodId: p.id }, p.end_at + 1, f.admin),
  ).rejects.toMatchObject({ code: "COSTS_INCOMPLETE" });
  await f.command(
    "complete-costs",
    { periodId: p.id, complete: true },
    p.end_at + 1,
    f.admin,
  );
  const version = (await f.get()).account!.version,
    key = crypto.randomUUID(),
    input: BillingInput = {
      action: "approve",
      reason: "Approved fixture",
      periodId: p.id,
      version,
    };
  const run = () =>
    billingCommand(
      db,
      f.admin,
      f.id,
      input,
      key,
      f.stripe.request,
      "https://example.test",
      p.end_at + 1,
    );
  await Promise.all([run(), run()]);
  f.stripe.loseResponse = true;
  await f.drain(p.end_at + 2);
  await f.drain(p.end_at + 60002);
  v = await f.get();
  expect(v.invoices).toHaveLength(1);
  expect(v.invoices[0].total_satang).toBe(92500);
  expect(v.invoices[0].status).toBe("open");
  expect(f.stripe.objects.size).toBe(1);
  expect([...f.stripe.objects.values()][0].total).toBe(92500);
  expect(
    f.stripe.calls.find((c) => c.path === "invoices")!.fields!.auto_advance,
  ).toBe("false");
  expect(
    f.stripe.calls
      .filter((c) => c.path === "invoiceitems")
      .every((c) => c.fields?.invoice === v.invoices[0].stripe_id),
  ).toBe(true);
  await expect(
    f.command("delete-cost", { costId: v.costs[0].id }, p.end_at + 3, f.admin),
  ).rejects.toMatchObject({ code: "COST_FROZEN" });
  await f.command(
    "void",
    { invoiceId: v.invoices[0].id },
    p.end_at + 70000,
    f.admin,
  );
  await f.drain(p.end_at + 70001);
  expect((await f.get()).invoices[0].status).toBe("void");
  await f.command(
    "complete-costs",
    { periodId: p.id, complete: true },
    p.end_at + 79999,
    f.admin,
  );
  await f.command("approve", { periodId: p.id }, p.end_at + 80000, f.admin);
  await f.drain(p.end_at + 80001);
  expect(
    (await f.get()).invoices.filter((i) => i.status !== "void"),
  ).toHaveLength(1);
});
it("applies changes only at month end, allows withdrawal, ends Hosting without starting another Support period, and bills late costs", async () => {
  const f = await fixture(Date.parse("2026-09-16T23:59:00+07:00"));
  await f.command("enroll", { planId: "standard" });
  const p = (await f.get()).periods[0];
  expect(p.support_satang).toBe(25000);
  await f.command("change-plan", { planId: "priority" });
  await f.command("withdraw");
  expect((await f.get()).enrollment!.pending_plan).toBeNull();
  await f.command("change-plan", { planId: "infrastructure" });
  await f.drain(p.end_at + 1);
  let v = await f.get();
  expect(v.enrollment!.plan_id).toBe("infrastructure");
  expect(v.periods[0].support_satang).toBe(0);
  const october = v.periods[0];
  await f.command("cancel-hosted", {}, p.end_at + 2);
  await f.drain(october.end_at + 1);
  v = await f.get();
  expect(v.enrollment!.ended_at).toBe(october.end_at);
  expect(v.periods).toHaveLength(2);
  await f.command(
    "cost",
    {
      periodId: p.id,
      category: "storage",
      amountSatang: 5000,
      serviceStart: p.start_at,
      serviceEnd: p.end_at,
      note: "Late provider cost",
      simulated: true,
    },
    october.end_at + 10,
    f.admin,
  );
  await f.command(
    "complete-costs",
    { periodId: p.id, complete: true },
    october.end_at + 11,
    f.admin,
  );
  await f.command("approve", { periodId: p.id }, october.end_at + 12, f.admin);
  await f.drain(october.end_at + 13);
  expect((await f.get()).invoices[0].total_satang).toBe(30000);
  expect((await f.get()).periods).toHaveLength(2);
});
it("holds a Sandbox credential failure for explicit repair and resumes the original reviewed invoice once", async () => {
  const f = await fixture();
  await f.command("enroll", { planId: "standard" });
  const period = (await f.get()).periods[0];
  await f.command("complete-costs", { periodId: period.id, complete: true }, period.end_at + 1, f.admin);
  await f.command("approve", { periodId: period.id }, period.end_at + 2, f.admin);
  let unavailable = true;
  const stripe: BillingStripe = async (path, fields, key) => {
    if (unavailable && path === "invoices")
      throw new ApiError(503, "STRIPE_AUTHENTICATION_REQUIRED", "Sandbox configuration needs repair.");
    return f.stripe.request(path, fields, key);
  };
  await drainBilling({ ...env }, period.end_at + 3, stripe);
  const failed = (await db.pool.query("SELECT * FROM billing_jobs WHERE holding_id=$1", [f.id])).rows[0];
  expect(failed).toMatchObject({ state: "attention", attempts: 1, last_error: "STRIPE_AUTHENTICATION_REQUIRED" });
  expect(JSON.parse(failed.progress_json)).toEqual({ operationAt: period.end_at + 3 });
  expect((await f.get()).invoices).toHaveLength(1);
  expect((await f.get()).invoices[0].stripe_id).toBeNull();
  await drainBilling({ ...env }, period.end_at + 60003, stripe);
  const held = (await db.pool.query("SELECT * FROM billing_jobs WHERE id=$1", [failed.id])).rows[0];
  expect(held.attempts).toBe(1);
  expect(held.progress_json).toBe(failed.progress_json);
  unavailable = false;
  const retryKey = crypto.randomUUID(), retryVersion = (await f.get()).account!.version;
  const input: BillingInput = { action: "retry", reason: "Sandbox configuration repaired", version: retryVersion, jobId: failed.id };
  const retry = () => billingCommand(db, f.admin, f.id, input, retryKey, stripe, "https://example.test", period.end_at + 60004);
  expect(await retry()).toEqual(await retry());
  await drainBilling({ ...env }, period.end_at + 60005, stripe);
  const done = (await db.pool.query("SELECT * FROM billing_jobs WHERE id=$1", [failed.id])).rows[0];
  expect(done).toMatchObject({ state: "done", attempts: 1, last_error: null });
  expect(JSON.parse(done.progress_json).operationAt).toBe(period.end_at + 3);
  const final = await f.get();
  expect(final.invoices).toHaveLength(1);
  expect(final.invoices[0]).toMatchObject({ id: failed.invoice_id, status: "open", total_satang: 50000 });
  expect(f.stripe.calls.filter(call => call.path === "invoices")).toHaveLength(1);
  expect(f.stripe.calls.find(call => call.path === "invoices")?.key).toBe(`erpx-invoice-${failed.invoice_id}`);
  expect(final.audit.filter(event => event.action === "retry")).toHaveLength(1);
});
it("requires explicit consent and a card, handles withdrawal before issue, rejects cross-holding and legacy active enrollment", async () => {
  const f = await fixture();
  f.stripe.legacy = true;
  await expect(
    f.command("enroll", { planId: "standard" }),
  ).rejects.toMatchObject({ code: "LEGACY_ACTIVE" });
  f.stripe.legacy = false;
  await f.command("enroll", { planId: "standard" });
  await expect(
    f.command("payment-mode", { paymentMode: "auto" }),
  ).rejects.toMatchObject({ code: "CONSENT_REQUIRED" });
  f.stripe.card = false;
  await expect(
    f.command("payment-mode", { paymentMode: "auto", consent: true }),
  ).rejects.toMatchObject({ code: "CARD_REQUIRED" });
  f.stripe.card = true;
  await f.command("payment-mode", { paymentMode: "auto", consent: true });
  const p = (await f.get()).periods[0];
  await f.command(
    "complete-costs",
    { periodId: p.id, complete: true },
    p.end_at + 1,
    f.admin,
  );
  await f.command("approve", { periodId: p.id }, p.end_at + 2, f.admin);
  await f.command("payment-mode", { paymentMode: "manual" }, p.end_at + 3);
  await f.drain(p.end_at + 4);
  expect(f.stripe.calls.some((c) => c.fields?.auto_advance === "true")).toBe(
    false,
  );
  expect([...f.stripe.objects.values()][0].collection_method).toBe(
    "send_invoice",
  );
  await expect(
    f.command("cancel-hosted", {}, f.now, { ...f.owner, holdingId: "other" }),
  ).rejects.toMatchObject({ status: 403 });
  await expect(f.command("approve", { periodId: p.id })).rejects.toMatchObject({
    status: 403,
  });
  await expect(
    billingStripe({ STRIPE_SECRET_KEY: "sk_live_rejected" })("customers"),
  ).rejects.toMatchObject({ code: "SANDBOX_REQUIRED" });
});
it("reconciles payment failures, authentication and paid state from current Stripe, never from event snapshots", async () => {
  const f = await fixture();
  await f.command("enroll", { planId: "standard" });
  await f.command("payment-mode", { paymentMode: "auto", consent: true });
  const p = (await f.get()).periods[0];
  await f.command(
    "complete-costs",
    { periodId: p.id, complete: true },
    p.end_at + 1,
    f.admin,
  );
  await f.command("approve", { periodId: p.id }, p.end_at + 2, f.admin);
  await f.drain(p.end_at + 3);
  expect(f.stripe.calls.some((c) => c.fields?.auto_advance === "true")).toBe(
    true,
  );
  const v = await f.get(),
    invoice = v.invoices[0],
    obj = f.stripe.objects.get(invoice.stripe_id!)!;
  for (const [index, status] of [
    "requires_payment_method",
    "requires_action",
    "succeeded",
  ].entries()) {
    obj.payment_intent = { id: "pi_test", status, livemode: false };
    if (status === "succeeded") obj.status = "paid";
    const id = crypto.randomUUID();
    await db.pool.query(
      "INSERT INTO stripe_inbox(id,event_type,created_at,payload_json) VALUES($1,'invoice.payment_failed',1,$2)",
      [
        id,
        JSON.stringify({
          id,
          livemode: false,
          type: "invoice.payment_failed",
          created: 1,
          data: {
            object: {
              id: invoice.stripe_id,
              customer: v.account!.customer_id,
              status: "open",
            },
          },
        }),
      ],
    );
    await f.drain(p.end_at + 10 + index);
    expect((await f.get()).invoices[0].payment_state).toBe(status);
  }
  expect((await f.get()).invoices[0].status).toBe("paid");
  await expect(
    f.command("void", { invoiceId: invoice.id }, p.end_at + 20, f.admin),
  ).rejects.toMatchObject({ code: "VOID_UNAVAILABLE" });
});
it("keeps historical plan revisions immutable and uses new prices only for the next period", async () => {
  const f = await fixture();
  await f.command("enroll", { planId: "priority" });
  const first = (await f.get()).periods[0];
  await f.command(
    "revise-plan",
    { planId: "priority", supportSatang: 180000, planRevision: 1 },
    f.now,
    f.admin,
  );
  expect((await f.get()).periods[0].support_satang).toBe(150000);
  await f.drain(first.end_at + 1);
  const second = (await f.get()).periods[0];
  expect(second.support_satang).toBe(180000);
  expect(second.plan_revision).toBe(2);
  await expect(
    f.command(
      "revise-plan",
      { planId: "infrastructure", supportSatang: 100 },
      f.now,
      f.admin,
    ),
  ).rejects.toMatchObject({ code: "INFRASTRUCTURE_SUPPORT_ZERO" });
  const stale: BillingInput = {
    action: "cancel-hosted",
    reason: "Stale browser tab",
    version: 0,
  };
  await expect(
    billingCommand(
      db,
      f.owner,
      f.id,
      stale,
      crypto.randomUUID(),
      f.stripe.request,
      "https://example.test",
      f.now,
    ),
  ).rejects.toMatchObject({ code: "VERSION_CONFLICT" });
});
it("bills late adjustments without duplicating Support and rejects references across Holdings", async () => {
  const f = await fixture();
  await f.command("enroll", { planId: "standard" });
  const p = (await f.get()).periods[0];
  await f.command(
    "complete-costs",
    { periodId: p.id, complete: true },
    p.end_at + 1,
    f.admin,
  );
  await f.command("approve", { periodId: p.id }, p.end_at + 2, f.admin);
  await f.drain(p.end_at + 3);
  let v = await f.get();
  const inv = v.invoices[0];
  f.stripe.objects.get(inv.stripe_id!)!.status = "paid";
  await f.drain(p.end_at + 400000);
  await expect(
    f.command(
      "cost",
      {
        periodId: p.id,
        category: "adjustment",
        amountSatang: 5000,
        serviceStart: p.start_at,
        serviceEnd: p.end_at,
        note: "Wrong reference",
        originalInvoiceId: "other-holding-invoice",
      },
      p.end_at + 400001,
      f.admin,
    ),
  ).rejects.toMatchObject({ code: "INVALID_ORIGINAL" });
  await f.command(
    "cost",
    {
      periodId: p.id,
      category: "adjustment",
      amountSatang: 5000,
      serviceStart: p.start_at,
      serviceEnd: p.end_at,
      note: "Late adjustment",
      originalInvoiceId: inv.id,
      simulated: true,
    },
    p.end_at + 400002,
    f.admin,
  );
  await f.command(
    "complete-costs",
    { periodId: p.id, complete: true },
    p.end_at + 400003,
    f.admin,
  );
  await f.command("approve", { periodId: p.id }, p.end_at + 400004, f.admin);
  await f.drain(p.end_at + 400005);
  v = await f.get();
  expect(v.invoices[0].total_satang).toBe(5000);
  expect(v.invoices[0].kind).toBe("adjustment");
  expect(v.invoices[0].original_invoice_id).toBe(inv.id);
  expect(v.invoices.find((i) => i.id === inv.id)!.status).toBe("paid");
});
