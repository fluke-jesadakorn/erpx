import 'server-only';
import { readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';

// SQL owned by native Rust technology provisioning consts; JSON mirror enforced by
// `tech_provision_sql_contract_matches_native_consts`.
const contractPath = fileURLToPath(new URL('./native-tech-provision-sql.json', import.meta.url));
const contract = JSON.parse(readFileSync(contractPath, 'utf8')) as Record<string, string>;

/** Native-owned SQL for a technology provisioning statement kind. */
export function nativeTechProvisionSql(kind: string): string {
  const sql = contract[kind];
  if (!sql) throw new Error(`Unknown native tech provisioning SQL kind: ${kind}`);
  return sql;
}
