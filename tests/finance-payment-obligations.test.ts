import { beforeAll, beforeEach, expect, it, vi } from "vitest";
import { applyMigrations, env, type TestMigration } from "./runtime-env";
import { expenseDb as db, expenseFixture, key } from "./helpers/expense-fixture";
import { financeWorkbench, paymentObligationsWorkspace, paymentRecordsWorkspace } from "../src/server/finance-workspace";

vi.mock("server-only", () => ({}));
vi.mock("../src/server/runtime", () => ({ holdingResources: async () => ({ write: env.H001_DB, files: env.H001_FILES }), runtimeEnv: async () => env }));

let f: Awaited<ReturnType<typeof expenseFixture>>;

beforeAll(async () => {
  await applyMigrations(db, env.STAFF_TEST_MIGRATIONS as TestMigration[]);
});

beforeEach(async () => {
  f = await expenseFixture();
});

it("finds settled payment evidence beyond the action queue with scoped, currency-preserving pagination",async()=>{
  const other=await expenseFixture(),prefix=`History-${key()}`;
  for(const [index,company,currency] of [[1,f.company,"THB"],[2,f.company,"USD"],[3,f.company,"THB"],[4,other.company,"USD"]] as const){
    await db.prepare("INSERT INTO finance_payments(id,company_id,kind,payment_number,payment_date,currency,amount_minor,outstanding_minor,status,created_by,created_at,updated_at) VALUES(?,?,'receipt',?,'2026-09-25',?,10000,0,'settled',?,1,1)")
      .bind(`${prefix}-${index}`,company,`${prefix}-${index}`,currency,f.owner.userId).run();
  }
  const first=await paymentRecordsWorkspace(f.owner,{q:prefix,paymentStatus:"settled"},2);
  expect(first.items.map(item=>item.id)).toEqual([`${prefix}-3`,`${prefix}-2`]);
  expect(first.items.map(item=>item.currency)).toEqual(["THB","USD"]);
  const second=await paymentRecordsWorkspace(f.owner,{q:prefix,paymentStatus:"settled",paymentCursor:first.nextCursor!},2);
  expect(second.items.map(item=>item.id)).toEqual([`${prefix}-1`]);
  expect(second.nextCursor).toBeNull();
  await expect(paymentRecordsWorkspace(other.owner,{q:prefix,paymentStatus:"settled",paymentCursor:first.nextCursor!},2)).rejects.toMatchObject({code:"INVALID_CURSOR"});
  await expect(paymentRecordsWorkspace({...f.owner,role:"employee"},{q:prefix})).rejects.toMatchObject({status:403});
});

it("combines payment sources into one finance queue while redacting people payments from preparers", async () => {
  const genericId = `payment-${key()}`;
  const payrollBatchId = `payroll-batch-${key()}`;
  await db.batch([
    db.prepare("INSERT INTO finance_payments(id,company_id,kind,payment_number,payment_date,currency,amount_minor,outstanding_minor,status,created_by,created_at,updated_at) VALUES(?,?, 'payment',?,'2026-09-25','THB',10000,10000,'approved',?,?,?)").bind(genericId, f.company, `PAY-${key()}`, f.reviewer.userId, 1, 1),
    db.prepare("INSERT INTO hr_payment_batches(id,company_id,source_type,source_id,currency,status,total_minor,created_by,created_at,updated_at) VALUES(?,?, 'payroll',?,'THB','draft',20000,?,?,?)").bind(payrollBatchId, f.company, `payroll-run-${key()}`, f.reviewer.userId, 1, 1),
    db.prepare("INSERT INTO hr_payment_batch_items(id,batch_id,reference,payee_name,amount_minor,currency) VALUES(?,?,?,'Demo employee',20000,'THB')").bind(`payroll-item-${key()}`, payrollBatchId, `PAYEE-${key()}`),
  ]);

  const finance = await paymentObligationsWorkspace(f.owner);
  expect(finance).toEqual(expect.arrayContaining([
    expect.objectContaining({ id: genericId, sourceType: "generic_payment", sourceLabel: "Supplier payment", amountMinor: 10000, nextAction: "Release through Treasury" }),
    expect.objectContaining({ id: payrollBatchId, sourceType: "people_payment", sourceLabel: "Payroll payment batch", amountMinor: 20000, href: expect.stringContaining("people/payroll/") }),
  ]));

  const preparer = { ...f.reviewer, role: "finance_preparer" };
  const restricted = await paymentObligationsWorkspace(preparer);
  expect(restricted.some(item => item.id === payrollBatchId)).toBe(false);
  expect(restricted.some(item => item.id === genericId)).toBe(true);
  const workbench = await financeWorkbench(f.owner);
  expect(workbench.attention.find(item => item.id === "payment-review")?.count).toBeGreaterThanOrEqual(2);
  expect((await paymentObligationsWorkspace(f.owner, { sourceType: "generic_payment" })).map(item => item.id)).toEqual([genericId]);
  expect((await paymentObligationsWorkspace(f.owner, { status: "approved" })).map(item => item.id)).toContain(genericId);
});

it("keeps overdue queue amounts in their transaction currencies across Company books",async()=>{
  const party=key();
  await db.prepare("INSERT INTO finance_parties(id,company_id,kind,code,name,currency,created_at,updated_at) VALUES(?,?,'customer',?,'Mixed currency customer','THB',1,1)").bind(party,f.company,party).run();
  for(const [currency,amount] of [["THB",10000],["USD",20000]] as const){
    const id=key();await db.prepare("INSERT INTO finance_invoices(id,company_id,party_id,kind,invoice_number,invoice_date,due_date,currency,subtotal_minor,total_minor,outstanding_minor,debit_account_id,credit_account_id,status,created_by,created_at,updated_at) VALUES(?,?,?,'receivable',?,'2026-01-01','2026-01-01',?,?,?,?,?,?,'posted',?,1,1)").bind(id,f.company,party,id,currency,amount,amount,amount,f.accounts.cash,f.accounts.revenue,f.reviewer.userId).run();
  }
  const queue=(await financeWorkbench(f.owner)).attention.find(item=>item.id==="receivable-overdue");
  expect(queue).toMatchObject({count:2,currency:null,amountMinor:null,amounts:[{currency:"THB",amountMinor:10000},{currency:"USD",amountMinor:20000}]});
});
