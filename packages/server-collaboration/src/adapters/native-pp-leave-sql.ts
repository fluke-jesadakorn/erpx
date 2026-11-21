import 'server-only';
import { readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';

// SQL owned by native Rust pp_leave consts; JSON mirror enforced by
// `pp_leave_sql_contract_matches_native_consts`.
const contractPath = fileURLToPath(new URL('./native-pp_leave-sql.json', import.meta.url));
const contract = JSON.parse(readFileSync(contractPath, 'utf8')) as Record<string, string>;

/** Native-owned SQL for a pp_leave statement kind. */
export function nativePpLeaveSql(kind: string): string {
  const sql = contract[kind];
  if (!sql) throw new Error(`Unknown native pp_leave SQL kind: ${kind}`);
  return sql;
}
