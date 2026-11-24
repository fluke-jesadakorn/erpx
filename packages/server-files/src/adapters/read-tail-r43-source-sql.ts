/** r43: byte-identical, read-only SQL heads shared across files owner adapters.
 * Text only: every call site still prepares and executes its own statement in
 * its supplied transaction with its own binds; no SQL text is accepted from a
 * caller and no result is cached here. Advisory locks, row locks, LOCK TABLE,
 * native writes and every r1-r42 helper are left untouched. */

/** Shared Field upload-ancestor owner witness nested in the Field set marker
 * and the Field writer upload preflight. The surrounding OR tails stay at each
 * call site, so both rendered markers remain byte-identical to their originals. */
export const FIELD_UPLOAD_ANCESTOR_EXISTS_SQL =
  'EXISTS(SELECT 1 FROM attachments a WHERE a.upload_id=u.id AND erpx_files_field_attachment_owned(a.id))';

/** Shared upload_intents OR EXISTS head of the two Field set-owner markers (the
 * Field writer marker and the finance service marker). The divergent owner and
 * WHERE tails stay at each call site. */
export const FIELD_SET_UPLOAD_EXISTS_HEAD_SQL =
  'OR EXISTS(SELECT 1 FROM upload_intents u WHERE u.attachment_set_id=s.id AND ';

/** Shared attachment id/AND guard head used by the two commerce delivery reads.
 * Each site keeps its own projection and trailing guard predicate, so both
 * rendered statements remain byte-identical. */
export const ATTACHMENT_ID_AND_HEAD_SQL =
  'FROM attachments a WHERE a.id=? AND ';
