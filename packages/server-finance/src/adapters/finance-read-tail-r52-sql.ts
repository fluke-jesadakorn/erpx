/** G4 / retire.finance.read-tail-r52 safe NEW read-tail SQL family.
 *
 * Every export renders byte-identical text to the literal it replaces. Callers
 * keep their own binds, bind order, result types and transaction; nothing here
 * is cached or executed. `status` is raw SQL text (a quoted literal or `?`) and
 * `tail` is the remaining predicate text after `status=`. Row locks, advisory
 * locks, native writes and prior r15-r51 helpers stay untouched.
 */

/** Company-scoped id/company/version/status existence guard. */
export const versionStatusGuardSql = (table: string, status: string, tail: string) =>
  `EXISTS(SELECT 1 FROM ${table} WHERE id=? AND company_id=? AND version=? AND status=${status}${tail})`;
