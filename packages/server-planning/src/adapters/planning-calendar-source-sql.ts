import "server-only";
/** Owner-local parameterized production resource-calendar point lookup shared by
 * the Planning capacity reader and the resource command guard. The only varying
 * part is a fixed server-owned projection list; no bind value or caller SQL is
 * interpolated. Each emitted statement is byte-identical to the former inline
 * literal it replaces. */
export const resourceCalendarByKey = (columns: string) =>
  `SELECT ${columns} FROM production_resource_calendar WHERE resource_id=? AND work_date=?`;
