/** r40: byte-identical, parameterized read-only sanitization.attested witness
 * head shared by the repository set-attestation read and its binding guard.
 * Text only: each call site keeps its trailing predicate, binds and transaction
 * and renders the exact original statement byte for byte; no SQL text is
 * accepted from a caller and no result is cached here. Advisory locks, row
 * locks, native writes and every r1-r39 helper are left untouched. */

/** Common read-only attachment_events attestation witness. The trailing
 * created_at predicate and its binds stay at each call site, so both rendered
 * statements remain byte-identical to their original literals. */
export const SANITIZATION_ATTESTED_HEAD_SQL =
  "SELECT 1 FROM attachment_events attested WHERE attested.set_id=? AND attested.company_id=? AND attested.action='sanitization.attested'";
