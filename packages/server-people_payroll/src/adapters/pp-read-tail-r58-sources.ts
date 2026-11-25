import 'server-only';

/** Canonical parameterized read builder (G4 read-tail r58). It owns the single
 * byte-identical roster-shift list head formerly repeated in two inline literals
 * at this owner's call sites; the callers keep their exact state predicate,
 * ORDER BY tail, projection and bind arity unchanged. Pure read only: no writes,
 * row/table locks, advisory statements, native kinds or cross-owner tails. */

/** Roster shifts by week list. `tail` is the exact original predicate/ordering
 * tail; the fixed head, projection and bind arity stay byte-identical. */
export const ppRd58RosterShiftsByWeekSql = (tail: string) =>
  `SELECT * FROM people_roster_shifts WHERE week_id=?${tail}`;
