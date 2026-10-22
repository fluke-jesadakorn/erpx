import { beforeAll, beforeEach, expect, it } from "vitest";
import { applyMigrations, env } from "../../support/runtime-env";
import { syncBillingTestFinance } from "@erpx/server-commerce/public/billing-test-finance";
import { mirrorInvoice, type BillingJobEnv } from "@erpx/server-billing/public/billing-jobs";
import type { StripeObject } from "@erpx/server-billing/public/billing-stripe";

const registry = {
  ERPX_RESOURCE_SLOTS: JSON.stringify([{ id: "h003", write: "H001_DB", read: "H001_READ_DB", files: "H001_FILES", internal: true }]),
  H001_DB: env.H001_DB,
  H001_READ_DB: env.H001_READ_DB,
};
const local = { id: "bill-target-1", holding_id: "h001", stripe_id: "in_test_1", total_satang: 149000 };
function invoice(status: string, paid = 0): StripeObject {
  return { id: local.stripe_id, livemode: false, status, currency: "thb",
    total: local.total_satang, amount_paid: paid,
    metadata: { erpx_invoice_id: local.id, holding_id: local.holding_id } };
}
const credit = (id: string, pre: number, post: number): StripeObject => ({
  id, livemode: false, invoice: local.stripe_id, status: "issued",
  pre_payment_amount: pre, post_payment_amount: post,
});

beforeAll(async () => {
  await applyMigrations(env.H001_DB, env.STAFF_TEST_MIGRATIONS);
  await applyMigrations(env.CONTROL_DB, env.CONTROL_TEST_MIGRATIONS);
});
beforeEach(async () => {
  await env.H001_DB.prepare("DELETE FROM billing_test_finance_lines").run();
  await env.H001_DB.prepare("DELETE FROM billing_test_finance_entries").run();
  await env.H001_DB.prepare("DELETE FROM billing_test_finance_documents").run();
});

it("posts one receivable and one receipt despite repeated Stripe events", async () => {
  await syncBillingTestFinance(registry, local, invoice("open"), []);
  const paid = await syncBillingTestFinance(registry, local, invoice("paid",149000), []);
  await syncBillingTestFinance(registry, local, invoice("paid",149000), []);
  expect(paid).toMatchObject({ invoiceSatang: 149000, receivableSatang: 0,
    receivedSatang: 149000, reversedSatang: 0 });
  const rows = await env.H001_DB.prepare(`SELECT entry_kind,amount_satang FROM billing_test_finance_entries
    ORDER BY entry_kind`).all<{entry_kind:string;amount_satang:number}>();
  expect(rows.results).toEqual([{ entry_kind: "issued", amount_satang: 149000 },
    { entry_kind: "receipt", amount_satang: 149000 }]);
  const balance = await env.H001_DB.prepare(`SELECT source_key,
    SUM(debit_satang) debit,SUM(credit_satang) credit
    FROM billing_test_finance_lines GROUP BY source_key`).all<{debit:number;credit:number}>();
  expect(balance.results.every(row => row.debit === row.credit)).toBe(true);
});

it("reverses an unpaid void without mutating issued history", async () => {
  await syncBillingTestFinance(registry, local, invoice("open"), []);
  const voided = await syncBillingTestFinance(registry, local, invoice("void"), []);
  expect(voided).toMatchObject({ receivableSatang: 0, reversedSatang: 149000 });
  const kinds = await env.H001_DB.prepare("SELECT entry_kind FROM billing_test_finance_entries ORDER BY entry_kind")
    .all<{entry_kind:string}>();
  expect(kinds.results.map(row => row.entry_kind)).toEqual(["issued","void"]);
});

it("posts pre-payment credits against receivable and post-payment credits to customer credit", async () => {
  const credits = [credit("cn_before",10000,0),credit("cn_after",0,5000)];
  const paid = await syncBillingTestFinance(registry, local, invoice("paid",139000), credits);
  expect(paid).toMatchObject({ receivableSatang: 0, receivedSatang: 139000,
    creditedSatang: 15000 });
  const liability = await env.H001_DB.prepare(`SELECT credit_satang FROM billing_test_finance_lines
    WHERE account_code='customer_credit'`).first<{credit_satang:number}>();
  expect(liability?.credit_satang).toBe(5000);
});

