import 'server-only';
import { readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';

// SQL owned by native Rust pp_attachments_service_facts consts; JSON mirror enforced by
// `pp_attachments_service_facts_sql_contract_matches_native_consts`.
const contractPath = fileURLToPath(new URL('./native-pp_attachments_service_facts-sql.json', import.meta.url));
const contract = JSON.parse(readFileSync(contractPath, 'utf8')) as Record<string, string>;

/** Native-owned SQL for a pp_attachments_service_facts statement kind. */
export function nativePpAttachmentsServiceFactsSql(kind: string): string {
  const sql = contract[kind];
  if (!sql) throw new Error(`Unknown native pp_attachments_service_facts SQL kind: ${kind}`);
  return sql;
}
