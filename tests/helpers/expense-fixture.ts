import { mapSpendingOrganization } from "./spending-organization";
import { seedExpenseCompany } from "../../fixtures/expenses/company";
import { env } from "../runtime-env";
import type { WorkContext } from "../../src/server/authorization";
import {
  saveExpense,
  getExpense,
  transitionExpense,
  expenseUpdateSchema,
} from "../../src/server/expenses";
import { saveExpenseAccounting } from "../../src/server/expense-workflow";
export const expenseDb = env.H001_DB,
  key = () => crypto.randomUUID();
export async function expenseFixture(mapped = true) {
  const f = await seedExpenseCompany(expenseDb, env.H001_FILES);
  if(mapped)await mapSpendingOrganization(f);
  const {
    db,
    company,
    branch,
    department,
    accountsDepartment,
    chart,
    period,
    accounts,
    claimant,
    head,
    tie,
    reviewer,
    manager,
    payer,
    owner,
    it,
    member,
    grant,
    attach,
  } = f;
  for (const [kind,step,user] of [["expense","department_approval",head.userId],["request","awaiting_approval",head.userId]]) {
    await db.prepare("INSERT INTO flow_defaults(company_id,kind,step_key,branch_id,department_id,primary_user_id,created_by,reason,created_at) VALUES(?,?,?,?,?,?,?,?,?)").bind(company,kind,step,branch,department,user,owner.userId,"Fixture primary responsibility",Date.now()).run();
  }
  async function claim(who = claimant) {
    return String(
      (
        await saveExpense(
          who,
          null,
          expenseUpdateSchema.parse({
            expectedVersion: 1,
            expenseDate: "2026-09-01",
            merchant: "Supplier",
            receiptReference: key(),
            businessPurpose: "Travel to visit the customer",
            currency: "THB",
            amount: "100",
            tax: "7",
            categoryAccountId: accounts.expense,
          }),
          key(),
        )
      ).id,
    );
  }
  async function action(
    who: WorkContext,
    id: string,
    action: "submit" | "approve" | "reject" | "cancel",
  ) {
    const row = await getExpense(who, id);
    return transitionExpense(
      who,
      id,
      action,
      {
        expectedVersion: Number(row.version),
        reason: "Reviewed original evidence",
      },
      key(),
    );
  }
  async function throughAccounts(id: string) {
    await attach(id);
    await action(claimant, id, "submit");
    await action(head, id, "approve");
    const row = await getExpense(reviewer, id);
    await saveExpenseAccounting(
      reviewer,
      id,
      {
        expectedVersion: Number(row.version),
        categoryAccountId: accounts.expense,
        tax: "7",
        periodId: period,
        entryDate: "2026-09-02",
        note: "Documents verified",
      },
      key(),
    );
    await action(reviewer, id, "approve");
  }
  return {
    db,
    company,
    branch,
    department,
    accountsDepartment,
    chart,
    period,
    accounts,
    claimant,
    head,
    tie,
    reviewer,
    manager,
    payer,
    owner,
    it,
    member,
    grant,
    attach,
    claim,
    action,
    throughAccounts,
  };
}
