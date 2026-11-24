/** r36: byte-identical, parameterized read-only EXISTS head shared by the two
 * Field set-owner preflights (the finance payment/journal preflight and the
 * Field writer-owner preflight). Text only: every call site still prepares and
 * executes its own statement in its supplied transaction with its own binds;
 * no SQL text is accepted from a caller and no result is cached here. Advisory
 * locks, row locks, native writes and every r1-r35 helper are left untouched. */

/** Common read-only EXISTS projection over attachment_sets. The trailing owner
 * predicate stays at each call site, so every rendered statement is byte-identical
 * to its original literal. */
export const FIELD_SET_OWNER_HEAD_SQL =
  'SELECT EXISTS(SELECT 1 FROM attachment_sets s WHERE s.id=? AND ';
