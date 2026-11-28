import type { CalendarAccess } from "@erpx/work-projection-server/domain/calendar/access";
import 'server-only';
import type { CalendarQuery } from "@erpx/work-projection-ui/lib/calendar";
import type { SqlDatabase } from "@erpx/platform-runtime-server/public/runtime/types";
import { calendarSupplierPromisesProcurementFacts } from "@erpx/procurement-server/public/calendar-supplier-promises-facts";
import { createCalendarSupplierPromises, type CalendarSupplierPromisesUnit } from "@erpx/work-projection-server/application/calendar/supplier-promises";
const service = createCalendarSupplierPromises();
export function calendarSupplierPromisesUnit(db: SqlDatabase,actor?:CalendarAccess["actor"]): CalendarSupplierPromisesUnit { return { read1: facts => calendarSupplierPromisesProcurementFacts(db,actor).read1(facts) }; }
export function supplierPromiseCalendarItems(db: SqlDatabase, a: CalendarAccess, q: CalendarQuery, from: string, to: string, limit: number) { return service.supplierPromiseCalendarItems(calendarSupplierPromisesUnit(db,a.actor), a, q, from, to, limit); }
