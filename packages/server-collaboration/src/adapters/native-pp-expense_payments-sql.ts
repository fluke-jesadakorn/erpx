import 'server-only';
import { readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';

// SQL owned by native Rust pp_expense_payments consts; JSON mirror enforced by
// `pp_expense_payments_sql_contract_matches_native_consts`.
const contractPath = fileURLToPath(new URL('./native-pp_expense_payments-sql.json', import.meta.url));
const contract = JSON.parse(readFileSync(contractPath, 'utf8')) as Record<string, string>;

/** Native-owned SQL for a pp_expense_payments statement kind. */
export function nativePpExpensePaymentsSql(kind: string): string {
  const sql = contract[kind];
  if (!sql) throw new Error(`Unknown native pp_expense_payments SQL kind: ${kind}`);
  return sql;
}
