import 'server-only';
import { readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';

// SQL owned by native Rust pp_organization_defaults consts; JSON mirror enforced by
// `pp_organization_defaults_sql_contract_matches_native_consts`.
const contractPath = fileURLToPath(new URL('./native-pp_organization_defaults-sql.json', import.meta.url));
const contract = JSON.parse(readFileSync(contractPath, 'utf8')) as Record<string, string>;

/** Native-owned SQL for a pp_organization_defaults statement kind. */
export function nativePpOrganizationDefaultsSql(kind: string): string {
  const sql = contract[kind];
  if (!sql) throw new Error(`Unknown native pp_organization_defaults SQL kind: ${kind}`);
  return sql;
}
