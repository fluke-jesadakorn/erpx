/** r33: byte-identical read-only EXISTS head shared by the two Field owner
 * link-existence witnesses (the finance payment/journal preflight and the Field
 * writer-owner preflight). The head is a pure EXISTS projection; every call site
 * keeps its own trailing JOIN/WHERE, binds and transaction and renders the exact
 * original statement. Text only: no SQL text is accepted from a caller and no
 * result is cached here. Advisory locks, row locks, native writes and every
 * r1-r32 helper are left untouched. */
export const FIELD_LINK_EXISTS_SQL =
  'SELECT EXISTS(SELECT 1 FROM attachment_links l LEFT JOIN attachment_sets s ON s.id=l.set_id LEFT JOIN upload_intents u ON u.id=l.upload_intent_id LEFT JOIN attachment_sets us ON us.id=u.attachment_set_id ';
