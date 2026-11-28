import { ApiError } from "@erpx/platform-runtime-server/public/http";
import "server-only";
import { type CalendarLayer } from "@erpx/work-projection-ui/lib/calendar";
import type { WorkContext } from "@erpx/identity-access-server/public/authorization";
import type { SqlDatabase } from "@erpx/platform-runtime-server/public/runtime/types";
import { appointmentWarningMessages } from "@erpx/work-projection-server/adapters/calendar/appointments";
import { calendarAccess, fail } from "@erpx/work-projection-server/adapters/calendar/access";
import type { CalendarLayerRequest } from "@erpx/work-projection-server/application/calendar/index";
export { calendarHierarchy } from "@erpx/work-projection-server/adapters/calendar/access";
export { calendarCommand } from "@erpx/work-projection-server/adapters/calendar/commands";
import { calendarIndexPeoplePayrollFacts } from "@erpx/people-payroll-server/public/calendar-index-facts";
import { calendarIndexWorkProjectionFacts } from "@erpx/work-projection-server/public/calendar-index-facts";
import { calendarIndexWorkflowFacts } from "@erpx/workflow-server/public/calendar-index-facts";
import { createCalendarIndex, type CalendarIndexUnit } from "@erpx/work-projection-server/application/calendar/index";
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
        case "support": return (await import("@erpx/work-projection-server/adapters/calendar/support")).supportCalendarItems(db, a, q, from, to, take);
        case "crm": return (await import("@erpx/work-projection-server/adapters/calendar/crm")).crmCalendarItems(db, a, q, from, to, take);
        case "customer_followups": return (await import("@erpx/work-projection-server/adapters/calendar/customer-followups")).customerFollowupCalendarItems(db, a, q, from, to, take);
        case "supplier_bills": return (await import("@erpx/work-projection-server/adapters/calendar/supplier-bills")).supplierBillCalendarItems(db, a, q, from, to, take);
        case "procurement_rfqs": return (await import("@erpx/work-projection-server/adapters/calendar/procurement-rfqs")).rfqCalendarItems(db, a, q, from, to, take);
        case "supplier_promises": return (await import("@erpx/work-projection-server/adapters/calendar/supplier-promises")).supplierPromiseCalendarItems(db, a, q, from, to, take);
        case "stock_transit": return (await import("@erpx/work-projection-server/adapters/calendar/transit")).transitCalendarItems(db, a, q, from, to, take);
        case "quality_holds": return (await import("@erpx/work-projection-server/adapters/calendar/quality")).qualityCalendarItems(db, a, q, from, to, take);
        case "supplier_batches": return (await import("@erpx/work-projection-server/adapters/calendar/supplier-batches")).supplierBatchCalendarItems(db, a, q, from, to, take);
        case "refunds": return (await import("@erpx/work-projection-server/adapters/calendar/refunds")).refundCalendarItems(db, a, q, from, to, take);
        case "maintenance": return (await import("@erpx/work-projection-server/adapters/calendar/maintenance")).maintenanceCalendarItems(db, a, q, fromMs, toMs, take);
        case "roster": return (await import("@erpx/work-projection-server/adapters/calendar/roster")).rosterItems(db, a, q, fromMs, toMs, take);
        case "leave": return (await import("@erpx/work-projection-server/adapters/calendar/leave")).leaveItems(db, a, q, from, to, take);
        case "appointments": return (await import("@erpx/work-projection-server/adapters/calendar/appointments")).appointmentItems(db, a, q, fromMs, toMs, take);
        case "announcements": return (await import("@erpx/work-projection-server/adapters/calendar/announcements")).announcementItems(db, a, q, from, to, take);
        case "lifecycle": return (await import("@erpx/work-projection-server/adapters/calendar/lifecycle")).lifecycleItems(db, a, q, from, to, fromMs, toMs, take);
        case "legal": return (await import("@erpx/legal-server/public/legal/integrations")).legalCalendar(db, actor, q, from, to, take);
    }
}
