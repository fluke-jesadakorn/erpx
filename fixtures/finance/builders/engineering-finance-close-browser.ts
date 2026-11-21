/** Fictional, independent close evidence; operating demo cash never shares this Company. */
import { randomBytes, randomUUID } from "node:crypto";
import { chmod, writeFile } from "node:fs/promises";
import { join } from "node:path";
import { hashPassword } from "better-auth/crypto";
import { seedExpenseCompany } from "../../expenses/company";
import type { WorkContext } from "@erpx/server-identity_access/public/authorization";
import {calculateCloseReconciliations,createAccount,createAccountSchema,createParty,createPeriod,createPeriodSchema,importBankStatement} from '@erpx/server-finance/public/finance';
import {seedFinanceJournal} from '../../../tests/support/helpers/finance-journal-seed';
import { loadIamAuthority } from "@erpx/server-identity_access/public/iam-store";
import { PostgresDatabase } from "@erpx/server-runtime/adapters/runtime/postgres";
import type { FileStore, SqlDatabase } from "@erpx/server-runtime/adapters/runtime/types";

const key = () => randomUUID();
type FinanceRow = Record<string, string | number | null>;
export type EngineeringFinanceCloseUser = {
  email: string;
  userId: string;
  assignmentId: string;
  role: "finance_preparer" | "finance_approver";
};
export type EngineeringFinanceCloseCompany = {
  companyId: string;
  chartId: string;
  periodIds: [string, string];
  openingJournalId: string;
  invoiceIds: [string, string];
  statementIds: [string, string];
  accounts: { bank: string; receivable: string; payable: string; equity: string };
  users: { preparer: EngineeringFinanceCloseUser; approver: EngineeringFinanceCloseUser };
};
export type EngineeringFinanceCloseBrowser = {
  run: string;
  password: string;
  notice: string;
  companies: Record<"desktop" | "mobile", EngineeringFinanceCloseCompany>;
};

