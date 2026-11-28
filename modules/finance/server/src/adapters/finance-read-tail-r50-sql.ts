/** G4 / retire.finance.read-tail-r50 safe NEW read-tail SQL families.
 *
 * Every export renders byte-identical text to the literal it replaces. Callers
 * keep their own binds, outer EXISTS, row locks, materialized clock CTEs,
 * advisory/native/write guards and prior r15-r49 helpers are untouched.
 */

/** Scalar clock probe: one predicate evaluated against one materialized clock. */
export const clockProbeSql = (predicate: string, clock: string) => `(SELECT ${predicate} FROM ${clock})`;

/** Payment-slip checksum reuse guard; callers supply both attachment joins. */
export const paymentSlipChecksumGuardSql = (usedJoin: string, proposedJoin: string) =>
  `NOT EXISTS(SELECT 1 FROM finance_payments p ${usedJoin} ON used.id=p.payment_slip_id ${proposedJoin} ON proposed.id=? WHERE p.company_id=? AND used.checksum_sha256=proposed.checksum_sha256)`;

/** A Thai document is still active: never cancelled or superseded. */
export const THAI_DOC_ACTIVE_GUARD_SQL = "NOT EXISTS(SELECT 1 FROM finance_thai_document_events WHERE document_id=? AND action IN ('cancelled','superseded'))";
