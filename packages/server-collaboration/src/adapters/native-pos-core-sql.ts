import 'server-only';
import { readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';

// SQL owned by native Rust POS core consts; JSON mirror enforced by
// `pos_core_sql_contract_matches_native_consts`.
const contractPath = fileURLToPath(new URL('./native-pos-core-sql.json', import.meta.url));
const contract = JSON.parse(readFileSync(contractPath, 'utf8')) as Record<string, string>;

/** Native-owned SQL for a POS core finance fact kind. */
export function nativePosCoreSql(kind: string): string {
  const sql = contract[kind];
  if (!sql) throw new Error(`Unknown native POS core SQL kind: ${kind}`);
  return sql;
}
