import type { SqlDatabase } from "@erpx/server-runtime/public/runtime/types";
export type ExpenseLedgerRow = Record<string, string | number | null>;
export async function expenseJournalProjection(db: SqlDatabase, companyId: string, journalId: string) {
  return (await db.prepare("SELECT a.id,a.code,a.name,l.debit_minor,l.credit_minor FROM finance_journals j JOIN finance_journal_lines l ON l.journal_id=j.id JOIN finance_accounts a ON a.id=l.account_id WHERE j.company_id=? AND j.id=? ORDER BY l.ordinal").bind(companyId, journalId).all<ExpenseLedgerRow>()).results;
}
export async function expensePostingAccounts(db: SqlDatabase, companyId: string) {
  return (await db.prepare("SELECT id,code,name,status,chart_id,currency FROM finance_accounts WHERE company_id=?").bind(companyId).all<ExpenseLedgerRow>()).results;
}
