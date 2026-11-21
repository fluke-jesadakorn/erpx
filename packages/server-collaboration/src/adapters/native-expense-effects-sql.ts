import 'server-only';
import { readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';

// SQL owned by native Rust expense effect consts; JSON mirror enforced by
// `expense_effects_sql_contract_matches_native_consts`.
const contractPath = fileURLToPath(new URL('./native-expense-effects-sql.json', import.meta.url));
const contract = JSON.parse(readFileSync(contractPath, 'utf8')) as Record<string, string>;

/** Native-owned SQL for an expense effect statement kind. */
export function nativeExpenseEffectsSql(kind: string): string {
  const sql = contract[kind];
  if (!sql) throw new Error(`Unknown native expense effects SQL kind: ${kind}`);
  return sql;
}
