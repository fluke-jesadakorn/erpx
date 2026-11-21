import 'server-only';
import { readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';

// SQL owned by native Rust consolidation consts; JSON mirror enforced by
// `consolidation_sql_contract_matches_native_consts`.
const contractPath = fileURLToPath(new URL('./native-consolidation-sql.json', import.meta.url));
const contract = JSON.parse(readFileSync(contractPath, 'utf8')) as Record<string, string>;

/** Native-owned SQL for a consolidation statement kind. */
export function nativeConsolidationSql(kind: string): string {
  const sql = contract[kind];
  if (!sql) throw new Error(`Unknown native consolidation SQL kind: ${kind}`);
  return sql;
}
