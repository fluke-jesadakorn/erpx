import 'server-only';
import { readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';

// SQL owned by the native Rust asset-policy generators. The checked-in JSON
// contract mirrors Rust consts enforced by `asset_policy_sql_contract_matches_native_consts`.
const contractPath = fileURLToPath(new URL('./native-asset-policy-sql.json', import.meta.url));
const contract = JSON.parse(readFileSync(contractPath, 'utf8')) as Record<string, string>;

/** Native-owned SQL for an asset policy statement kind. */
export function nativeAssetPolicySql(kind: string): string {
  const sql = contract[kind];
  if (!sql) throw new Error(`Unknown native asset policy SQL kind: ${kind}`);
  return sql;
}
