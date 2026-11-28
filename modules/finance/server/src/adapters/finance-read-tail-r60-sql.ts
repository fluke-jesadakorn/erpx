/** G4 / retire.finance.read-tail-r60 closed group-entity read-tail renderer.
 *
 * This export renders byte-identical text to the literal it replaces: the
 * projection, table, `holding_id=?` bind and raw tail are preserved. Callers
 * keep their tail text, binds and result parsing; nothing here is cached or
 * executed. Row locks, advisory/native/tax reads, r38 constants and every
 * r15-r59 helper stay inline and untouched.
 */

/** Group entities of a holding. `tail` is raw SQL after `holding_id=?`. */
export const groupEntitiesByHoldingSql = (tail: string) =>
  `SELECT * FROM finance_group_entities WHERE holding_id=?${tail}`;
