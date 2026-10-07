// Pure owner contract: native compatibility helpers remain outside the core graph.
export { calendarLayers, calendarQuerySchema, createCalendarQuerySchema, calendarRange, itemOnDate, appointmentInput, announcementInput, calendarCommandSchema } from "@/lib/calendar-domain";
export type { CalendarLayer, CalendarQuery, CalendarNode, CalendarPerson, CalendarHistory, CalendarItem, CalendarData, CalendarHierarchy, CalendarCommand } from "@/lib/calendar-domain";
