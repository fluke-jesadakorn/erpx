export * from "@erpx/server-work_projection/public/rules/calendar-domain";
export {today} from "@erpx/feature-work_projection/lib/calendar-date";
import {today} from "@erpx/feature-work_projection/lib/calendar-date";
import {createCalendarQuerySchema} from "@erpx/server-work_projection/public/rules/calendar-domain";
/** UI/native query compatibility; core composition supplies its date. */
export const calendarQuerySchema=createCalendarQuerySchema(()=>today());
