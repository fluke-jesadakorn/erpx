import 'server-only';

/** Canonical parameterized read builder (G4 read-tail r62). It owns the single
 * byte-identical hr_people email-uniqueness NOT EXISTS head formerly repeated in
 * two inline literals at this owner's call sites; the callers keep their exact
 * predicate tails and bind arrays unchanged. Pure read only: no writes, row/table
 * locks, advisory statements, native kinds or cross-owner tails. */

/** People email-uniqueness guard. `tail` is the exact original predicate tail;
 * the fixed head, closing parenthesis and bind arity stay byte-identical. */
export const ppRd62PeopleEmailGuard = (tail: string) =>
  `NOT EXISTS(SELECT 1 FROM hr_people WHERE company_id=?${tail})`;
