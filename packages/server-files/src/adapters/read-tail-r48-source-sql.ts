/** r48: byte-identical read-only SQL heads shared across files owner adapters.
 * Text only: every call site still prepares and executes its own statement in
 * its supplied transaction with its own binds; no SQL text is accepted from a
 * caller and no result is cached here. Advisory locks, row locks, LOCK TABLE,
 * native writes and every r1-r47 helper are left untouched. */

/** Shared scoped-existence projection over attachments / attachment_sets. The
 * divergent table-alias and WHERE tails stay at each call site, so both rendered
 * preflights remain byte-identical to their originals. */
export const ATTACHMENT_EXISTS_HEAD_SQL =
  'SELECT EXISTS(SELECT 1 FROM attachment';

/** Shared projection head of the two messaging settlement source witnesses.
 * The divergent column tails stay at each call site. */
export const MESSAGING_SETTLEMENT_HEAD_SQL =
  'SELECT l.id link_id,s.id set_id,';

/** Shared file-link projection head of the ecommerce and marketing package
 * obligation sources. The target_ identifier completes differently at each
 * call site (target_id vs target_type), so only the common prefix is folded. */
export const PACKAGE_FILE_LINK_TARGET_HEAD_SQL =
  "SELECT 'file-link' kind,l.id,s.target_";
