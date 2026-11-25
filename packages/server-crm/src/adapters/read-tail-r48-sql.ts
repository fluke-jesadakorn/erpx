/** G4 / retire.crm.read-tail-r48 byte-identical company-scope head.
 *
 * The actor scope row and the obligation scope CTE share one read head: both
 * project the company bind first, then append their own column/tail. The
 * renderer below reproduces the pre-dedupe inline SQL byte-for-byte; callers
 * pass only their own tail, so rendered SQL, multiline whitespace and bind
 * order stay identical. Locks, command-receipt reads, native command/read
 * kinds, cross-owner exports and the r8-r47 families are untouched.
 */
export const crmCompanyScopeSql = (tail: string) => `SELECT ?::text company_id${tail}`;
