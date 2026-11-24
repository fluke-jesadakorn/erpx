import 'server-only';

/** Canonical staff-task workspace existence head shared by the records and
 *  non-records Work source guards. The alias, bound id and tail predicate are
 *  the only varying bytes; each rendering is byte-identical to the retired
 *  inline literal. Binds and locks are unchanged. */
export const operationsStaffTaskExists = (alias: string, id: string, tail: string) =>
  `EXISTS(SELECT 1 FROM staff_tasks ${alias} WHERE ${alias}.id=${id} AND ${tail})`;
