/** r34: byte-identical, parameterized read-head shared by the two
 * attachment-reference read witnesses (the generic service upload/destroy
 * preflight and the messaging upload rollback preflight). The head is a pure
 * EXISTS projection; every call site still prepares and executes its own
 * statement in its supplied transaction with its own binds, and no result is
 * cached here. Advisory locks, row locks, native writes and every r1-r33
 * helper are left untouched. */

/** Common read-only EXISTS projection prefix over attachments. The trailing
 * predicate and remaining EXISTS branches stay at each call site, so every
 * rendered statement is byte-identical to its original literal. */
export const ATTACHMENT_REFERENCE_HEAD_SQL =
  'SELECT 1 WHERE EXISTS(SELECT 1 FROM attachments WHERE ';
