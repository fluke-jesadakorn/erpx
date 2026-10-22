import type { CalendarAccess } from "@erpx/server-work_projection/domain/calendar/access";
import 'server-only';
import type { CalendarQuery } from "@erpx/feature-work_projection/lib/calendar";
import type { SqlDatabase } from "@erpx/server-runtime/public/runtime/types";
import { calendarSupplierBatchesFinanceFacts } from "@erpx/server-finance/public/calendar-supplier-batches-facts";
import { createCalendarSupplierBatches, type CalendarSupplierBatchesUnit } from "@erpx/server-work_projection/application/calendar/supplier-batches";
const service = createCalendarSupplierBatches();
export function calendarSupplierBatchesUnit(db: SqlDatabase,actor:CalendarAccess["actor"]): CalendarSupplierBatchesUnit { return { read1: facts => calendarSupplierBatchesFinanceFacts(db,actor).read1(facts) }; }
export function supplierBatchCalendarItems(db: SqlDatabase, a: CalendarAccess, q: CalendarQuery, from: string, to: string, limit: number) { return service.supplierBatchCalendarItems(calendarSupplierBatchesUnit(db,a.actor), a, q, from, to, limit); }
