/** r47: closed-union, read-only SQL head families shared by the files read tail.
 * Text only: every call site still prepares and executes its own statement in
 * its supplied database/transaction with its own binds; no result is cached
 * here. Advisory locks, row locks, LOCK TABLE, native writes, scanner worker
 * SQL and every r1-r46 helper are left untouched. */

/** Set-row draft-link witness head shared by the orphan-cleanup EXISTS and
 * NOT EXISTS guards. Each site keeps its own EXISTS prefix, state/expires_at
 * predicate and bind; only the repeated read head is lifted here. */
export const SET_ROW_DRAFT_LINK_EXISTS_HEAD_SQL = 'SELECT 1 FROM attachment_links link WHERE link.set_id=set_row.id';

/** Social upload cleanup direct-link projection head. The two CTEs differ
 * only after `l.`; the newline-free head keeps each correlation and bind in
 * place at the call site. */
export const CLEANUP_LINK_ROWS_HEAD_SQL = 'SELECT * FROM attachment_links l WHERE l.';

/** Social upload cleanup bound-command id subquery head. The receipts and
 * audits witnesses keep their own alias and table; the shared `IN(...)` body
 * renders byte-identically from this constant. */
export const CLEANUP_BOUND_COMMAND_ID_HEAD_SQL = 'SELECT bound_command_id FROM sets)';

/** Leave-evidence legacy source-id projection head. The canonical and legacy
 * UNION ALL branches differ only after FROM; the embedded newline and indent
 * reproduce the original bytes exactly at both sites. */
export const LEAVE_LEGACY_SOURCE_ID_HEAD_SQL = 'SELECT source.id source_id\n                          FROM ';
