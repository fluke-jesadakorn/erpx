import type { CalendarAccess } from "../../domain/calendar/access";
import 'server-only';
import type { CalendarQuery } from "@/lib/work_projection/calendar";
import type { SqlDatabase } from "@/server/modules/runtime/public/runtime/types";
import { calendarSupplierBatchesFinanceFacts } from "../../../finance/public/calendar-supplier-batches-facts";
import { createCalendarSupplierBatches, type CalendarSupplierBatchesUnit } from "../../application/calendar/supplier-batches";
const service = createCalendarSupplierBatches();
export function calendarSupplierBatchesUnit(db: SqlDatabase): CalendarSupplierBatchesUnit { return { read1: facts => calendarSupplierBatchesFinanceFacts(db).read1(facts) }; }
export function supplierBatchCalendarItems(db: SqlDatabase, a: CalendarAccess, q: CalendarQuery, from: string, to: string, limit: number) { return service.supplierBatchCalendarItems(calendarSupplierBatchesUnit(db), a, q, from, to, limit); }
