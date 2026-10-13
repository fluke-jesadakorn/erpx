import {calendarTodayAt} from "@/server/modules/work_projection/public/rules/calendar-date-domain";
/** UI/native compatibility for current-day formatting. */
export function today(timeZone="Asia/Bangkok"){return calendarTodayAt(new Date(),timeZone);}
