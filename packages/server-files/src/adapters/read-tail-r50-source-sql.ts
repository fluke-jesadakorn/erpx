/** r50: byte-identical read-only SQL head shared by the leave-evidence canonical
 * projection and the generic set-row draft witness. Text only: every call site
 * still prepares and executes its own statement with its own binds; no SQL text
 * is accepted from a caller and no result is cached here. Advisory locks, row
 * locks, LOCK TABLE, native writes and every r1-r49 helper are left untouched. */

/** Shared DISTINCT set-row projection head (24 bytes). The canonical
 * leave-evidence witness completes it with `id,source.id ...` while the generic
 * set-row draft witness completes it with `company_id,set_row.target_id ...`;
 * each site keeps its own FROM/WHERE/ORDER BY tail and binds. */
export const DISTINCT_SET_ROW_HEAD_SQL = 'SELECT DISTINCT set_row.';
