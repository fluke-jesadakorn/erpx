import 'server-only';
import { readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';

// SQL owned by native Rust company money consts; JSON mirror enforced by
// `company_money_sql_contract_matches_native_consts`.
const contractPath = fileURLToPath(new URL('./native-company-money-sql.json', import.meta.url));
const contract = JSON.parse(readFileSync(contractPath, 'utf8')) as Record<string, string>;

/** Native-owned SQL for a company money statement kind. */
export function nativeCompanyMoneySql(kind: string): string {
  const sql = contract[kind];
  if (!sql) throw new Error(`Unknown native company money SQL kind: ${kind}`);
  return sql;
}
