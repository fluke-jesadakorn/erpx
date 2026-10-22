import {calendarTodayAt} from "@erpx/server-work_projection/public/rules/calendar-date-domain";
/** UI/native compatibility for current-day formatting. */
export function today(timeZone="Asia/Bangkok"){return calendarTodayAt(new Date(),timeZone);}
