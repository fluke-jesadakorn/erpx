import 'server-only';
import { ppRdRound2cStarByOnePred } from './pp-read-tail-round2c-sources';

/** Canonical parameterized read builder (G4 read-tail r58). The former owner-local
 * roster-shift head now renders from the shared round-2c template; this builder
 * pins its exact table/column and the callers keep their state predicate, ORDER BY
 * tail, projection and bind arity unchanged. Pure read only. */

/** Roster shifts by week list. `tail` is the exact original predicate/ordering
 * tail; the fixed head, projection and bind arity stay byte-identical. */
export const ppRd58RosterShiftsByWeekSql = (tail: string) =>
  ppRdRound2cStarByOnePred('people_roster_shifts', 'week_id', tail);
