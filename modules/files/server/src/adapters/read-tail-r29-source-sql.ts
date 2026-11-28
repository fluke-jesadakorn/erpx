/** Byte-identical read-tail SQL fragments shared by the r29 families.
 * Text only: every call site still prepares and executes its own statement in
 * its supplied transaction with its own binds; no result is cached here.
 * Locks, advisory statements and native write boundaries are not part of
 * these families. r10-r28 helpers are left untouched. */

/** Publication-id prefix shared by the creative share guard and the PMS
 * command fact reader. The FOR SHARE lease stays at its call site. */
export const PUBLICATION_ID_PREFIX_SQL = 'SELECT id FROM attachment_publications';

/** Stored-ledger presence prefix shared by reconciliation and upload cleanup. */
export const STORAGE_LEDGER_PRESENCE_SQL = 'SELECT 1 FROM attachment_storage_ledger WHERE company_id=?';

/** Link/set JOIN prefix shared by legal destruction and publication reads;
 * only the set alias and the trailing predicate stay at the call site. */
export const LINK_SET_JOIN_SQL = 'SELECT 1 FROM attachment_links link JOIN attachment_sets';

/** Approved-publication EXISTS prefix shared by creative, legal and rollback
 * guards; only the publication alias and tail vary per caller. */
export const PUBLICATION_EXISTS_SQL = 'EXISTS(SELECT 1 FROM attachment_publications';

/** Package-obligation file-link JOIN through the unioned publication branch.
 * The publication kind is caller-supplied text; every rendered statement keeps
 * its original bytes, binds and transaction. */
export const packageFileLinkJoinSql = (kind: string) =>
  `\n FROM sets s JOIN attachment_links l ON l.set_id=s.id LEFT JOIN attachments a ON a.id=l.attachment_id LEFT JOIN upload_intents u ON u.id=l.upload_intent_id\n UNION ALL SELECT '${kind}',`;
