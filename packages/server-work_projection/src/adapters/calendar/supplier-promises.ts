import type { CalendarAccess } from "@erpx/server-work_projection/domain/calendar/access";
import 'server-only';
import type { CalendarQuery } from "@erpx/feature-work_projection/lib/calendar";
import type { SqlDatabase } from "@erpx/server-runtime/public/runtime/types";
import { calendarSupplierPromisesProcurementFacts } from "@erpx/server-procurement/public/calendar-supplier-promises-facts";
import { createCalendarSupplierPromises, type CalendarSupplierPromisesUnit } from "@erpx/server-work_projection/application/calendar/supplier-promises";
const service = createCalendarSupplierPromises();
export function calendarSupplierPromisesUnit(db: SqlDatabase,actor?:CalendarAccess["actor"]): CalendarSupplierPromisesUnit { return { read1: facts => calendarSupplierPromisesProcurementFacts(db,actor).read1(facts) }; }
export function supplierPromiseCalendarItems(db: SqlDatabase, a: CalendarAccess, q: CalendarQuery, from: string, to: string, limit: number) { return service.supplierPromiseCalendarItems(calendarSupplierPromisesUnit(db,a.actor), a, q, from, to, limit); }
