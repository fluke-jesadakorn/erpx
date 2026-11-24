import 'server-only';

/** Canonical staff-task workspace existence head shared by the records and
 *  non-records Work source guards and the handover source guard. The alias,
 *  bound id, tail predicate and optional gap after `EXISTS(` are the only
 *  varying bytes; each rendering is byte-identical to the retired inline
 *  literal. The default gap keeps the pre-existing callers unchanged. Binds
 *  and locks are unchanged. */
export const operationsStaffTaskExists = (alias: string, id: string, tail: string, openSep = '') =>
  `EXISTS(${openSep}SELECT 1 FROM staff_tasks ${alias} WHERE ${alias}.id=${id} AND ${tail})`;
