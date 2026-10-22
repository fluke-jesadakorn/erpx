import { beforeAll, beforeEach, describe, expect, it, vi } from "vitest";
import { applyMigrations, env } from "../../support/runtime-env";
import { expenseDb as db, expenseFixture, key } from "../../support/helpers/expense-fixture";
import { createInvoice, createParty, getInvoice, invoiceSchema, postInvoice, voidDraftInvoice } from "@erpx/server-finance/public/finance";
import { confirmManualRate } from "@erpx/server-finance/public/company-money";
import { assignFlowPrimary, resolveFlow } from "@erpx/server-workflow/public/flow-routing";
import { documentPostingPreview } from "@erpx/server-finance/public/document-posting-preview";

vi.mock("server-only", () => ({}));
vi.mock("@erpx/server-runtime/adapters/runtime", () => ({ holdingResources: async () => ({ write: env.H001_DB, files: env.H001_FILES }), runtimeEnv: async () => env }));
let f: Awaited<ReturnType<typeof expenseFixture>>;
beforeAll(async () => {
  await applyMigrations(db, env.STAFF_TEST_MIGRATIONS);
});
beforeEach(async () => { f = await expenseFixture(); });

async function input() {
  const party = await createParty(f.reviewer, { kind: "supplier", code: key(), name: "บริษัทบริการสมมติ", currency: "THB", paymentTermsDays: 30, creditLimit: "0" }, key());
  return invoiceSchema.parse({ partyId: party.id, kind: "payable", invoiceNumber: key(), invoiceDate: "2026-09-10", dueDate: "2026-10-10", currency: "THB", subtotal: "2500.03", tax: "175.00", debitAccountId: f.accounts.expense, creditAccountId: f.accounts.payable, taxAccountId: f.accounts.tax, lines: [
    { description: "ค่าบริการติดตั้ง", quantity: "1.5", unitPrice: "1000.01", taxBps: 700, accountId: f.accounts.expense },
    { description: "ค่าบริการตรวจสอบ", quantity: "2", unitPrice: "500.01", taxBps: 700, accountId: f.accounts.expense },
  ] });
}

