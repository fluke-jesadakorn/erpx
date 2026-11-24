/** G4 / retire.crm.read-tail-r30 byte-identical read family.
 *
 * The current-scope "allowed" selector is the read head shared by the CRM
 * analytics-access and metric-history authority probes. The renderer below
 * reproduces the pre-dedupe inline SQL byte-for-byte; callers pass their own
 * assignment expression, scope source, alias and optional predicate tail.
 * Locks, command-receipt reads, native command kinds and the r8-r29 families
 * are untouched.
 */

/** One parameterized renderer for the current assignment-scope authority read.
 * Only the assignment expression, scope source, alias and predicate tail vary;
 * the one read token lives here and renders byte-identically as both retired
 * inline literals. */
export const crmCurrentScopeAllowedSql = (
  assignment: string,
  scope: string,
  alias: string,
  tail = '',
) => `SELECT ${assignment}${tail} allowed FROM ${scope} ${alias}`;
