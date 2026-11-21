import 'server-only';
import { readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';

// SQL owned by native Rust people lifecycle consts; JSON mirror enforced by
// `people_lifecycle_sql_contract_matches_native_consts`.
const contractPath = fileURLToPath(new URL('./native-people-lifecycle-sql.json', import.meta.url));
const contract = JSON.parse(readFileSync(contractPath, 'utf8')) as Record<string, string>;

/** Native-owned SQL for a people lifecycle statement kind. */
export function nativePeopleLifecycleSql(kind: string): string {
  const sql = contract[kind];
  if (!sql) throw new Error(`Unknown native people lifecycle SQL kind: ${kind}`);
  return sql;
}
