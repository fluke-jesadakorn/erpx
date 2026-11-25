// Shared owner-local column-projection by-predicate family over employee_requests.
// Both Records callers render their exact prior SQL byte-for-byte; only the
// selected columns and the row predicate differ.
// r30: the employee_requests column head is admitted once and shared with the
// workflow flow-routing join family; alias, JOIN and tail bytes stay local.
import { projectionColumnHeadSql } from './read-tail-r32-sql';
export const employeeRequestSelectHead = (columns: string) => projectionColumnHeadSql(columns, 'employee_requests');
export const employeeRequestColumnsSql = (columns: string, predicate: string) =>
  `${employeeRequestSelectHead(columns)} WHERE ${predicate}`;
