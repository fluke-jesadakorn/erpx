/** r32: parameterized SELECT-head family shared by the files read tail.
 * The stored and destroyed ledger-row readers differ only by the state
 * predicate; each rendered statement is byte-identical to its original literal.
 * Text only: every call site still prepares and executes its own statement in
 * its supplied database with its own binds; no result is cached here. Locks,
 * advisory statements, native writes and every r1-r31 helper are left untouched. */

/** Ledger rows for one attachment, filtered by a caller-supplied state predicate. */
export const ledgerStateRowsSql = (statePredicate: string) =>
  `SELECT id,object_key,bytes,variant FROM attachment_storage_ledger WHERE attachment_id=? AND ${statePredicate} ORDER BY id`;
