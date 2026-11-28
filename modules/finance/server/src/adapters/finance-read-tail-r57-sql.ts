/** G4 / retire.finance.read-tail-r57 closed read-tail renderers.
 *
 * Every export renders byte-identical text to the literal it replaces: the
 * head, output columns, table, bind order and raw tail are preserved. Callers
 * keep their tail text, binds and raw-text parsing; nothing here is cached or
 * executed. Locks, native plan kinds, row-exclusive set reads, r15-r56 helper
 * bodies and every other read-tail family stay inline and untouched.
 */

/** Customer party id/name list. `tail` is raw SQL after `company_id=?`. */
export const finPartyIdNameByCompanySql = (tail: string) =>
  `SELECT id,name FROM finance_parties WHERE company_id=?${tail}`;

/** Period id read. `tail` is raw SQL after `company_id=?`. */
export const finPeriodIdByCompanySql = (tail: string) =>
  `SELECT id FROM finance_periods WHERE company_id=?${tail}`;

/** All-party list. `tail` is raw SQL after `company_id=?`. */
export const finPartiesAllByCompanySql = (tail: string) =>
  `SELECT * FROM finance_parties WHERE company_id=?${tail}`;

/** Period existence probe. `tail` is raw SQL after `company_id=?`. */
export const finPeriodProbeByCompanySql = (tail: string) =>
  `SELECT 1 FROM finance_periods WHERE company_id=?${tail}`;
