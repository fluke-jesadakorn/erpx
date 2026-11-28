/** Byte-identical read-tail SQL fragments shared by the r26 families.
 * Text only: every call site still prepares and executes its own statement in
 * its supplied database/transaction and no result is cached here. Locks,
 * advisory statements and native write boundaries are not part of these families. */

/** Preserved-object EXISTS predicate: the only slots are the ledger/derivative
 * table, its alias and the outer source reference. */
export const preservedObjectExistsSql = (table: string, alias: string, source: string) =>
  `EXISTS(SELECT 1 FROM ${table} ${alias} WHERE ${alias}.attachment_id=${source} AND ${alias}.object_key=?)`;

/** One-column to_jsonb image snapshot of a row selected by id. */
export const imageSnapshotSql = (alias: string, table: string) =>
  `SELECT to_jsonb(${alias})::text image FROM ${table} ${alias} WHERE id=?`;

/** Orphan reconciliation LEFT JOIN: a stored row with no matching attachment. */
export const orphanReconciliationSql = (table: string, alias: string) =>
`SELECT ${alias}.id FROM ${table} ${alias}
    LEFT JOIN attachments source ON source.id=${alias}.attachment_id
    WHERE source.id IS NULL OR (source.company_id IS NOT NULL AND ${alias}.company_id IS DISTINCT FROM source.company_id) ORDER BY ${alias}.id`;

/** Attachments row selected by id + company, projecting a caller-supplied column list. */
export const attachmentsByIdCompanySql = (columns: string) =>
  `SELECT ${columns} FROM attachments WHERE id=? AND company_id=?`;
