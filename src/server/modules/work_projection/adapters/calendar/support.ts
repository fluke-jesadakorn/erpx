import type { CalendarAccess } from "../../domain/calendar/access";
import 'server-only';
import type { CalendarQuery } from "@/lib/work_projection/calendar";
import { visibleSourceRecord, sourceRecordInput } from "@/server/modules/records/public/source-record-policy";
import type { SqlDatabase } from "@/server/modules/runtime/public/runtime/types";
import { calendarSupportSupportFacts } from "../../../support/public/calendar-support-facts";
import { createCalendarSupport, type CalendarSupportUnit } from "../../application/calendar/support";
const service = createCalendarSupport();
export function calendarSupportUnit(db: SqlDatabase): CalendarSupportUnit {
    return { read1: facts => calendarSupportSupportFacts(db).read1(facts),
        read2: facts => calendarSupportSupportFacts(db).read2(facts),
        sourceVisible: async (actor, source) => { const row = sourceRecordInput.parse(JSON.parse(source)); return Boolean(await visibleSourceRecord(actor, { kind: row.kind, id: row.id }, db)); } };
}
export function supportCalendarItems(db: SqlDatabase, a: CalendarAccess, q: CalendarQuery, from: string, to: string, limit: number) { return service.supportCalendarItems(calendarSupportUnit(db), a, q, from, to, limit); }