it("reverses a voided credit note with an auditable opposite entry", async () => {
  await syncBillingTestFinance(registry,local,invoice("open"),[credit("cn_reversed",10000,0)]);
  const restored = await syncBillingTestFinance(registry,local,invoice("open"),
    [{ ...credit("cn_reversed",10000,0), status: "void" }]);
  expect(restored).toMatchObject({ receivableSatang: 149000, creditedSatang: 0 });
  const entries = await env.H001_DB.prepare("SELECT entry_kind FROM billing_test_finance_entries WHERE source_key LIKE 'cn_reversed:%' ORDER BY entry_kind")
    .all<{entry_kind:string}>();
  expect(entries.results.map(row => row.entry_kind)).toEqual(["credit","credit_reversal"]);
});

it("rejects live or mismatched Stripe evidence before touching the test ledger", async () => {
  await expect(syncBillingTestFinance(registry,local,{ ...invoice("paid",149000), livemode:true },[]))
    .rejects.toThrow("TEST_FINANCE_STRIPE_SCOPE_MISMATCH");
  await expect(syncBillingTestFinance(registry,local,{ ...invoice("paid",149000), total:148999 },[]))
    .rejects.toThrow("TEST_FINANCE_AMOUNT_MISMATCH");
  const rows = await env.H001_DB.prepare("SELECT 1 FROM billing_test_finance_documents").all();
  expect(rows.results).toHaveLength(0);
});

it("mirrors a confirmed Stripe target invoice into Control reconciliation and ERPX test ledger", async () => {
  const id = `bill-${crypto.randomUUID()}`;
  const holding = `h${crypto.randomUUID().replaceAll("-","").slice(0,12)}`;
  const stripeId = `in_${crypto.randomUUID().replaceAll("-","")}`;
  const now = Date.UTC(2026,9,1);
  await env.CONTROL_DB.prepare(`INSERT INTO holding_bindings
    (holding_id,slug,display_name,write_binding,read_binding,files_binding,status)
    VALUES(?,?,?,?,?,?,'active')`)
    .bind(holding,holding,"Test Holding","H001_DB","H001_READ_DB","H001_FILES").run();
  await env.CONTROL_DB.prepare(`INSERT INTO billing_accounts(holding_id,created_at) VALUES(?,?)`)
    .bind(holding,now).run();
  const enrollment = `en-${id}`, period = `period-${id}`;
  await env.CONTROL_DB.prepare(`INSERT INTO billing_enrollments
    (id,holding_id,plan_id,started_at) VALUES(?,?,?,?)`)
    .bind(enrollment,holding,"infrastructure",now-30*86400000).run();
  await env.CONTROL_DB.prepare(`INSERT INTO billing_periods
    (id,holding_id,enrollment_id,month,start_at,end_at,plan_id,plan_revision,support_satang)
    VALUES(?,?,?,'2026-09',?,?, 'infrastructure',1,0)`)
    .bind(period,holding,enrollment,now-30*86400000,now).run();
  await env.CONTROL_DB.prepare(`INSERT INTO billing_invoices
    (id,holding_id,period_id,kind,total_satang,lines_json,payment_mode,stripe_id,
     created_at,approved_by,updated_at,rating_model)
    VALUES(?,?,?,'main',149000,'[]','manual',?,?,?,?,'target')`)
    .bind(id,holding,period,stripeId,now,"admin",now).run();
  const confirmed: StripeObject = { id: stripeId,livemode:false,status:"paid",currency:"thb",
    total:149000,amount_paid:149000,metadata:{erpx_invoice_id:id,holding_id:holding} };
  const stripe = async (path: string) => path.startsWith("credit_notes?")
    ? { id:"list",data:[],has_more:false } as StripeObject : confirmed;
  const client = await env.CONTROL_DB.pool.connect();
  try {
    const jobEnv = { ...registry, CONTROL_DB:env.CONTROL_DB } as unknown as BillingJobEnv;
    await mirrorInvoice(client,id,stripe,now,jobEnv);
    await mirrorInvoice(client,id,stripe,now+1,jobEnv);
  } finally { client.release(); }
  const projection = await env.CONTROL_DB.prepare(`SELECT invoice_satang,receivable_satang,
    received_satang,stripe_status FROM billing_test_finance_projection WHERE billing_invoice_id=?`)
    .bind(id).first<Record<string,unknown>>();
  expect(projection).toMatchObject({ invoice_satang:149000,receivable_satang:0,
    received_satang:149000,stripe_status:"paid" });
  const entries = await env.H001_DB.prepare(`SELECT count(*)::int count FROM billing_test_finance_entries
    WHERE stripe_invoice_id=?`).bind(stripeId).first<{count:number}>();
  expect(entries?.count).toBe(2);
});
