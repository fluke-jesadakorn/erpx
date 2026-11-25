import 'server-only';
import {OPERATIONS_STAFF_TASK_HEAD} from './read-tail-r53-sources';

/** Canonical staff-task workspace existence head shared by the records and
 *  non-records Work source guards and the handover source guard. The alias,
 *  bound id, tail predicate and optional gap after `EXISTS(` are the only
 *  varying bytes; each rendering is byte-identical to the retired inline
 *  literal. The default gap keeps the pre-existing callers unchanged. Binds
 *  and locks are unchanged. */
export const operationsStaffTaskExists = (alias: string, id: string, tail: string, openSep = '') =>
  `EXISTS(${openSep}${OPERATIONS_STAFF_TASK_HEAD(alias)}WHERE ${alias}.id=${id} AND ${tail})`;
