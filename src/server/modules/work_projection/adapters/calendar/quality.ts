import type { CalendarAccess } from "../../domain/calendar/access";
import 'server-only';
import type { CalendarQuery } from '@/lib/calendar';
import type { SqlDatabase } from "@/server/modules/runtime/public/runtime/types";
import { calendarQualityQualityFacts } from "../../../quality/public/calendar-quality-facts";
import { createCalendarQuality, type CalendarQualityUnit } from "../../application/calendar/quality";
const service = createCalendarQuality();
export function calendarQualityUnit(db: SqlDatabase): CalendarQualityUnit { return { read1: facts => calendarQualityQualityFacts(db).read1(facts) }; }
export function qualityCalendarItems(db: SqlDatabase, a: CalendarAccess, q: CalendarQuery, from: string, to: string, limit: number) { return service.qualityCalendarItems(calendarQualityUnit(db), a, q, from, to, limit); }
