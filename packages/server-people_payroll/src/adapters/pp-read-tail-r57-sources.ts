import 'server-only';
import { ppRdRound2cStarByOnePred } from './pp-read-tail-round2c-sources';
import { ppRdRound2dExistsById } from './pp-read-tail-round2d-sources';

/** Canonical parameterized read builders (G4 read-tail r57). The row-list head
 * now renders from the shared round-2c template; this module pins its exact
 * table/column and the callers keep their original tails, status lists, ORDER BY
 * and bind arrays unchanged. Pure reads only: no writes, row/table locks,
 * advisory statements, native kinds or cross-owner projection tails. */

/** People-id existence guard. `tail` is the exact original predicate tail; the fixed
 * head, owner kind and bind arity stay byte-identical. */
export const ppRd57PeopleIdGuard = (tail: string) =>
  ppRdRound2dExistsById('hr_people', tail);

/** Compensation-revision row list by engagement. `tail` is the exact original
 * predicate/ordering tail; the projection head and the caller's binds stay byte-identical. */
export const ppRd57CompRevisionByEngagementSql = (tail: string) =>
  ppRdRound2cStarByOnePred('hr_compensation_revisions', 'engagement_id', tail);
