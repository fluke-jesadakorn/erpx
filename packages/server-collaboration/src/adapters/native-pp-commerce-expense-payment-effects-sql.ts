import 'server-only';
import { readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
const contractPath = fileURLToPath(new URL('./native-pp_commerce_expense_payment_effects-sql.json', import.meta.url));
const contract = JSON.parse(readFileSync(contractPath, 'utf8')) as Record<string, string>;
export function nativePpCommerceExpensePaymentEffectsSql(kind: string): string {
  const sql = contract[kind];
  if (!sql) throw new Error(`Unknown native pp_commerce_expense_payment_effects SQL kind: ${kind}`);
  return sql;
}
