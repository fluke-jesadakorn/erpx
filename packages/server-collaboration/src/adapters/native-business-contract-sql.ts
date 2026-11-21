import 'server-only';
import { readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';

// SQL owned by native Rust business-contract consts; JSON mirror enforced by
// `business_contract_sql_contract_matches_native_consts`.
const contractPath = fileURLToPath(new URL('./native-business-contract-sql.json', import.meta.url));
const contract = JSON.parse(readFileSync(contractPath, 'utf8')) as Record<string, string>;

/** Native-owned SQL for a business-contract statement kind. */
export function nativeBusinessContractSql(kind: string): string {
  const sql = contract[kind];
  if (!sql) throw new Error(`Unknown native business contract SQL kind: ${kind}`);
  return sql;
}
