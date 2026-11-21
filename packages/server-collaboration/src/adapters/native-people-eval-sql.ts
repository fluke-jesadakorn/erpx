import 'server-only';
import { readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';

// SQL owned by native Rust people evaluation consts; JSON mirror enforced by
// `people_eval_sql_contract_matches_native_consts`.
const contractPath = fileURLToPath(new URL('./native-people-eval-sql.json', import.meta.url));
const contract = JSON.parse(readFileSync(contractPath, 'utf8')) as Record<string, string>;

/** Native-owned SQL for a people evaluation statement kind. */
export function nativePeopleEvalSql(kind: string): string {
  const sql = contract[kind];
  if (!sql) throw new Error(`Unknown native people evaluation SQL kind: ${kind}`);
  return sql;
}
