import 'server-only';
import { readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';

// SQL owned by native Rust pp_operations_people_lifecycle_worker_effects consts; JSON mirror enforced by
// `pp_operations_people_lifecycle_worker_effects_sql_contract_matches_native_consts`.
const contractPath = fileURLToPath(new URL('./native-pp_operations_people_lifecycle_worker_effects-sql.json', import.meta.url));
const contract = JSON.parse(readFileSync(contractPath, 'utf8')) as Record<string, string>;

/** Native-owned SQL for a pp_operations_people_lifecycle_worker_effects statement kind. */
export function nativePpOperationsPeopleLifecycleWorkerEffectsSql(kind: string): string {
  const sql = contract[kind];
  if (!sql) throw new Error(`Unknown native pp_operations_people_lifecycle_worker_effects SQL kind: ${kind}`);
  return sql;
}
