/** r37: byte-identical, parameterized read-only projection head shared by the
 * set-scoped attachment-link owner read and its joined variant in the files
 * service repository. Text only: every call site still prepares and executes
 * its own statement in its supplied transaction with its own binds; no SQL text
 * is accepted from a caller and no result is cached here. Advisory locks, row
 * locks, native writes and every r1-r36 helper are left untouched. */

/** Common read-only projection over attachment_links joined to attachment_sets.
 * The trailing column list and predicate stay at each call site, so every
 * rendered statement is byte-identical to its original literal. */
export const ATTACHMENT_LINK_PROJECTION_HEAD_SQL =
  'SELECT l.*,s.target_type,s.target_id,s.component_kind,s.purpose_key,s.expected_target_version,';
