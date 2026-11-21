import 'server-only';
import { readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';

// SQL owned by native Rust pp_payroll_recoveries consts; JSON mirror enforced by
// `pp_payroll_recoveries_sql_contract_matches_native_consts`.
const contractPath = fileURLToPath(new URL('./native-pp_payroll_recoveries-sql.json', import.meta.url));
const contract = JSON.parse(readFileSync(contractPath, 'utf8')) as Record<string, string>;

/** Native-owned SQL for a pp_payroll_recoveries statement kind. */
export function nativePpPayrollRecoveriesSql(kind: string): string {
  const sql = contract[kind];
  if (!sql) throw new Error(`Unknown native pp_payroll_recoveries SQL kind: ${kind}`);
  return sql;
}
