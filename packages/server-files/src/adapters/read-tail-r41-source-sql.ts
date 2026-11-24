/** r41: byte-identical, read-only SQL heads shared across files owner adapters.
 * Text only: every call site still prepares and executes its own statement in
 * its supplied transaction with its own binds; no SQL text is accepted from a
 * caller and no result is cached here. Advisory locks, row locks, LOCK TABLE,
 * native writes and every r1-r40 helper are left untouched. */

/** Shared EXISTS head of the two Field set-owner marker witnesses (the finance
 * payment/journal marker and the Field writer-owner marker). The divergent
 * join/WHERE tails stay at each call site so both rendered markers are
 * byte-identical to their originals. */
export const FIELD_SET_MARKER_EXISTS_HEAD_SQL =
  ' OR EXISTS(SELECT 1 FROM attachment_links l LEFT JOIN upload_intents u ON u.id=l.upload_intent_id LEFT JOIN attachment_sets us ON us.id=u.attachment_set_id ';

/** Shared scalar owner probe head used by the Field writer-owner attachment
 * preflight and the attachment scan guard; each site keeps its own suffix and
 * binds. */
export const FIELD_ATTACHMENT_OWNED_HEAD_SQL =
  'SELECT erpx_files_field_attachment_owned(?)';

/** Shared storage-ledger object-key presence head used by the legal destruction
 * facts and the messaging upload rollback; trailing predicates stay at sites. */
export const STORAGE_LEDGER_OBJECT_KEY_HEAD_SQL =
  'SELECT 1 FROM attachment_storage_ledger WHERE object_key=?';
