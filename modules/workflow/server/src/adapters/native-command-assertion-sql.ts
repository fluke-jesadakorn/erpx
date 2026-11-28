import 'server-only';
import contract from './native-command-assertion-sql.json' with { type: 'json' };

// The native Rust generator owns the SQL. Its contract test enforces equality
// with this JSON module, which ships inside the standalone server bundle.
export function clearAssertionSql(): string {
  return contract.sql;
}
