import type { CalendarAccess } from "@erpx/server-work_projection/domain/calendar/access";
import 'server-only';
import type { CalendarQuery } from "@erpx/feature-work_projection/lib/calendar";
import type { SqlDatabase } from "@erpx/server-runtime/public/runtime/types";
import { calendarSupplierBillsFinanceFacts } from "@erpx/server-finance/public/calendar-supplier-bills-facts";
import { createCalendarSupplierBills, type CalendarSupplierBillsUnit } from "@erpx/server-work_projection/application/calendar/supplier-bills";
const service = createCalendarSupplierBills();
export function calendarSupplierBillsUnit(db: SqlDatabase,actor?:CalendarAccess["actor"]): CalendarSupplierBillsUnit { return { read1: facts => calendarSupplierBillsFinanceFacts(db,actor).read1(facts) }; }
export function supplierBillCalendarItems(db: SqlDatabase, a: CalendarAccess, q: CalendarQuery, from: string, to: string, limit: number) { return service.supplierBillCalendarItems(calendarSupplierBillsUnit(db,a.actor), a, q, from, to, limit); }
