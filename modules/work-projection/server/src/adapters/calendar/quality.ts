import type {WorkContext} from "@erpx/identity-access-server/public/authorization";
import type { CalendarAccess } from "@erpx/work-projection-server/domain/calendar/access";
import 'server-only';
import type { CalendarQuery } from "@erpx/work-projection-ui/lib/calendar";
import type { SqlDatabase } from "@erpx/platform-runtime-server/public/runtime/types";
import { calendarQualityQualityFacts } from "@erpx/quality-server/public/calendar-quality-facts";
import { createCalendarQuality, type CalendarQualityUnit } from "@erpx/work-projection-server/application/calendar/quality";
const service = createCalendarQuality();
export function calendarQualityUnit(db: SqlDatabase,actor?:WorkContext): CalendarQualityUnit { return { read1: facts => calendarQualityQualityFacts(db,actor).read1(facts) }; }
export function qualityCalendarItems(db: SqlDatabase, a: CalendarAccess, q: CalendarQuery, from: string, to: string, limit: number) { return service.qualityCalendarItems(calendarQualityUnit(db,a.actor), a, q, from, to, limit); }
