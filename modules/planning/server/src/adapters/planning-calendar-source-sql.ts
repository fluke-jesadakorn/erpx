import "server-only";
import {planningPackageFactRows} from "./planning-package-fact-sql";
/** Owner-local production resource-calendar point lookup shared by the Planning
 * capacity reader and the resource command guard. r23 keeps this name as a thin
 * wrapper; the generalized single-line projection/FROM builder owns the single
 * read literal and emits the identical statement. Only a fixed server-owned
 * projection varies; no bind value or caller SQL is interpolated. */
export const resourceCalendarByKey = (columns: string) =>
  planningPackageFactRows(columns, 'production_resource_calendar', 'resource_id=? AND work_date=?');
