import 'server-only';
import { readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';

// The SQL is owned by the native Rust generator. A checked-in JSON contract
// mirrors it, and `workflow_assertion_clear_contract_matches_native_sql` in
// notification_statement.rs enforces that the two stay identical. Reading it
// synchronously keeps clearWorkflowCommandAssertions a real Statement producer
// (required because db.batch validates `statement instanceof Statement`).
const contractPath = fileURLToPath(new URL('./native-command-assertion-sql.json', import.meta.url));
const contract = JSON.parse(readFileSync(contractPath, 'utf8')) as { sql: string };

export function clearAssertionSql(): string {
  return contract.sql;
}
