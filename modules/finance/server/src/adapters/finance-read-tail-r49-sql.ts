/** G4 / retire.finance.read-tail-r49 safe NEW read-tail SQL families.
 *
 * Every export renders byte-identical text to the literal it replaces. Callers
 * keep their own binds, row locks, result types and native write pairing;
 * nothing here is cached or executed. Tax, advisory, native, expansion unions
 * and prior r15-r48 helpers are untouched.
 */

/** One owned row by id and company. Callers append their own literal tail
 * (extra predicates or a row lock) where the original site had one. */
export const tableByIdSql = (table: string) => `SELECT * FROM ${table} WHERE id=? AND company_id=?`;

/** Correlated journal reference guard used by posting-setup integrity images. */
export const journalRefGuardSql = (ref: string, owner: string) =>
  `NOT EXISTS(SELECT 1 FROM finance_journals j WHERE j.id=${ref} AND j.company_id=${owner})`;

/** Eliminations for a consolidation run. The caller supplies the WHERE body. */
export const consolidationEliminationsSql = (where: string) =>
  `SELECT * FROM finance_consolidation_eliminations WHERE ${where} ORDER BY created_at,id`;

/** Eliminations whose run is a purchased run under the caller's history text. */
export const purchasedRunGuardSql = (history: string) =>
  `EXISTS(SELECT 1 FROM finance_consolidation_runs purchased_run WHERE purchased_run.id=finance_consolidation_eliminations.run_id AND (${history}))`;
