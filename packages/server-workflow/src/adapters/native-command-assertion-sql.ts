import 'server-only';
import { readFileSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';

// The SQL is owned by the native Rust generator. A checked-in JSON contract
// mirrors it, and `workflow_assertion_clear_contract_matches_native_sql` in
// notification_statement.rs enforces that the two stay identical. Reading it
// synchronously keeps clearWorkflowCommandAssertions a real Statement producer
// (required because db.batch validates `statement instanceof Statement`).
// Passing the raw string keeps this working under bundlers that ship their own
// URL implementation (Turbopack), where a URL instance fails the node:url check.
const contractPath = join(
  dirname(fileURLToPath(import.meta.url)),
  './native-command-assertion-sql.json',
);
const contract = JSON.parse(readFileSync(contractPath, 'utf8')) as { sql: string };

export function clearAssertionSql(): string {
  return contract.sql;
}
