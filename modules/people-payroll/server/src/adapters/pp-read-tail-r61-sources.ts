import 'server-only';

/** Canonical parameterized read builder (G4 read-tail r61). It owns the single
 * byte-identical holiday-calendars-by-company head formerly repeated in two
 * inline literals at this owner's call sites; the callers keep their exact
 * predicate/ORDER BY tail, projection and bind arity unchanged. Pure read only:
 * no writes, row/table locks, advisory statements, native kinds or cross-owner
 * tails. */

/** Holiday calendars by company list. `tail` is the exact original
 * predicate/ordering tail; the fixed head, projection and bind arity stay
 * byte-identical. */
export const ppRd61HolidayCalendarsByCompanySql = (tail: string) =>
  `SELECT id,name FROM leave_holiday_calendars WHERE company_id=?${tail}`;
