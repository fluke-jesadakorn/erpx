import type {WorkContext} from "@erpx/server-identity_access/public/authorization";
import type { CalendarAccess } from "@erpx/server-work_projection/domain/calendar/access";
import 'server-only';
import type { CalendarQuery } from "@erpx/feature-work_projection/lib/calendar";
import type { SqlDatabase } from "@erpx/server-runtime/public/runtime/types";
import { calendarQualityQualityFacts } from "@erpx/server-quality/public/calendar-quality-facts";
import { createCalendarQuality, type CalendarQualityUnit } from "@erpx/server-work_projection/application/calendar/quality";
const service = createCalendarQuality();
export function calendarQualityUnit(db: SqlDatabase,actor?:WorkContext): CalendarQualityUnit { return { read1: facts => calendarQualityQualityFacts(db,actor).read1(facts) }; }
export function qualityCalendarItems(db: SqlDatabase, a: CalendarAccess, q: CalendarQuery, from: string, to: string, limit: number) { return service.qualityCalendarItems(calendarQualityUnit(db,a.actor), a, q, from, to, limit); }