/** Shared only with disposable native tests; all Finance amounts use their owner commands. */
export async function seedFinanceCloseCompany(db: SqlDatabase, files: FileStore, project: string) {
  const f = await seedExpenseCompany(db, files);
  const now = Date.now();
  await db.prepare("UPDATE organization_nodes SET display_name=? WHERE id=?")
    .bind(`Fictional ${project} Finance close Company`, f.company).run();
  const preparer = f.reviewer, approver = f.manager;
  for (const [actor, label] of [[preparer, "preparer"], [approver, "approver"]] as const) {
    actor.holdingSlug = "simulation";
    actor.holdingName = "Simulation";
    actor.assignmentScope = "holding";
    actor.branchId = null;
    actor.departmentId = null;
    actor.modules = ["finance"];
    actor.name = `${project} fictional Finance ${label}`;
    actor.scope = { kind: "holding", holdingId: "h001", holdingSlug: "simulation", holdingName: "Simulation", companyId: f.company };
    await db.batch([
      db.prepare("UPDATE assignments SET revoked_at=? WHERE company_id=? AND user_id=? AND id<>?")
        .bind(now, f.company, actor.userId, actor.assignmentId),
      db.prepare("UPDATE assignments SET modules_json='[\"finance\"]' WHERE id=? AND company_id=?")
        .bind(actor.assignmentId, f.company),
      db.prepare("UPDATE hr_people SET legal_name=?,preferred_name=? WHERE company_id=? AND user_id=?")
        .bind(actor.name, actor.name, f.company, actor.userId),
      db.prepare("INSERT INTO iam_members(company_id,user_id,principal_id,person_id,status,login_policy) VALUES(?,?,?,?,'active','local')")
        .bind(f.company, actor.userId, `principal-${actor.userId}`, `person-${actor.userId}`),
    ]);
    actor.iam = await loadIamAuthority(db, f.company, actor.userId, actor.role);
  }
  const accounts = {} as EngineeringFinanceCloseCompany["accounts"];
  for (const [name, code, category, controlKind] of [
    ["bank", "CLOSE-BANK", "asset", "bank"],
    ["receivable", "CLOSE-AR", "asset", "receivable"],
    ["payable", "CLOSE-AP", "liability", "payable"],
    ["equity", "CLOSE-EQ", "equity", "none"],
  ] as const) {
    const account = await createAccount(preparer, createAccountSchema.parse({
      chartId: f.chart, code, name: `Fictional close ${name}`, category,
      currency: "THB", isControl: controlKind !== "none", controlKind,
    }), key());
    accounts[name] = String(account.id);
  }
  const january = await createPeriod(preparer, createPeriodSchema.parse({ name: "January 2027", startDate: "2027-01-01", endDate: "2027-01-31" }), key());
  const february = await createPeriod(preparer, createPeriodSchema.parse({ name: "February 2027", startDate: "2027-02-01", endDate: "2027-02-28" }), key());
  const periodIds: [string, string] = [String(january.id), String(february.id)];
  const customer = await createParty(preparer, { kind: "customer", code: "CLOSE-C", name: "Fictional opening customer", currency: "THB", creditLimit: "0", paymentTermsDays: 30 }, key());
  const supplier = await createParty(preparer, { kind: "supplier", code: "CLOSE-S", name: "Fictional opening supplier", currency: "THB", creditLimit: "0", paymentTermsDays: 30 }, key());
  const openingJournal = await seedFinanceJournal(db, {
    companyId: f.company, chartId: f.chart, periodId: periodIds[0], entryDate: "2027-01-01", currency: "THB",
    key: key(), createdBy: preparer.userId, status: "draft",
    memo: "Fictional retained opening GL and exact AR/AP schedules",
    lines: [
      { accountId: accounts.bank, debit: "1000.00", credit: "0" },
      { accountId: accounts.receivable, debit: "300.00", credit: "0" },
      { accountId: accounts.payable, debit: "0", credit: "200.00" },
      { accountId: accounts.equity, debit: "0", credit: "1100.00" },
    ],
  });
  const openingJournalId = String(openingJournal.id), openedAt = Date.now();
  await db.batch([
    db.prepare("UPDATE finance_journals SET source_type='opening_import',source_id=?,is_opening=1,status='posted',posted_by=?,posted_at=?,version=2 WHERE id=? AND company_id=?")
      .bind(f.chart, approver.userId, openedAt, openingJournalId, f.company),
    db.prepare("INSERT INTO finance_invoices(id,company_id,party_id,kind,invoice_number,invoice_date,due_date,currency,subtotal_minor,tax_minor,total_minor,outstanding_minor,base_total_minor,base_outstanding_minor,debit_account_id,credit_account_id,source_type,source_id,status,journal_id,version,created_by,approved_by,posted_at,created_at,updated_at) VALUES(?,?,?,'receivable','DEMO-OPEN-AR','2027-01-01','2027-01-31','THB',30000,0,30000,30000,30000,30000,?,?,'opening_import',?,'posted',?,1,?,?,?,?,?)")
      .bind(`${openingJournalId}-subledger-0`, f.company, customer.id, accounts.receivable, accounts.equity, openingJournalId, openingJournalId, preparer.userId, approver.userId, openedAt, openedAt, openedAt),
    db.prepare("INSERT INTO finance_invoices(id,company_id,party_id,kind,invoice_number,invoice_date,due_date,currency,subtotal_minor,tax_minor,total_minor,outstanding_minor,base_total_minor,base_outstanding_minor,debit_account_id,credit_account_id,source_type,source_id,status,journal_id,version,created_by,approved_by,posted_at,created_at,updated_at) VALUES(?,?,?,'payable','DEMO-OPEN-AP','2027-01-01','2027-01-31','THB',20000,0,20000,20000,20000,20000,?,?,'opening_import',?,'posted',?,1,?,?,?,?,?)")
      .bind(`${openingJournalId}-subledger-1`, f.company, supplier.id, accounts.equity, accounts.payable, openingJournalId, openingJournalId, preparer.userId, approver.userId, openedAt, openedAt, openedAt),
  ]);
  await db.prepare("INSERT INTO flow_defaults(company_id,kind,step_key,primary_user_id,created_by,reason,created_at) VALUES(?,'journal','draft',?,?,'Independent fictional opening reviewer',?)")
    .bind(f.company, approver.userId, preparer.userId, now).run();
  const statementIds: string[] = [];
  for (const [statementNumber, statementDate] of [["DEMO-JAN-2027", "2027-01-31"], ["DEMO-FEB-2027", "2027-02-28"]] as const) {
    const statement = await importBankStatement(preparer, {
      bankAccount: accounts.bank, statementNumber, statementDate, opening: "1000.00", closing: "1000.00",
      sourceHash: `DECLARED-FICTIONAL-CLOSE-${f.company}-${statementNumber}`, rows: [],
    }, key());
    statementIds.push(String(statement.id));
  }
  const invoiceIds: [string, string] = [`${openingJournalId}-subledger-0`, `${openingJournalId}-subledger-1`];
  for (const periodId of periodIds) {
    const period = await db.prepare("SELECT * FROM finance_periods WHERE id=? AND company_id=?").bind(periodId, f.company).first<FinanceRow>();
    if (!period) throw new Error("FICTIONAL_CLOSE_PERIOD_MISSING");
    const reconciliation = await calculateCloseReconciliations(db, preparer, period);
    const expected = { bank: 100000, receivable: 30000, payable: 20000 };
    if (!reconciliation.ready || reconciliation.rows.length !== 3 || reconciliation.rows.some(row => row.ledgerMinor !== expected[row.kind] || row.sourceMinor !== expected[row.kind] || row.differenceMinor !== 0)) {
      throw new Error("FICTIONAL_CLOSE_OPENING_NOT_RECONCILED");
    }
  }
  return { companyId: f.company, chartId: f.chart, periodIds, openingJournalId, invoiceIds, statementIds: statementIds as [string, string], accounts, preparer, approver };
}

