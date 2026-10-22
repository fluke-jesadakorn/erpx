import { describe, expect, it } from "vitest";
import { expenseAccountingViolation, expensePostingViolation, mayEditExpense, mayReviewExpense } from "@erpx/server-people_payroll/domain/expense-policy";
import { createExpensePreview, type ExpensePreviewActor, type ExpensePreviewPorts } from "@erpx/server-people_payroll/application/expense-preview";
import { createExpensePostingPreview } from "@erpx/server-people_payroll/application/expense-posting-preview";

describe("Expense domain independence", () => {
  const employee = { userId: "employee" };
  it("limits editing to the claimant or creator of an unsubmitted active claim", () => {
    expect(mayEditExpense(employee, { status: "draft", created_by: "preparer", claimant_user_id: "employee" })).toBe(true);
    expect(mayEditExpense(employee, { status: "rejected", created_by: "employee", cancelled_at: 1 })).toBe(false);
    expect(mayEditExpense(employee, { status: "approved", created_by: "employee" })).toBe(false);
    expect(mayEditExpense(employee, { status: "draft", created_by: "other", claimant_user_id: "other" })).toBe(false);
  });
  it("requires the current independent reviewer and preserves v1 assignment selection", () => {
    const row = { status: "in_review", workflow_version: 1, can_decide: 1, current_approver_id: "reviewer", created_by: "employee", prepared_by: "preparer", claimant_user_id: "employee" };
    expect(mayReviewExpense({ userId: "reviewer" }, row)).toBe(true);
    expect(mayReviewExpense({ userId: "other" }, row)).toBe(false);
    expect(mayReviewExpense(employee, { ...row, workflow_version: 2, current_approver_id: "employee" })).toBe(false);
    expect(mayReviewExpense({ userId: "reviewer" }, { ...row, can_decide: 0 })).toBe(false);
  });
  it("rejects negative or all-inclusive tax and a date preceding the expense", () => {
    expect(expenseAccountingViolation(10000, 700, "2026-10-01", "2026-10-05")).toBeNull();
    expect(expenseAccountingViolation(10000, 10000, "2026-10-01", "2026-10-05")?.code).toBe("TAX_INVALID");
    expect(expenseAccountingViolation(10000, -1, "2026-10-01", "2026-10-05")?.code).toBe("TAX_INVALID");
    expect(expenseAccountingViolation(10000, 700, "2026-10-05", "2026-10-01")?.code).toBe("POSTING_DATE_INVALID");
  });
  it("requires another person to post and preserves the expense date", () => {
    const row = { created_by: "employee", prepared_by: "preparer", claimant_user_id: "employee", expense_date: "2026-10-01" };
    expect(expensePostingViolation(employee, row, "2026-10-05")?.code).toBe("SELF_POST_FORBIDDEN");
    expect(expensePostingViolation({ userId: "reviewer" }, row, "2026-09-30")?.code).toBe("POSTING_DATE_INVALID");
    expect(expensePostingViolation({ userId: "reviewer" }, row, "2026-10-01")).toBeNull();
  });
});

describe("Expense posting projection", () => {
  it("reads posted journal lines without reading a mutable mapping", async () => {
    const posted = [{ id: "account", debit_minor: 100, credit_minor: 0 }];
    const preview = createExpensePostingPreview({ repository: async () => ({ posted: async () => posted, configuration: async () => { throw new Error("unexpected configuration read"); }, accounts: async () => { throw new Error("unexpected account read"); } }) });
    expect(await preview({ journal_id: "journal" })).toEqual({ kind: "posted", posted });
  });
  it("rejects inactive, foreign-chart and foreign-currency mappings before showing a valid posting", async () => {
    const expense = { category_account_id: "cost", amount_minor: 10000, tax_minor: 700, currency: "THB", accounting_json: '{"entryDate":"2026-10-05"}' };
    const configuration = { chart_id: "chart", input_tax_account_id: "tax", payable_account_id: "payable" };
    const accounts = ["cost", "tax", "payable"].map(id => ({ id, status: "active", chart_id: "chart", currency: "THB" }));
    const preview = (rows: typeof accounts) => createExpensePostingPreview({ repository: async () => ({ posted: async () => [], configuration: async () => configuration, accounts: async () => rows }) })(expense);
    expect(await preview(accounts)).toMatchObject({ kind: "draft", invalid: false, total: 10700, mappings: [{ debit: 10000, credit: 0 }, { debit: 700, credit: 0 }, { debit: 0, credit: 10700 }] });
    for (const replacement of [{ status: "inactive" }, { chart_id: "other" }, { currency: "USD" }]) expect(await preview(accounts.map(account => account.id === "tax" ? { ...account, ...replacement } : account))).toMatchObject({ invalid: true });
  });
});

describe("Expense preview application", () => {
  class Failure extends Error { constructor(readonly status: number, readonly code: string, message: string) { super(message); } }
  const actor: ExpensePreviewActor = { userId: "employee", companyId: "company", branchId: "branch", departmentId: "team" };
  function ports(overrides: Partial<ExpensePreviewPorts<ExpensePreviewActor, { steps: string[]; blockers: string[] }>> = {}) {
    return {
      options: async () => ({ people: [{ id: "person", user_id: "employee" }] }),
      expense: async () => ({ status: "draft", created_by: "employee", claimant_user_id: "employee" }),
      staff: async () => ({ branch_id: "branch", department_id: "team" }),
      route: async () => ({ steps: ["independent-review"], blockers: [] }),
      error: violation => new Failure(violation.status, violation.code, violation.message),
      blocker: error => error instanceof Failure && [409, 422].includes(error.status) ? error.message : null,
      ...overrides,
    } satisfies ExpensePreviewPorts<ExpensePreviewActor, { steps: string[]; blockers: string[] }>;
  }
  it("resolves a claimant through injected ports without runtime resources", async () => {
    expect(await createExpensePreview(ports())(actor, { personId: "person" })).toEqual({ steps: ["independent-review"], blockers: [] });
  });
  it("rejects a foreign person and a locked claim before resolving an approval route", async () => {
    await expect(createExpensePreview(ports())(actor, { personId: "foreign" })).rejects.toMatchObject({ code: "PERSON_NOT_FOUND" });
    await expect(createExpensePreview(ports({ expense: async () => ({ status: "approved", created_by: "employee" }) }))(actor, {}, "claim")).rejects.toMatchObject({ code: "EXPENSE_LOCKED" });
  });
  it("prevents replacing the claimant and hides staff outside the selected scope", async () => {
    await expect(createExpensePreview(ports({ options: async () => ({ people: [{ id: "person", user_id: "other" }] }) }))(actor, { personId: "person" }, "claim")).rejects.toMatchObject({ code: "CLAIMANT_IMMUTABLE" });
    await expect(createExpensePreview(ports({ staff: async () => ({ branch_id: "other", department_id: "team" }) }))(actor, {})).rejects.toMatchObject({ code: "PERSON_NOT_FOUND" });
  });
  it("shows business blockers while propagating infrastructure failures", async () => {
    const blocked = createExpensePreview(ports({ route: async () => { throw new Failure(409, "ROUTE_BLOCKED", "Choose an independent owner."); } }));
    expect(await blocked(actor, {})).toEqual({ steps: [], blockers: ["Choose an independent owner."] });
    const broken = createExpensePreview(ports({ route: async () => { throw new Error("unavailable"); } }));
    await expect(broken(actor, {})).rejects.toThrow("unavailable");
  });
});
