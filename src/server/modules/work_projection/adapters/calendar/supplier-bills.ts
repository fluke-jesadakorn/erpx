import type { CalendarAccess } from "../../domain/calendar/access";
import 'server-only';
import type { CalendarQuery } from "@/lib/work_projection/calendar";
import type { SqlDatabase } from "@/server/modules/runtime/public/runtime/types";
import { calendarSupplierBillsFinanceFacts } from "../../../finance/public/calendar-supplier-bills-facts";
import { createCalendarSupplierBills, type CalendarSupplierBillsUnit } from "../../application/calendar/supplier-bills";
const service = createCalendarSupplierBills();
export function calendarSupplierBillsUnit(db: SqlDatabase,actor?:CalendarAccess["actor"]): CalendarSupplierBillsUnit { return { read1: facts => calendarSupplierBillsFinanceFacts(db,actor).read1(facts) }; }
export function supplierBillCalendarItems(db: SqlDatabase, a: CalendarAccess, q: CalendarQuery, from: string, to: string, limit: number) { return service.supplierBillCalendarItems(calendarSupplierBillsUnit(db,a.actor), a, q, from, to, limit); }
