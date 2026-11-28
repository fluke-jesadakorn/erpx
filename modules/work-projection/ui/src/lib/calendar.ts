export * from "@erpx/work-projection-server/public/rules/calendar-domain";
export {today} from "@erpx/work-projection-ui/lib/calendar-date";
import {today} from "@erpx/work-projection-ui/lib/calendar-date";
import {createCalendarQuerySchema} from "@erpx/work-projection-server/public/rules/calendar-domain";
/** UI/native query compatibility; core composition supplies its date. */
export const calendarQuerySchema=createCalendarQuerySchema(()=>today());
