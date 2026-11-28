/** r51: alias-parameterized read-only EXISTS head shared by the two
 * attachment-link state witnesses in the files read tail. Text only: every
 * call site still prepares and executes its own statement in its supplied
 * database/transaction with its own binds; no SQL text is accepted from a
 * caller and no result is cached here. Advisory locks, row locks, LOCK TABLE,
 * native writes and every r1-r50 helper are left untouched. */

/** Attachment-link state witness head. The two read-only EXISTS / NOT EXISTS
 * guards differ only by the closed alias union (`closed` | `l`) and by the
 * state-list/expiry tail, which stays at each call site. Rendering the head
 * with either alias reproduces the original inline literal byte for byte. */
export const LINK_STATE_EXISTS_HEAD_SQL = (alias: 'closed' | 'l') =>
  `SELECT 1 FROM attachment_links ${alias} WHERE ${alias}.attachment_id=a.id AND ${alias}.state IN (`;
