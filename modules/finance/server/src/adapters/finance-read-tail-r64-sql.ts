/** G4 / retire.finance.read-tail-r64 closed instrument-run read-tail renderer.
 *
 * This export renders byte-identical text to the literal it replaces: the
 * projection, table, `WHERE` head and raw tail are preserved. Callers keep
 * their tail text, binds and result parsing; nothing here is cached or
 * executed. Row locks, advisory/native/tax reads, r38 constants and every
 * r15-r63 helper stay inline and untouched.
 */

/** Instrument-run existence by instrument. `tail` is raw SQL after `?`. */
export const instrumentRunExistsSql = (tail: string) =>
  `SELECT 1 FROM finance_instrument_runs WHERE instrument_id=?${tail}`;
