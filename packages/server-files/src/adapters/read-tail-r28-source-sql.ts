/** Byte-identical read-tail SQL fragment shared by the r28 private-image reads.
 * Text only: every call site still prepares and executes its own statement in
 * its supplied database/transaction with its own binds; no result is cached
 * here. Locks, advisory statements and native write boundaries are not part
 * of this family. */

/** Common read-only projection prefix of the three private original-image
 * sources. The trailing continuation and its binds stay at each call site, so
 * every rendered statement is unchanged byte for byte. */
export const PRIVATE_IMAGE_COLUMNS_SQL = 'SELECT a.object_key,a.checksum_sha256,a.media_type,a.bytes';
