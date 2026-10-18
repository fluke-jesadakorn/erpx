import type {WorkContext} from "../../../identity_access/public/authorization";
import type { CalendarAccess } from "../../domain/calendar/access";
import 'server-only';
import type { CalendarQuery } from "@/lib/work_projection/calendar";
import type { SqlDatabase } from "@/server/modules/runtime/public/runtime/types";
import { calendarCrmCrmFacts } from "../../../crm/public/calendar-crm-facts";
import { createCalendarCrm, type CalendarCrmUnit } from "../../application/calendar/crm";
const service = createCalendarCrm();
export function calendarCrmUnit(db: SqlDatabase,actor:WorkContext): CalendarCrmUnit { return { read1: facts => calendarCrmCrmFacts(db,actor).read1(facts) }; }
export function crmCalendarItems(db: SqlDatabase, a: CalendarAccess, q: CalendarQuery, from: string, to: string, limit: number) { return service.crmCalendarItems(calendarCrmUnit(db,a.actor), a, q, from, to, limit); }
