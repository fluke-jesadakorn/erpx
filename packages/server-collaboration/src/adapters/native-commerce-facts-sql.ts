import 'server-only';
import { readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';

// SQL owned by native Rust commerce fact consts; JSON mirror enforced by
// `commerce_facts_sql_contract_matches_native_consts`.
const contractPath = fileURLToPath(new URL('./native-commerce-facts-sql.json', import.meta.url));
const contract = JSON.parse(readFileSync(contractPath, 'utf8')) as Record<string, string>;

/** Native-owned SQL for a commerce finance fact kind. */
export function nativeCommerceFactsSql(kind: string): string {
  const sql = contract[kind];
  if (!sql) throw new Error(`Unknown native commerce facts SQL kind: ${kind}`);
  return sql;
}
