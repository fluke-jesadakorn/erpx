// Shared owner-local column-projection by-predicate family over employee_requests.
// Both Records callers render their exact prior SQL byte-for-byte; only the
// selected columns and the row predicate differ.
export const employeeRequestColumnsSql = (columns: string, predicate: string) =>
  `SELECT ${columns} FROM employee_requests WHERE ${predicate}`;
