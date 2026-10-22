import { expenseAccountingFacts, expensePostingDraft, type ExpensePostingRow } from "@erpx/server-people_payroll/domain/expense-posting-preview";
export function createExpensePostingPreview(ports: { repository(): Promise<{
  posted(journalId: string): Promise<ExpensePostingRow[]>;
  configuration(): Promise<ExpensePostingRow | null>;
  accounts(): Promise<ExpensePostingRow[]>;
}> }) {
  return async (expense: ExpensePostingRow) => {
    const repository = await ports.repository();
    if (expense.journal_id) return { kind: "posted" as const, posted: await repository.posted(String(expense.journal_id)) };
    const config = await repository.configuration(), accounting = expenseAccountingFacts(expense);
    if (!config) return { kind: "unconfigured" as const };
    return expensePostingDraft(expense, config, await repository.accounts(), accounting);
  };
}
