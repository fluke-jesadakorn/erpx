export type ExpensePostingRow = Record<string, string | number | null>;
export function expenseAccountingFacts(expense: ExpensePostingRow) {
  return JSON.parse(String(expense.accounting_json ?? "{}")) as { periodId?: string; entryDate?: string };
}
export function expensePostingDraft(expense: ExpensePostingRow, config: ExpensePostingRow, accounts: ExpensePostingRow[], accounting: ReturnType<typeof expenseAccountingFacts>) {
  const mappings = [
    { id: expense.category_account_id, debit: Number(expense.amount_minor), credit: 0 },
    { id: config.input_tax_account_id, debit: Number(expense.tax_minor), credit: 0 },
    { id: config.payable_account_id, debit: 0, credit: Number(expense.amount_minor) + Number(expense.tax_minor) },
  ].filter(line => line.debit || line.credit);
  const invalid = mappings.some(line => !accounts.some(account => account.id === line.id && account.status === "active" && account.chart_id === config.chart_id && account.currency === expense.currency));
  return { kind: "draft" as const, accounting, mappings, accounts, invalid, total: Number(expense.amount_minor) + Number(expense.tax_minor) };
}
