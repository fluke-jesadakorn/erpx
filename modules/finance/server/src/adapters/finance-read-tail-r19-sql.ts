/** Byte-identical Finance read-tail SQL shared across owner-existence predicates and
 * materialized clock CTE prefixes. Text only: every call site still prepares and executes
 * with its own binds and order. Lock-taking (FOR UPDATE / FOR SHARE) variants stay inline. */

/** Root-to-owner existence join used by document-purchases and the initial-stock
 * journal sources. `alias` is one of owner_source / marker / source. */
export const rootOwnerExistsSql = (root: string, src: string, alias: string) =>
  `EXISTS(SELECT 1 FROM (${root}) root JOIN (${src}) ${alias} ON ${alias}.id=root.id AND ${alias}.company_id=root.company_id)`;

/** Materialized clock CTE prefix. `spaced` and `asColumn` flag the two byte-level
 * variants observed in the read tail (whitespace before "(", AS before now_ms); the
 * rendered text stays identical to each original call site. */
export const materializedClockCteSql = (name: string, clockSql: string, spaced = false, asColumn = false) =>
  `WITH ${name} AS MATERIALIZED${spaced ? ' ' : ''}(SELECT ${clockSql} ${asColumn ? 'AS ' : ''}now_ms)`;
