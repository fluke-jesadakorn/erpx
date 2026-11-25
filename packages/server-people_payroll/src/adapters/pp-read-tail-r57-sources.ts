import 'server-only';

/** Canonical parameterized read builders (G4 read-tail r57). Each builder owns one
 * byte-identical SQL head formerly repeated in inline literals at this owner's
 * call sites; the callers keep their original tails, status lists, ORDER BY and
 * bind arrays unchanged. Pure reads only: no writes, row/table locks, advisory
 * statements, native kinds or cross-owner projection tails. */

/** People-id existence guard. `tail` is the exact original predicate tail; the fixed
 * head, owner kind and bind arity stay byte-identical. */
export const ppRd57PeopleIdGuard = (tail: string) =>
  `EXISTS(SELECT 1 FROM hr_people WHERE id=?${tail})`;

/** Compensation-revision row list by engagement. `tail` is the exact original
 * predicate/ordering tail; the projection head and the caller's binds stay byte-identical. */
export const ppRd57CompRevisionByEngagementSql = (tail: string) =>
  `SELECT * FROM hr_compensation_revisions WHERE engagement_id=?${tail}`;
