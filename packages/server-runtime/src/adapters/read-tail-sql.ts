/**
 * Runtime-owned canonical SQL for the closed `holding_bindings` read family.
 * Every caller composes the same statement from its column list and optional
 * suffix, so the query text is defined once and cannot drift between owners.
 * No relation name or predicate is accepted from an untrusted caller.
 */
export const holdingBindingsSql = (columns: string, suffix = '') =>
  `SELECT ${columns} FROM holding_bindings${suffix}`;
