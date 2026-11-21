import 'server-only';
import { readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';

// SQL owned by native Rust finance AI consts; JSON mirror enforced by
// `finance_ai_sql_contract_matches_native_consts`.
const contractPath = fileURLToPath(new URL('./native-finance-ai-sql.json', import.meta.url));
const contract = JSON.parse(readFileSync(contractPath, 'utf8')) as Record<string, string>;

/** Native-owned SQL for a finance AI statement kind. */
export function nativeFinanceAiSql(kind: string): string {
  const sql = contract[kind];
  if (!sql) throw new Error(`Unknown native finance AI SQL kind: ${kind}`);
  return sql;
}