describe("native multi-line Finance documents", () => {
  it("retains exact decimal line revisions source snapshots and a balanced independently posted supplier document after reload", async () => {
    const original = { ...await input(), subtotal: "2500.04" }, command = key();
    const draft = await createInvoice(f.reviewer, original, command);
    expect(await createInvoice(f.reviewer, original, command)).toEqual(draft);
    const first = await getInvoice(f.manager, String(draft.id));
    expect(await documentPostingPreview(f.manager, "invoice", String(draft.id), db)).toMatchObject({ state: "ready", posted: false, debit: 267504, credit: 267504 });
    expect(await documentPostingPreview({ ...f.manager, companyId: "other" }, "invoice", String(draft.id), db)).toBeNull();
    expect(first.lines.map(line => [Number(line.quantity_millis), Number(line.unit_price_minor), Number(line.subtotal_minor), Number(line.tax_minor)])).toEqual([[1500, 100001, 150002, 10500], [2000, 50001, 100002, 7000]]);
    const frozen = first.lines.map(line => line.source_snapshot_json);
    const edited = { ...original, expectedVersion: 1, lines: original.lines!.map(line => ({ ...line, description: `${line.description} ตรวจทาน` })) };
    await createInvoice(f.reviewer, edited, key(), String(draft.id));
    const draftMoney = await db.prepare("SELECT amount_units::text,usd_units::text,fx_rate_id FROM money_transaction_snapshots WHERE company_id=? AND source_type='finance_invoice' AND source_id=? AND amount_kind='draft:v2'").bind(f.company,draft.id).first();
    await confirmManualRate(f.manager,{currency:"THB",effectiveDate:"2026-09-10",currencyPerUsd:"40",note:"Synthetic later quote must not rewrite approved source revisions"},key());
    const flow=await resolveFlow(db,f.owner,"invoice",String(draft.id));
    await assignFlowPrimary(db,f.owner,"invoice",String(draft.id),{stepKey:"draft",userId:f.manager.userId,reason:"Independent source line reviewer",saveDefault:false,expectedVersion:flow.version,recordVersion:flow.recordVersion},key());
    const postKey=key(), posted=await postInvoice(f.manager,String(draft.id),{expectedVersion:2,periodId:f.period,entryDate:"2026-09-10"},postKey);
    expect(await postInvoice(f.manager,String(draft.id),{expectedVersion:2,periodId:f.period,entryDate:"2026-09-10"},postKey)).toEqual(posted);
    const reloaded=await getInvoice(f.reviewer,String(draft.id));
    expect(await documentPostingPreview(f.reviewer, "invoice", String(draft.id), db)).toMatchObject({ state: "ready", posted: true, debit: 267504, credit: 267504 });
    expect(reloaded.invoice).toMatchObject({status:"posted",total_minor:267504,outstanding_minor:267504,line_revision:2,version:3});
    expect(reloaded.lines.map(line=>line.description)).toEqual(edited.lines!.map(line=>line.description));
    expect(reloaded.lineHistory).toHaveLength(2);
    expect((await db.prepare("SELECT source_snapshot_json FROM finance_invoice_line_versions WHERE invoice_id=? AND source_version=1 ORDER BY ordinal").bind(draft.id).all<{source_snapshot_json:string}>()).results.map(line=>line.source_snapshot_json)).toEqual(frozen);
    expect(await db.prepare("SELECT amount_units::text,usd_units::text,fx_rate_id FROM money_transaction_snapshots WHERE company_id=? AND source_type='finance_invoice' AND source_id=? AND amount_kind='total'").bind(f.company,draft.id).first()).toEqual(draftMoney);
    const ledger=(await db.prepare("SELECT account_id,SUM(debit_minor-credit_minor)::integer balance FROM finance_journal_lines WHERE journal_id=? GROUP BY account_id").bind(posted.journalId).all<{account_id:string;balance:number}>()).results;
    expect(Object.fromEntries(ledger.map(row=>[row.account_id,row.balance]))).toEqual({[f.accounts.expense]:250004,[f.accounts.tax]:17500,[f.accounts.payable]:-267504});
    expect(await db.prepare("SELECT SUM(base_debit_minor-base_credit_minor)::integer difference FROM finance_journal_lines WHERE journal_id=?").bind(posted.journalId).first("difference")).toBe(0);
    expect(await db.prepare("SELECT COUNT(*) n FROM finance_journals WHERE source_type='invoice' AND source_id=?").bind(draft.id).first("n")).toBe(1);
  });

  it("rejects inconsistent totals cross-Company accounts stale edits self posting and changes to retained source history", async () => {
    const value = await input();
    await expect(createInvoice(f.reviewer, value, key())).rejects.toMatchObject({ code: "INVOICE_LINE_TOTAL_MISMATCH" });
    const good = { ...value, subtotal: "2500.04" }, draft = await createInvoice(f.reviewer, good, key());
    await expect(createInvoice(f.reviewer, { ...good, expectedVersion: 1, lines: undefined }, key(), String(draft.id))).rejects.toMatchObject({ code: "INVOICE_LINES_REQUIRED" });
    await createInvoice(f.reviewer, { ...good, expectedVersion: 1, lines: good.lines!.map(line => ({ ...line, description: `${line.description} ฉบับตรวจทาน` })) }, key(), String(draft.id));
    await expect(createInvoice(f.reviewer, { ...good, expectedVersion: 1 }, key(), String(draft.id))).rejects.toMatchObject({ code: "VERSION_CONFLICT" });
    await expect(postInvoice(f.reviewer, String(draft.id), { expectedVersion: 2, periodId: f.period, entryDate: "2026-09-10" }, key())).rejects.toMatchObject({ status: 403 });
    const foreign = await expenseFixture();
    await expect(createInvoice(f.reviewer, { ...good, lines: [{ ...good.lines![0], accountId: foreign.accounts.expense }, good.lines![1]] }, key())).rejects.toMatchObject({ code: "ACCOUNT_INVALID" });
    await expect(getInvoice(foreign.owner, String(draft.id))).rejects.toMatchObject({ code: "INVOICE_NOT_FOUND" });
    await expect(db.prepare("UPDATE finance_invoice_line_versions SET description='overwrite' WHERE invoice_id=?").bind(draft.id).run()).rejects.toThrow("FINANCE_INVOICE_LINE_IMMUTABLE");
    await expect(db.prepare("DELETE FROM finance_invoice_line_versions WHERE invoice_id=?").bind(draft.id).run()).rejects.toThrow("FINANCE_INVOICE_LINE_IMMUTABLE");
    await voidDraftInvoice(f.reviewer, String(draft.id), 2, key());
    const cancelled = await getInvoice(f.manager, String(draft.id));
    expect(cancelled.invoice).toMatchObject({ status: "voided", outstanding_minor: 0, version: 3, line_revision: 2 });
    expect(cancelled.lines).toHaveLength(2); expect(cancelled.lineHistory).toHaveLength(2);
    expect(await db.prepare("SELECT COUNT(*) n FROM finance_journals WHERE source_id=?").bind(draft.id).first("n")).toBe(0);
  });

  it("rejects JPY and KWD new monetary documents without reinterpreting their minor units",async()=>{
    for(const currency of ["JPY","KWD"])await expect(createParty(f.reviewer,{kind:"supplier",code:key(),name:"Unsupported legacy ledger scale",currency,paymentTermsDays:30,creditLimit:"100"},key())).rejects.toMatchObject({code:"FINANCE_CURRENCY_SCALE_UNSUPPORTED"});
    expect(await db.prepare("SELECT COUNT(*) n FROM finance_parties WHERE company_id=?").bind(f.company).first("n")).toBe(0);
  });

  it("keeps backward-compatible whole-document inputs explicit without inventing product quantities", async () => {
    const value = await input(), draft = await createInvoice(f.reviewer, { ...value, subtotal: "100", tax: "7", lines: undefined }, key());
    const detail = await getInvoice(f.manager, String(draft.id));
    expect(detail.invoice.line_mode).toBe("whole_document");
    expect(detail.lines).toMatchObject([{ origin_kind: "whole_document", quantity_millis: null, unit_price_minor: null, tax_bps: null, subtotal_minor: 10000, tax_minor: 700, total_minor: 10700 }]);
    expect(JSON.parse(detail.lines[0].source_snapshot_json)).toMatchObject({ schema: "finance.invoice.line.v1", party: { id: value.partyId, version: 1 }, netAccount: { id: f.accounts.expense, version: 1 }, controlAccount: { id: f.accounts.payable, version: 1 } });
  });
});
