import { beforeAll, describe, expect, it, vi } from "vitest";
import { env, applyMigrations } from "./runtime-env";
import type { WorkContext } from "../src/server/authorization";
import { confirmManualRate } from "../src/server/company-money";
import {
  aging,
  approvePayment,
  changeAccountStatus,
  confirmPayment,
  createPayment,
  importBankStatement,
  importOpeningTrialBalance,
  postJournal,
  previewOpeningTrialBalance,
  releasePayment,
  reverseJournal,
} from "../src/server/finance";

vi.mock("../src/server/runtime", () => ({
  holdingResources: async () => ({ write: env.H001_DB, files: env.H001_FILES }),
  runtimeEnv: async () => env,
}));

const db = env.H001_DB;
const actor: WorkContext = {
  userId: "opening-preparer",
  assignmentId: "opening-assignment",
  role: "finance_preparer",
  companyId: "opening-company",
  branchId: null,
  departmentId: null,
} as WorkContext;
const approver: WorkContext = {
  ...actor,
  userId: "opening-approver",
  assignmentId: "opening-approver-assignment",
  role: "finance_approver",
} as WorkContext;
const treasury: WorkContext = {
  ...actor,
  userId: "opening-treasury",
  assignmentId: "opening-treasury-assignment",
  role: "treasury",
} as WorkContext;

const definition = {
  chartId: "opening-chart",
  periodId: "opening-period",
  entryDate: "2026-01-01",
  currency: "THB",
  lines: [
    { accountId: "opening-cash", debit: "1000.00", credit: "0", description: "Imported cash", subledgerKind: "none" as const },
    { accountId: "opening-equity", debit: "0", credit: "1000.00", description: "Opening equity", subledgerKind: "none" as const },
  ],
  subledgerItems: [],
};

beforeAll(async () => {
  await applyMigrations(db, [...env.STAFF_TEST_MIGRATIONS,{name:"0253_finance_control_account_kinds",track:"holding-write"}]);
  await db.exec(`
    INSERT INTO organization_nodes(id,kind,slug,display_name,company_id) VALUES('opening-company','company','opening-company','Opening company','opening-company');
    INSERT INTO assignments(id,user_id,role,modules_json,company_id,valid_from,valid_until,scope_kind) VALUES('opening-assignment','opening-preparer','finance_preparer','["finance"]','opening-company',0,9999999999999,'holding');
    INSERT INTO assignments(id,user_id,role,modules_json,company_id,valid_from,valid_until,scope_kind) VALUES('opening-approver-assignment','opening-approver','finance_approver','["finance"]','opening-company',0,9999999999999,'holding');
    INSERT INTO assignments(id,user_id,role,modules_json,company_id,valid_from,valid_until,scope_kind) VALUES('opening-treasury-assignment','opening-treasury','treasury','["finance","people_payroll"]','opening-company',0,9999999999999,'holding');
    INSERT INTO hr_people(id,user_id,company_id,legal_name,country_code,status,created_by,created_at,updated_at) VALUES('opening-approver-person','opening-approver','opening-company','Opening Approver','TH','active','opening-preparer',1,1);
    INSERT INTO hr_engagements(id,person_id,company_id,worker_type,employment_type,status,start_date,job_title,currency,created_by,created_at,updated_at) VALUES('opening-approver-engagement','opening-approver-person','opening-company','employee','salaried','active','2026-01-01','Finance Approver','THB','opening-preparer',1,1);
    INSERT INTO finance_charts(id,company_id,name,base_currency,accounting_basis,status,created_by,created_at,updated_at) VALUES('opening-chart','opening-company','Opening chart','THB','IFRS','active','opening-preparer',1,1);
    INSERT INTO finance_accounts(id,chart_id,company_id,code,name,category,currency,created_at,updated_at) VALUES
      ('opening-cash','opening-chart','opening-company','1000','Cash','asset','THB',1,1),
      ('opening-equity','opening-chart','opening-company','3000','Opening equity','equity','THB',1,1);
    INSERT INTO finance_parties(id,company_id,kind,code,name,currency,status,created_at,updated_at) VALUES
      ('opening-customer-a','opening-company','customer','C001','Alpha customer','THB','active',1,1),
      ('opening-customer-b','opening-company','customer','C002','Beta customer','THB','active',1,1);
    INSERT INTO finance_periods(id,company_id,name,start_date,end_date,created_at) VALUES('opening-period','opening-company','Opening 2026','2026-01-01','2026-12-31',1);
  `);
  await confirmManualRate(approver, {
    currency: "THB", effectiveDate: "2026-01-01", currencyPerUsd: "36",
    note: "Synthetic isolated opening-balance fixture quote; not a market observation or provider result.",
  }, "opening-fixture-thb-rate");
});

