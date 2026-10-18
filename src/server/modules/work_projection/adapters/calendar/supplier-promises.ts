import type { CalendarAccess } from "../../domain/calendar/access";
import 'server-only';
import type { CalendarQuery } from "@/lib/work_projection/calendar";
import type { SqlDatabase } from "@/server/modules/runtime/public/runtime/types";
import { calendarSupplierPromisesProcurementFacts } from "../../../procurement/public/calendar-supplier-promises-facts";
import { createCalendarSupplierPromises, type CalendarSupplierPromisesUnit } from "../../application/calendar/supplier-promises";
const service = createCalendarSupplierPromises();
export function calendarSupplierPromisesUnit(db: SqlDatabase,actor?:CalendarAccess["actor"]): CalendarSupplierPromisesUnit { return { read1: facts => calendarSupplierPromisesProcurementFacts(db,actor).read1(facts) }; }
export function supplierPromiseCalendarItems(db: SqlDatabase, a: CalendarAccess, q: CalendarQuery, from: string, to: string, limit: number) { return service.supplierPromiseCalendarItems(calendarSupplierPromisesUnit(db,a.actor), a, q, from, to, limit); }
