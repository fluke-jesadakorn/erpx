import 'server-only';
import { readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';

// SQL owned by native Rust pp_people_evaluation_ai consts; JSON mirror enforced by
// `pp_people_evaluation_ai_sql_contract_matches_native_consts`.
const contractPath = fileURLToPath(new URL('./native-pp_people_evaluation_ai-sql.json', import.meta.url));
const contract = JSON.parse(readFileSync(contractPath, 'utf8')) as Record<string, string>;

/** Native-owned SQL for a pp_people_evaluation_ai statement kind. */
export function nativePpPeopleEvaluationAiSql(kind: string): string {
  const sql = contract[kind];
  if (!sql) throw new Error(`Unknown native pp_people_evaluation_ai SQL kind: ${kind}`);
  return sql;
}
