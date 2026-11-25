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

/**
 * Runtime-owned canonical SQL for the closed `runtime_health` heartbeat read
 * family. App and Platform surfaces project different column subsets of the
 * same fixed read; one builder keeps the query text identical across callers.
 * No relation name or predicate is accepted from an untrusted caller.
 * Control-plane lock and migration statements are not part of this family.
 */
export const runtimeHealthSql = (columns: string, suffix = '') =>
  `SELECT ${columns} FROM runtime_health${suffix}`;

/**
 * Runtime-owned canonical SQL for the closed `executive_external_tokens` read
 * family. Token resolution projects different column subsets and secret/id
 * predicates of the same fixed read; one builder keeps the query text identical
 * across every caller. No relation name or predicate comes from an untrusted
 * caller. Control-plane lock and migration statements are not part of this family.
 */
export const executiveExternalTokensSql = (columns: string, suffix = '') =>
  `SELECT ${columns} FROM executive_external_tokens${suffix}`;

/**
 * Runtime-owned canonical SQL for the closed `executive_report_shares` read
 * family. Report access, view, ownership-candidate and authority surfaces
 * project different column subsets and predicates of the same fixed read; one
 * builder keeps the query text identical across every caller. No relation name
 * or predicate comes from an untrusted caller. Control-plane lock, guard and
 * migration statements are not part of this family.
 */
export const executiveReportSharesSql = (columns: string, suffix = '') =>
  `SELECT ${columns} FROM executive_report_shares${suffix}`;

/**
 * Runtime-owned canonical SQL for the closed `executive_report_revisions`
 * projection family. Report-view and restore surfaces project different
 * column subsets and predicates of the same fixed read; one builder keeps the
 * query text identical across every caller. No relation name or predicate
 * comes from an untrusted caller. Control-plane lock, guard and migration
 * statements are not part of this family.
 */
export const executiveReportRevisionsSql = (columns: string, suffix = '') =>
  `SELECT ${columns} FROM executive_report_revisions WHERE report_id=?${suffix}`;

/**
 * Runtime-owned canonical SQL for the closed `executive_reports` read family.
 * Report access and library surfaces project different column subsets and a
 * shared-definition join of the same fixed read; one builder keeps the query
 * text identical across every caller. No relation name or predicate comes from
 * an untrusted caller. Control-plane lock, guard and migration statements are
 * not part of this family.
 */
export const executiveReportsSql = (columns: string, suffix = '') =>
  `SELECT ${columns} FROM executive_reports${suffix}`;

// The catalog-presence and migration-control read families are now owned by
// the native control-plane read port; no TypeScript builder renders them.

