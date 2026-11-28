/** G4 / retire.finance.read-tail-r58 customer read heads.
 *
 * Each renderer returns the byte-identical head shared by its adapter sites.
 * Callers append their own tail and keep the same binds; rendered SQL is
 * unchanged. Text only: nothing is cached or executed by this module. Locks,
 * tax families, native read plans and the r54-r57 helpers stay untouched.
 */
export const finCustomerPartyIdSql = (tail: string) =>
  `SELECT id FROM finance_parties WHERE id=?${tail}`;

export const finCustomerPartyListSql = (tail: string) =>
  `SELECT id,name,currency FROM finance_parties WHERE company_id=?${tail}`;
