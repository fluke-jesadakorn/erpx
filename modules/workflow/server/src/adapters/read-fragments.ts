import 'server-only';
import contract from '../workflow-read-fragments-sql.json' with { type: 'json' };

/**
 * Native-owned Workflow read fragments (owner tail). The exact bytes live in
 * `workflow-read-fragments-sql.json`, bound into Rust with `include_str!` by
 * `domain/read_fragments.rs`; this module only injects trusted closed
 * parameters. Cross-owner projection strings stay owned by their packages.
 */
export const fragments = contract;

/** Fill a contract template, rejecting an unknown or unresolved placeholder. */
export function fill(template: string, values: Readonly<Record<string, string>>): string {
  let sql = template;
  for (const [name, value] of Object.entries(values)) {
    const token = `{${name}}`;
    if (!sql.includes(token))
      throw new Error(`WORKFLOW_READ_FRAGMENT_PLACEHOLDER_MISSING:${name}`);
    sql = sql.split(token).join(value);
  }
  if (/\{[a-zA-Z]+\}/.test(sql))
    throw new Error('WORKFLOW_READ_FRAGMENT_PLACEHOLDER_UNRESOLVED');
  return sql;
}
