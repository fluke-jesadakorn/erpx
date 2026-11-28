/** G4 / retire.finance.read-tail-r63 closed recognition-run read-tail renderer.
 *
 * This export renders byte-identical text to the literal it replaces: the
 * projection, table, `WHERE` head and raw tail are preserved. Callers keep
 * their tail text, binds and result parsing; nothing here is cached or
 * executed. Row locks, advisory/native/tax reads, r38 constants and every
 * r15-r62 helper stay inline and untouched.
 */

/** Recognition run existence by billing run. `tail` is raw SQL after `WHERE`. */
export const recognitionRunExistsSql = (tail: string) =>
  `SELECT 1 FROM finance_recognition_runs WHERE ${tail}`;
