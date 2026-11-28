import type { CalendarAccess } from "@erpx/work-projection-server/domain/calendar/access";
import 'server-only';
import type { CalendarQuery } from "@erpx/work-projection-ui/lib/calendar";
import type { SqlDatabase } from "@erpx/platform-runtime-server/public/runtime/types";
import { calendarSupplierBatchesFinanceFacts } from "@erpx/finance-server/public/calendar-supplier-batches-facts";
import { createCalendarSupplierBatches, type CalendarSupplierBatchesUnit } from "@erpx/work-projection-server/application/calendar/supplier-batches";
const service = createCalendarSupplierBatches();
export function calendarSupplierBatchesUnit(db: SqlDatabase,actor:CalendarAccess["actor"]): CalendarSupplierBatchesUnit { return { read1: facts => calendarSupplierBatchesFinanceFacts(db,actor).read1(facts) }; }
export function supplierBatchCalendarItems(db: SqlDatabase, a: CalendarAccess, q: CalendarQuery, from: string, to: string, limit: number) { return service.supplierBatchCalendarItems(calendarSupplierBatchesUnit(db,a.actor), a, q, from, to, limit); }
