import 'server-only';

/** Canonical parameterized read builder (G4 read-tail r60). It owns the single
 * byte-identical pay-groups-by-company head formerly repeated in two inline
 * literals at this owner's call sites; the callers keep their exact predicate,
 * ORDER BY tail, projection and bind arity unchanged. Pure read only: no writes,
 * row/table locks, advisory statements, native kinds or cross-owner tails. */

/** Pay groups by company list. `tail` is the exact original predicate/ordering
 * tail; the fixed head, projection and bind arity stay byte-identical. */
export const ppRd60PayGroupsByCompanySql = (tail: string) =>
  `SELECT id,name,currency,frequency FROM hr_pay_groups WHERE company_id=?${tail}`;
