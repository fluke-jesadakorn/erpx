import { ApiError } from "../../../runtime/public/http";
import "server-only";
import { type CalendarLayer } from "@/lib/calendar";
import type { WorkContext } from "@/server/modules/identity_access/public/authorization";
import type { SqlDatabase } from "@/server/modules/runtime/public/runtime/types";
import { appointmentWarningMessages } from "./appointments";
import { calendarAccess, fail } from "./access";
import type { CalendarLayerRequest } from "../../application/calendar/index";
export { calendarHierarchy } from "./access";
export { calendarCommand } from "./commands";
import { calendarIndexPeoplePayrollFacts } from "../../../people_payroll/public/calendar-index-facts";
import { calendarIndexWorkProjectionFacts } from "../../../work_projection/public/calendar-index-facts";
import { calendarIndexWorkflowFacts } from "../../../workflow/public/calendar-index-facts";
import { createCalendarIndex, type CalendarIndexUnit } from "../../application/calendar/index";
const service = createCalendarIndex({ clock: { now: () => Date.now() }, error: (status, code, message) => new ApiError(status, code, message), fail, log: (...values) => console.error(...values) });
export function calendarIndexUnit(db: SqlDatabase): CalendarIndexUnit {
    return { access: actor => calendarAccess(db, actor), warnings: (...args) => appointmentWarningMessages(db, ...args), layer: (layer, input) => calendarLayer(db, layer, input), read1: facts => calendarIndexPeoplePayrollFacts(db).read1(facts),
        read2: facts => calendarIndexWorkProjectionFacts(db).read2(facts),
        read3: facts => calendarIndexPeoplePayrollFacts(db).read3(facts),
        read4: facts => calendarIndexWorkflowFacts(db).read4(facts),
        read5: facts => calendarIndexWorkProjectionFacts(db).read5(facts) };
}
export function calendarData(db: SqlDatabase, actor: WorkContext, raw: unknown) { return service.calendarData(calendarIndexUnit(db), actor, raw); }
async function calendarLayer(db: SqlDatabase, layer: CalendarLayer, input: CalendarLayerRequest) {
    const { actor, access: a, query: q, from, to, fromMs, toMs, take } = input;
    switch (layer) {
        case "support": return (await import("./support")).supportCalendarItems(db, a, q, from, to, take);
        case "crm": return (await import("./crm")).crmCalendarItems(db, a, q, from, to, take);
        case "customer_followups": return (await import("./customer-followups")).customerFollowupCalendarItems(db, a, q, from, to, take);
        case "supplier_bills": return (await import("./supplier-bills")).supplierBillCalendarItems(db, a, q, from, to, take);
        case "procurement_rfqs": return (await import("./procurement-rfqs")).rfqCalendarItems(db, a, q, from, to, take);
        case "supplier_promises": return (await import("./supplier-promises")).supplierPromiseCalendarItems(db, a, q, from, to, take);
        case "stock_transit": return (await import("./transit")).transitCalendarItems(db, a, q, from, to, take);
        case "quality_holds": return (await import("./quality")).qualityCalendarItems(db, a, q, from, to, take);
        case "supplier_batches": return (await import("./supplier-batches")).supplierBatchCalendarItems(db, a, q, from, to, take);
        case "refunds": return (await import("./refunds")).refundCalendarItems(db, a, q, from, to, take);
        case "maintenance": return (await import("./maintenance")).maintenanceCalendarItems(db, a, q, fromMs, toMs, take);
        case "roster": return (await import("./roster")).rosterItems(db, a, q, fromMs, toMs, take);
        case "leave": return (await import("./leave")).leaveItems(db, a, q, from, to, take);
        case "appointments": return (await import("./appointments")).appointmentItems(db, a, q, fromMs, toMs, take);
        case "announcements": return (await import("./announcements")).announcementItems(db, a, q, from, to, take);
        case "lifecycle": return (await import("./lifecycle")).lifecycleItems(db, a, q, from, to, fromMs, toMs, take);
        case "legal": return (await import("../../../legal/public/legal/integrations")).legalCalendar(db, actor, q, from, to, take);
    }
}