function exactDatabase(db: PostgresDatabase, run: string, binding: string) {
  const url = new URL(String(db.pool.options.connectionString));
  if (url.hostname !== "127.0.0.1" || url.port !== "5432" || url.username !== "erpx_test_admin" || url.pathname !== `/erpx_browser_${run}_${binding}`) {
    throw new Error("EXACT_FINANCE_CLOSE_BROWSER_DATABASE_REQUIRED");
  }
}

export async function seedEngineeringFinanceCloseBrowser(input: { run: string; directory: string; control: PostgresDatabase; db: PostgresDatabase; files: FileStore }) {
  if (!/^[a-f0-9]{12}$/.test(input.run)) throw new Error("EXACT_FINANCE_CLOSE_BROWSER_RUN_REQUIRED");
  exactDatabase(input.control, input.run, "control_db");
  exactDatabase(input.db, input.run, "h001_db");
  const password = `FinanceClose-${randomBytes(18).toString("hex")}`;
  const passwordHash = await hashPassword(password), now = Date.now();
  const companies = {} as EngineeringFinanceCloseBrowser["companies"];
  for (const project of ["desktop", "mobile"] as const) {
    const seeded = await seedFinanceCloseCompany(input.db, input.files, project);
    const users = {} as EngineeringFinanceCloseCompany["users"];
    for (const [actor, label] of [[seeded.preparer, "preparer"], [seeded.approver, "approver"]] as const satisfies ReadonlyArray<readonly [WorkContext, "preparer" | "approver"]>) {
      const email = `finance-close-${input.run}-${project}-${label}@simulation.test`;
      await input.db.prepare("UPDATE hr_people SET email=? WHERE company_id=? AND user_id=?").bind(email, seeded.companyId, actor.userId).run();
      await input.control.batch([
        input.control.prepare('INSERT INTO "user"(id,name,email,emailVerified,createdAt,updatedAt) VALUES(?,?,?,1,?,?)').bind(actor.userId, actor.name, email, now, now),
        input.control.prepare("INSERT INTO account(id,accountId,providerId,issuer,userId,password,createdAt,updatedAt) VALUES(?,?,'credential','local:credential',?,?,?,?)")
          .bind(`account-${actor.userId}`, actor.userId, actor.userId, passwordHash, now, now),
        input.control.prepare("INSERT INTO principals(id,user_id,display_name,status,created_at) VALUES(?,?,?,'active',?)")
          .bind(`principal-${actor.userId}`, actor.userId, actor.name, now),
      ]);
      users[label] = { email, userId: actor.userId, assignmentId: actor.assignmentId, role: label === "preparer" ? "finance_preparer" : "finance_approver" };
    }
    companies[project] = {
      companyId: seeded.companyId, chartId: seeded.chartId, periodIds: seeded.periodIds,
      openingJournalId: seeded.openingJournalId, invoiceIds: seeded.invoiceIds,
      statementIds: seeded.statementIds, accounts: seeded.accounts, users,
    };
  }
  const state: EngineeringFinanceCloseBrowser = {
    run: input.run, password,
    notice: "Fictional Finance close Companies with independently posted opening schedules and declared manual bank statements. Separate desktop/mobile evidence; no real bank or provider certification.",
    companies,
  };
  const fixturePath = join(input.directory, "engineering-finance-close.json");
  await writeFile(fixturePath, `${JSON.stringify(state, null, 2)}\n`, { mode: 0o600 });
  await chmod(fixturePath, 0o600);
  return { companyIds: Object.values(companies).map(company => company.companyId), projects: Object.keys(companies), notice: state.notice, fixturePath };
}
