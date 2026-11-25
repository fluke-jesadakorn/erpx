import 'server-only';

/** Canonical parameterized read builder (G4 read-tail r59). It owns the single
 * byte-identical Thai payroll profile list head formerly repeated in two inline
 * literals at this owner's call sites; the callers keep their exact predicate,
 * ORDER BY tail, projection and bind arity unchanged. Pure read only: no writes,
 * row/table locks, advisory statements, native kinds or cross-owner tails. */

/** Thai payroll profiles by company list. `tail` is the exact original
 * predicate/ordering tail; the fixed head, projection and bind arity stay
 * byte-identical. */
export const ppRd59ThaiProfilesByCompanySql = (tail: string) =>
  `SELECT * FROM hr_thai_payroll_profiles WHERE company_id=?${tail}`;
