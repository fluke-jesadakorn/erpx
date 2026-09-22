import { beforeAll, beforeEach, expect, it, vi } from "vitest";
import { applyMigrations, env, type TestMigration } from "./runtime-env";
import { expenseDb as db, expenseFixture, key } from "./helpers/expense-fixture";
import { financeWorkbench, paymentObligationsWorkspace } from "../src/server/finance-workspace";

vi.mock("server-only", () => ({}));
vi.mock("../src/server/runtime", () => ({ holdingResources: async () => ({ write: env.H001_DB, files: env.H001_FILES }), runtimeEnv: async () => env }));

let f: Awaited<ReturnType<typeof expenseFixture>>;

beforeAll(async () => {
  await applyMigrations(db, env.STAFF_TEST_MIGRATIONS as TestMigration[]);
});

beforeEach(async () => {
  f = await expenseFixture();
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
