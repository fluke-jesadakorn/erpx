/** r44: byte-identical, read-only SQL heads shared across files owner adapters.
 * Text only: every call site still prepares and executes its own statement in
 * its supplied database/transaction with its own binds; no SQL text is accepted
 * from a caller and no result is cached here. Advisory locks, row locks,
 * LOCK TABLE, native writes and every r1-r43 helper are left untouched. */

/** Shared upload_intents absence/existence predicate head (42 bytes). Each site
 * keeps its own NOT / OR prefix and WHERE tail, so the rendered statements stay
 * byte-identical to their originals. */
export const UPLOAD_INTENTS_EXISTS_HEAD_SQL =
  'EXISTS(SELECT 1 FROM upload_intents WHERE ';

/** Shared unqualified attachment-by-id projection head. It is deliberately not
 * r26 `attachmentsByIdCompanySql`: no company column and no caller-supplied
 * projection; each site keeps its own `AND ...` tail or ends at `id=?`. */
export const ATTACHMENT_STAR_ID_HEAD_SQL =
  'SELECT * FROM attachments WHERE id=?';

/** Shared scan-attempt OR EXISTS head of the two package/messaging witnesses.
 * It is deliberately not `scanPendingSql`: that helper renders a complete
 * predicate with its own alias and state list. Each site keeps its own alias
 * and state tail, and the LOCK TABLE statements stay at the call sites. */
export const SCAN_ATTEMPTS_OR_EXISTS_HEAD_SQL =
  'OR EXISTS(SELECT 1 FROM attachment_scan_attempts ';
