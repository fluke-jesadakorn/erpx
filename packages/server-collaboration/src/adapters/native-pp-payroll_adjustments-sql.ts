import 'server-only';
import { readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';

// SQL owned by native Rust pp_payroll_adjustments consts; JSON mirror enforced by
// `pp_payroll_adjustments_sql_contract_matches_native_consts`.
const contractPath = fileURLToPath(new URL('./native-pp_payroll_adjustments-sql.json', import.meta.url));
const contract = JSON.parse(readFileSync(contractPath, 'utf8')) as Record<string, string>;

/** Native-owned SQL for a pp_payroll_adjustments statement kind. */
export function nativePpPayrollAdjustmentsSql(kind: string): string {
  const sql = contract[kind];
  if (!sql) throw new Error(`Unknown native pp_payroll_adjustments SQL kind: ${kind}`);
  return sql;
}
