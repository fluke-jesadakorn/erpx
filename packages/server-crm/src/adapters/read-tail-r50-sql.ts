/** G4 / retire.crm.read-tail-r50 byte-identical parenthesized-select head.
 *
 * The attachment target witness and the marketing-assignment guard share one
 * read head: both open a parenthesized SQL expression and then
 * append their own expression, wrapper and predicate tail. The renderer below
 * reproduces the pre-dedupe inline SQL byte-for-byte; callers pass only their
 * own body, so rendered SQL, whitespace and bind order stay identical. Locks,
 * no-space probes, command-receipt reads, native command/read kinds, cross-owner
 * exports and the r8-r49 families are untouched.
 */
export const crmSelectExprSql = (body: string) => `SELECT (${body}`;
