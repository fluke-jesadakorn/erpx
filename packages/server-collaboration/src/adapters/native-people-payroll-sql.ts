import 'server-only';
import { readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';

// SQL owned by native Rust main people payroll consts; JSON mirror enforced by
// `people_payroll_sql_contract_matches_native_consts`.
const contractPath = fileURLToPath(new URL('./native-people-payroll-sql.json', import.meta.url));
const contract = JSON.parse(readFileSync(contractPath, 'utf8')) as Record<string, string>;

/** Native-owned SQL for a main people payroll statement kind. */
export function nativePeoplePayrollSql(kind: string): string {
  const sql = contract[kind];
  if (!sql) throw new Error(`Unknown native main people payroll SQL kind: ${kind}`);
  return sql;
}
