import 'server-only';
import { readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';

// SQL owned by native Rust pp_expense_workflow consts; JSON mirror enforced by
// `pp_expense_workflow_sql_contract_matches_native_consts`.
const contractPath = fileURLToPath(new URL('./native-pp_expense_workflow-sql.json', import.meta.url));
const contract = JSON.parse(readFileSync(contractPath, 'utf8')) as Record<string, string>;

/** Native-owned SQL for a pp_expense_workflow statement kind. */
export function nativePpExpenseWorkflowSql(kind: string): string {
  const sql = contract[kind];
  if (!sql) throw new Error(`Unknown native pp_expense_workflow SQL kind: ${kind}`);
  return sql;
}
