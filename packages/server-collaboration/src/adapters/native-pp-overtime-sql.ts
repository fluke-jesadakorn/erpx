import 'server-only';
import { readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';

// SQL owned by native Rust pp_overtime consts; JSON mirror enforced by
// `pp_overtime_sql_contract_matches_native_consts`.
const contractPath = fileURLToPath(new URL('./native-pp_overtime-sql.json', import.meta.url));
const contract = JSON.parse(readFileSync(contractPath, 'utf8')) as Record<string, string>;

/** Native-owned SQL for a pp_overtime statement kind. */
export function nativePpOvertimeSql(kind: string): string {
  const sql = contract[kind];
  if (!sql) throw new Error(`Unknown native pp_overtime SQL kind: ${kind}`);
  return sql;
}
