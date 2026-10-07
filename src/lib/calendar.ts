export * from "@/server/modules/work_projection/public/rules/calendar-domain";
export {today} from "./calendar-date";
import {today} from "./calendar-date";
import {createCalendarQuerySchema} from "@/server/modules/work_projection/public/rules/calendar-domain";
/** UI/native query compatibility; core composition supplies its date. */
export const calendarQuerySchema=createCalendarQuerySchema(()=>today());
