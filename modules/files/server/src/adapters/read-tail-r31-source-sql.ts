/** r31: parameterized SELECT-head families shared by the files read tail.
 * Text only: every call site still prepares and executes its own statement in
 * its supplied database/transaction with its own binds; no result is cached
 * here. Locks, advisory statements, native writes and every r1-r30 helper are
 * left untouched. */

/** Publication row selected by id. The company-scoped and unscoped variants
 * differ only by the optional predicate, and each rendered statement is
 * byte-identical to its original literal. The FOR UPDATE suffix stays at its
 * call site. */
export const attachmentPublicationByIdSql = (companyScoped: boolean) =>
  `SELECT * FROM attachment_publications WHERE id=?${companyScoped ? ' AND company_id=?' : ''}`;

/** Stored-ledger byte total: the WHERE predicate is caller-supplied text and
 * the rendered statement is byte-identical to each original literal. The r6
 * usedStorageSql constant is not part of this family and is left unchanged. */
export const ledgerBytesSumSql = (predicate: string) =>
  `SELECT COALESCE(sum(bytes),0) FROM attachment_storage_ledger WHERE ${predicate}`;
