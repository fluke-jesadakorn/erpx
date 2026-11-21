import 'server-only';
import { readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';

// SQL owned by native Rust leave self-service consts; JSON mirror enforced by
// `leave_self_service_sql_contract_matches_native_consts`.
const contractPath = fileURLToPath(new URL('./native-leave-self-service-sql.json', import.meta.url));
const contract = JSON.parse(readFileSync(contractPath, 'utf8')) as Record<string, string>;

/** Native-owned SQL for a leave self-service statement kind. */
export function nativeLeaveSelfServiceSql(kind: string): string {
  const sql = contract[kind];
  if (!sql) throw new Error(`Unknown native leave self-service SQL kind: ${kind}`);
  return sql;
}
