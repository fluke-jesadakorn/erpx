/** Byte-identical read-tail SQL fragment shared by the r27 attachment-id reads.
 * Text only: every call site still prepares and executes its own statement in
 * its supplied database/transaction with its own binds; no result is cached
 * here. Locks, advisory statements and native write boundaries are not part
 * of this family. */

/** Common read-only projection prefix of the social-draft and service-repository
 * attachment-id reads. The trailing JOIN/WHERE and its binds stay at each call
 * site, so every rendered statement is unchanged byte for byte. */
export const SELECT_ATTACHMENT_ID_SQL = 'SELECT a.id FROM attachments a';
