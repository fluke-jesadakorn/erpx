import 'server-only';
import { readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
const contractPath = fileURLToPath(new URL('./native-pp_expenses-sql.json', import.meta.url));
const contract = JSON.parse(readFileSync(contractPath, 'utf8')) as Record<string, string>;
export function nativePpExpensesSql(kind: string): string {
  const sql = contract[kind];
  if (!sql) throw new Error(`Unknown native pp_expenses SQL kind: ${kind}`);
  return sql;
}
