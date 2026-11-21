import 'server-only';
import { readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';

// SQL owned by native Rust main finance consts; JSON mirror enforced by
// `finance_main_sql_contract_matches_native_consts`.
const contractPath = fileURLToPath(new URL('./native-finance-main-sql.json', import.meta.url));
const contract = JSON.parse(readFileSync(contractPath, 'utf8')) as Record<string, string>;

/** Native-owned SQL for a main finance statement kind. */
export function nativeFinanceMainSql(kind: string): string {
  const sql = contract[kind];
  if (!sql) throw new Error(`Unknown native main finance SQL kind: ${kind}`);
  return sql;
}
