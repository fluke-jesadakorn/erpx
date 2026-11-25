import "server-only";
import {planningSelectColumnsFrom} from "./planning-select-columns-sql";
/** Owner-local production resource-calendar point lookup shared by the Planning
 * capacity reader and the resource command guard. r23 keeps this name as a thin
 * wrapper; the generalized single-line projection/FROM builder owns the single
 * read literal and emits the identical statement. Only a fixed server-owned
 * projection varies; no bind value or caller SQL is interpolated. */
export const resourceCalendarByKey = (columns: string) =>
  planningSelectColumnsFrom(columns, 'production_resource_calendar WHERE resource_id=? AND work_date=?');
