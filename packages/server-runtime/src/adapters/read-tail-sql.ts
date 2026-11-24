/**
 * Runtime-owned canonical SQL for the closed `holding_bindings` read family.
 * Every caller composes the same statement from its column list and optional
 * suffix, so the query text is defined once and cannot drift between owners.
 * No relation name or predicate is accepted from an untrusted caller.
 */
export const holdingBindingsSql = (columns: string, suffix = '') =>
  `SELECT ${columns} FROM holding_bindings${suffix}`;

/**
 * Runtime-owned canonical SQL for the closed `executive_generation_jobs`
 * projection family. Owner and hospitality surfaces project different column
 * subsets of the same fixed read; one builder keeps the query text identical
 * across every caller. No relation name or predicate comes from an untrusted
 * caller. Control-plane lock and migration statements are not part of this family.
 */
export const executiveGenerationJobsSql = (columns: string, suffix = '') =>
  `SELECT ${columns} FROM executive_generation_jobs${suffix}`;
