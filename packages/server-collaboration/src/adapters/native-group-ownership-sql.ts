import 'server-only';
import { readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';

// SQL owned by native Rust group-ownership consts; JSON mirror enforced by
// `group_ownership_sql_contract_matches_native_consts`.
const contractPath = fileURLToPath(new URL('./native-group-ownership-sql.json', import.meta.url));
const contract = JSON.parse(readFileSync(contractPath, 'utf8')) as Record<string, string>;

/** Native-owned SQL for a group-ownership statement kind. */
export function nativeGroupOwnershipSql(kind: string): string {
  const sql = contract[kind];
  if (!sql) throw new Error(`Unknown native group ownership SQL kind: ${kind}`);
  return sql;
}