describe("opening trial balance", () => {
  it("changes an account status with version, reason, idempotency and audit evidence", async () => {
    const chartId = `account-status-chart-${crypto.randomUUID()}`;
    const accountId = `${chartId}-account`;
    await db.exec(`
      INSERT INTO finance_charts(id,company_id,name,base_currency,accounting_basis,status,created_by,created_at,updated_at) VALUES('${chartId}','opening-company','Account status chart','THB','IFRS','active','opening-preparer',1,1);
      INSERT INTO finance_accounts(id,chart_id,company_id,code,name,category,currency,created_at,updated_at) VALUES('${accountId}','${chartId}','opening-company','1999','Temporary account','asset','THB',1,1);
    `);
    const inactive = await changeAccountStatus(actor, chartId, accountId, { status: "inactive", expectedVersion: 1, reason: "Retire the unused account before close" }, "account-status-inactive");
    expect(inactive).toMatchObject({ id: accountId, status: "inactive", version: 2 });
    expect(await changeAccountStatus(actor, chartId, accountId, { status: "inactive", expectedVersion: 1, reason: "Retire the unused account before close" }, "account-status-inactive")).toEqual(inactive);
    expect(await db.prepare("SELECT status,version FROM finance_accounts WHERE id=?").bind(accountId).first()).toEqual({ status: "inactive", version: 2 });
    expect(await db.prepare("SELECT action,record_id,record_version FROM audit_events WHERE command_id=?").bind("account-status-inactive").first()).toMatchObject({ action: "finance.account.inactive", record_id: accountId, record_version: 2 });
    await expect(changeAccountStatus(actor, chartId, accountId, { status: "active", expectedVersion: 1, reason: "Stale status edit" }, "account-status-stale")).rejects.toMatchObject({ code: "VERSION_CONFLICT" });
    const active = await changeAccountStatus(actor, chartId, accountId, { status: "active", expectedVersion: 2, reason: "Restore the account for the next period" }, "account-status-active");
    expect(active).toMatchObject({ id: accountId, status: "active", version: 3 });
  });

  it("previews normalized lines and imports one auditable draft", async () => {
    const preview = await previewOpeningTrialBalance(actor, definition);
    expect(preview).toMatchObject({ debit: "1000.00", credit: "1000.00", currency: "THB" });
    expect(preview.previewToken).toHaveLength(64);
    expect(preview.lines[0]).toMatchObject({ accountCode: "1000", accountName: "Cash" });

    const input = { ...definition, previewToken: preview.previewToken };
    const created = await importOpeningTrialBalance(actor, input, "opening-import-one");
    expect(created).toMatchObject({ status: "draft", debit: "1000.00", credit: "1000.00" });
    expect(await importOpeningTrialBalance(actor, input, "opening-import-one")).toEqual(created);
    expect(await db.prepare("SELECT source_type,is_opening,status FROM finance_journals WHERE id=?").bind(created.id).first()).toEqual({ source_type: "opening_import", is_opening: 1, status: "draft" });
    expect(await db.prepare("SELECT COUNT(*) n FROM audit_events WHERE action='finance.journal.create'").first()).toEqual({ n: 1 });
  });

  it("rejects invalid and stale previews", async () => {
    await expect(previewOpeningTrialBalance(actor, { ...definition, chartId: "opening-chart-invalid", lines: definition.lines })).rejects.toMatchObject({ code: "CHART_NOT_FOUND" });
    await expect(previewOpeningTrialBalance(actor, { ...definition, lines: definition.lines.map((line, index) => ({ ...line, credit: "0", debit: index ? "1.00" : line.debit })) })).rejects.toMatchObject({ code: "UNBALANCED_OPENING_BALANCE" });

    const otherChart = "opening-chart-stale";
    await db.exec(`
      INSERT INTO finance_charts(id,company_id,name,base_currency,accounting_basis,status,created_by,created_at,updated_at) VALUES('${otherChart}','opening-company','Stale chart','THB','IFRS','active','opening-preparer',1,1);
      INSERT INTO finance_accounts(id,chart_id,company_id,code,name,category,currency,created_at,updated_at) VALUES
        ('stale-cash','${otherChart}','opening-company','1000','Cash','asset','THB',1,1),
        ('stale-equity','${otherChart}','opening-company','3000','Equity','equity','THB',1,1);
    `);
    const staleDefinition = { ...definition, chartId: otherChart, lines: [{ ...definition.lines[0], accountId: "stale-cash" }, { ...definition.lines[1], accountId: "stale-equity" }] };
    const preview = await previewOpeningTrialBalance(actor, staleDefinition);
    await db.prepare("UPDATE finance_accounts SET updated_at=2 WHERE id='stale-cash'").run();
    await expect(importOpeningTrialBalance(actor, { ...staleDefinition, previewToken: preview.previewToken }, "opening-import-stale")).rejects.toMatchObject({ code: "OPENING_PREVIEW_STALE" });
  });

  it("allows only one concurrent opening import per chart", async () => {
    const chartId = "opening-chart-race";
    await db.exec(`
      INSERT INTO finance_charts(id,company_id,name,base_currency,accounting_basis,status,created_by,created_at,updated_at) VALUES('${chartId}','opening-company','Race chart','THB','IFRS','active','opening-preparer',1,1);
      INSERT INTO finance_accounts(id,chart_id,company_id,code,name,category,currency,created_at,updated_at) VALUES
        ('race-cash','${chartId}','opening-company','1000','Cash','asset','THB',1,1),
        ('race-equity','${chartId}','opening-company','3000','Equity','equity','THB',1,1);
    `);
    const input = { ...definition, chartId, lines: [{ ...definition.lines[0], accountId: "race-cash" }, { ...definition.lines[1], accountId: "race-equity" }] };
    const preview = await previewOpeningTrialBalance(actor, input);
    const results = await Promise.allSettled([
      importOpeningTrialBalance(actor, { ...input, previewToken: preview.previewToken }, "opening-import-race-a"),
      importOpeningTrialBalance(actor, { ...input, previewToken: preview.previewToken }, "opening-import-race-b"),
    ]);
    expect(results.filter((result) => result.status === "fulfilled")).toHaveLength(1);
    expect(await db.prepare("SELECT COUNT(*) n FROM finance_journals WHERE chart_id=? AND is_opening=1 AND status='draft'").bind(chartId).first()).toEqual({ n: 1 });
  });

  it("reconciles customer schedules, posts them with independent approval, and voids them with the opening reversal", async () => {
    await db.exec(`
      INSERT INTO finance_charts(id,company_id,name,base_currency,accounting_basis,status,created_by,created_at,updated_at) VALUES('opening-subledger-chart','opening-company','Subledger chart','THB','IFRS','active','opening-preparer',1,1);
      INSERT INTO finance_accounts(id,chart_id,company_id,code,name,category,currency,is_control,control_kind,created_at,updated_at) VALUES
        ('opening-ar','opening-subledger-chart','opening-company','1100','Trade receivables','asset','THB',1,'receivable',1,1),
        ('opening-offset','opening-subledger-chart','opening-company','3000','Opening equity','equity','THB',0,'none',1,1);
    `);
    const reconciled = {
      chartId: "opening-subledger-chart", periodId: "opening-period", entryDate: "2026-01-01", currency: "THB",
      lines: [
        { accountId: "opening-ar", debit: "600.00", credit: "0", description: "Opening receivables", subledgerKind: "receivable" as const },
        { accountId: "opening-offset", debit: "0", credit: "600.00", description: "Opening equity", subledgerKind: "none" as const },
      ],
      subledgerItems: [
        { kind: "receivable" as const, partyId: "opening-customer-a", reference: "OPEN-AR-001", dueDate: "2026-01-15", controlAccountId: "opening-ar", offsetAccountId: "opening-offset", amount: "250.00" },
        { kind: "receivable" as const, partyId: "opening-customer-b", reference: "OPEN-AR-002", dueDate: "2026-02-15", controlAccountId: "opening-ar", offsetAccountId: "opening-offset", amount: "350.00" },
      ],
    };
    const preview=await previewOpeningTrialBalance(actor,reconciled);
    expect(preview).toMatchObject({subledgerCount:2,debit:"600.00",credit:"600.00"});
    const journal=await importOpeningTrialBalance(actor,{...reconciled,previewToken:preview.previewToken},"opening-reconciled");
    expect(journal).toMatchObject({status:"draft",subledgerCount:2});
    await expect(importOpeningTrialBalance(actor,{...reconciled,subledgerItems:reconciled.subledgerItems.map((item,index)=>index?item:{...item,reference:"CHANGED-REFERENCE"}),previewToken:preview.previewToken},"opening-reconciled")).rejects.toMatchObject({code:"IDEMPOTENCY_CONFLICT"});
    expect(await db.prepare("SELECT count(*) n FROM finance_invoices WHERE source_id=? AND status='draft'").bind(journal.id).first()).toEqual({n:2});
    await postJournal(approver,String(journal.id),"opening-reconciled-post",{expectedVersion:1});
    const open=await aging(approver,"receivable","2026-03-01");
    expect(open.filter(row=>row.source_id===journal.id).map(row=>row.outstanding)).toEqual(["250.00","350.00"]);
    await reverseJournal(approver,String(journal.id),"opening-reconciled-reverse",{periodId:"opening-period",entryDate:"2026-01-02",memo:"Correct approved opening schedules"});
    expect(await db.prepare("SELECT count(*) n FROM finance_invoices WHERE source_id=? AND status='voided' AND outstanding_minor=0").bind(journal.id).first()).toEqual({n:2});
  });

  it("rejects a subledger schedule that does not equal its GL control balance", async () => {
    const invalid={
      chartId:"opening-chart-stale",periodId:"opening-period",entryDate:"2026-01-01",currency:"THB",
      lines:[
        {accountId:"stale-cash",debit:"1000.00",credit:"0",subledgerKind:"receivable" as const},
        {accountId:"stale-equity",debit:"0",credit:"1000.00",subledgerKind:"none" as const},
      ],
      subledgerItems:[{kind:"receivable" as const,partyId:"opening-customer-a",reference:"OPEN-AR-MISMATCH",dueDate:"2026-01-15",controlAccountId:"stale-cash",offsetAccountId:"stale-equity",amount:"999.00"}],
    };
    await db.prepare("UPDATE finance_accounts SET is_control=1,control_kind='receivable' WHERE id='stale-cash'").run();
    await expect(previewOpeningTrialBalance(actor,invalid)).rejects.toMatchObject({code:"OPENING_SUBLEDGER_MISMATCH"});
  });

  it("settles an opening receivable through Treasury and blocks reversal after settlement", async () => {
    await db.exec(`
      INSERT INTO finance_charts(id,company_id,name,base_currency,accounting_basis,status,created_by,created_at,updated_at) VALUES('opening-settle-chart','opening-company','Settlement chart','THB','IFRS','active','opening-preparer',1,1);
      INSERT INTO finance_accounts(id,chart_id,company_id,code,name,category,currency,is_control,control_kind,created_at,updated_at) VALUES
        ('opening-settle-ar','opening-settle-chart','opening-company','1100','Opening trade receivables','asset','THB',1,'receivable',1,1),
        ('opening-settle-bank','opening-settle-chart','opening-company','1200','Opening settlement bank','asset','THB',1,'bank',1,1),
        ('opening-settle-equity','opening-settle-chart','opening-company','3000','Opening equity','equity','THB',0,'none',1,1);
    `);
    const reconciled = {
      chartId: "opening-settle-chart", periodId: "opening-period", entryDate: "2026-01-01", currency: "THB",
      lines: [
        { accountId: "opening-settle-ar", debit: "250.00", credit: "0", description: "Opening receivable", subledgerKind: "receivable" as const },
        { accountId: "opening-settle-equity", debit: "0", credit: "250.00", description: "Opening equity", subledgerKind: "none" as const },
      ],
      subledgerItems: [{ kind: "receivable" as const, partyId: "opening-customer-a", reference: "OPEN-SETTLE-001", dueDate: "2026-01-15", controlAccountId: "opening-settle-ar", offsetAccountId: "opening-settle-equity", amount: "250.00" }],
    };
    const preview = await previewOpeningTrialBalance(actor, reconciled);
    const journal = await importOpeningTrialBalance(actor, { ...reconciled, previewToken: preview.previewToken }, "opening-settlement");
    await postJournal(approver, String(journal.id), "opening-settlement-post", { expectedVersion: 1 });

    const payment = await createPayment(actor, { partyId: "opening-customer-a", kind: "receipt", paymentNumber: "OPEN-RECEIPT-001", paymentDate: "2026-01-20", currency: "THB", amount: "250.00", bankAccount: "opening-settle-bank" }, "opening-settlement-payment");
    await approvePayment(approver, String(payment.id), { expectedVersion: 1, proof: "Independent opening receivable review" }, "opening-settlement-approve");
    await releasePayment(treasury, String(payment.id), { expectedVersion: 2, proof: "Treasury verified the opening receipt" }, "opening-settlement-release");
    const statement = await importBankStatement(actor, { bankAccount: "opening-settle-bank", statementNumber: "OPEN-SETTLE-STMT-001", statementDate: "2026-01-31", opening: "0", closing: "250.00", sourceHash: "opening-settlement-source", rows: [{ externalId: "OPEN-SETTLE-BANK-001", transactionDate: "2026-01-20", description: "Opening receivable receipt", amount: "250.00", currency: "THB" }] }, "opening-settlement-bank");
    const bankId = String(await db.prepare("SELECT id FROM finance_bank_transactions WHERE statement_id=?").bind(statement.id).first("id"));
    const settled = await confirmPayment(treasury, String(payment.id), { expectedVersion: 3, periodId: "opening-period", bankTransactionId: bankId, bankVersion: 1, bankReference: "OPEN-SETTLE-BANK-001", proof: "Exact bank evidence matches the opening receivable", allocations: [{ invoiceId: `${journal.id}-subledger-0`, amount: "250.00" }] }, "opening-settlement-confirm");
    expect(settled).toMatchObject({ status: "settled", version: 4 });
    expect(await db.prepare("SELECT status,outstanding_minor FROM finance_invoices WHERE id=?").bind(`${journal.id}-subledger-0`).first()).toEqual({ status: "paid", outstanding_minor: 0 });
    await expect(reverseJournal(approver, String(journal.id), "opening-settlement-reverse", { periodId: "opening-period", entryDate: "2026-02-01", memo: "Must correct source settlement first" })).rejects.toMatchObject({ code: "OPENING_SUBLEDGER_SETTLED" });
  });
});
