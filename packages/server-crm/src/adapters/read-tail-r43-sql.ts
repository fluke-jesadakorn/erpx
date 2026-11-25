/** G4 / retire.crm.read-tail-r43 byte-identical scalar-subquery head.
 *
 * The owner-local scalar subqueries share one renderer. Callers pass only the
 * closed projection and body strings (including their own cross-owner source
 * interpolations and callers' aliases), so rendered SQL, whitespace and bind
 * order stay byte-identical to the retired inline literals. Locks, command
 * receipts, native command/read kinds and the r8-r42 families are untouched.
 */
export const crmScalarSubquerySql = (projection: string, body: string) => `(SELECT ${projection} FROM ${body})`;
