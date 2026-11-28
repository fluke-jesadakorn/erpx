/** r49: byte-identical read-only SQL heads shared across files owner adapters.
 * Text only: every call site still prepares and executes its own statement in
 * its supplied transaction with its own binds; no SQL text is accepted from a
 * caller and no result is cached here. Advisory locks, row locks, LOCK TABLE,
 * native writes and every r1-r48 helper are left untouched. */

/** Shared sum-of-bytes projection head of the beta management facts and the
 * document derivatives storage guard. The divergent table name and tail stay
 * at each call site. */
export const SUM_BYTES_ATTACHMENT_HEAD_SQL = 'SELECT SUM(bytes) FROM attachment_';

/** Shared count head of the two leave-evidence assertions. The trailing
 * newline belongs to the head; the indentation of the following FROM line
 * differs between the two sites and stays at each call site. */
export const LEAVE_COUNT_HEAD_SQL = 'SELECT count(*)\n';

/** Shared upload-intent projection head. The service repository still composes
 * its own binds and row-lock tail, and the cleanup graph keeps its table lock
 * and later witness recheck. */
export const UPLOAD_INTENT_STAR_WHERE_HEAD_SQL = 'SELECT * FROM upload_intents WHERE ';
