import { ApiError } from "@erpx/server-runtime/public/http";
import "server-only";
import { type CalendarLayer } from "@erpx/feature-work_projection/lib/calendar";
import type { WorkContext } from "@erpx/server-identity_access/public/authorization";
import type { SqlDatabase } from "@erpx/server-runtime/public/runtime/types";
import { appointmentWarningMessages } from "@erpx/server-work_projection/adapters/calendar/appointments";
import { calendarAccess, fail } from "@erpx/server-work_projection/adapters/calendar/access";
import type { CalendarLayerRequest } from "@erpx/server-work_projection/application/calendar/index";
export { calendarHierarchy } from "@erpx/server-work_projection/adapters/calendar/access";
export { calendarCommand } from "@erpx/server-work_projection/adapters/calendar/commands";
import { calendarIndexPeoplePayrollFacts } from "@erpx/server-people_payroll/public/calendar-index-facts";
import { calendarIndexWorkProjectionFacts } from "@erpx/server-work_projection/public/calendar-index-facts";
import { calendarIndexWorkflowFacts } from "@erpx/server-workflow/public/calendar-index-facts";
import { createCalendarIndex, type CalendarIndexUnit } from "@erpx/server-work_projection/application/calendar/index";
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
        case "support": return (await import("@erpx/server-work_projection/adapters/calendar/support")).supportCalendarItems(db, a, q, from, to, take);
        case "crm": return (await import("@erpx/server-work_projection/adapters/calendar/crm")).crmCalendarItems(db, a, q, from, to, take);
        case "customer_followups": return (await import("@erpx/server-work_projection/adapters/calendar/customer-followups")).customerFollowupCalendarItems(db, a, q, from, to, take);
        case "supplier_bills": return (await import("@erpx/server-work_projection/adapters/calendar/supplier-bills")).supplierBillCalendarItems(db, a, q, from, to, take);
        case "procurement_rfqs": return (await import("@erpx/server-work_projection/adapters/calendar/procurement-rfqs")).rfqCalendarItems(db, a, q, from, to, take);
        case "supplier_promises": return (await import("@erpx/server-work_projection/adapters/calendar/supplier-promises")).supplierPromiseCalendarItems(db, a, q, from, to, take);
        case "stock_transit": return (await import("@erpx/server-work_projection/adapters/calendar/transit")).transitCalendarItems(db, a, q, from, to, take);
        case "quality_holds": return (await import("@erpx/server-work_projection/adapters/calendar/quality")).qualityCalendarItems(db, a, q, from, to, take);
        case "supplier_batches": return (await import("@erpx/server-work_projection/adapters/calendar/supplier-batches")).supplierBatchCalendarItems(db, a, q, from, to, take);
        case "refunds": return (await import("@erpx/server-work_projection/adapters/calendar/refunds")).refundCalendarItems(db, a, q, from, to, take);
        case "maintenance": return (await import("@erpx/server-work_projection/adapters/calendar/maintenance")).maintenanceCalendarItems(db, a, q, fromMs, toMs, take);
        case "roster": return (await import("@erpx/server-work_projection/adapters/calendar/roster")).rosterItems(db, a, q, fromMs, toMs, take);
        case "leave": return (await import("@erpx/server-work_projection/adapters/calendar/leave")).leaveItems(db, a, q, from, to, take);
        case "appointments": return (await import("@erpx/server-work_projection/adapters/calendar/appointments")).appointmentItems(db, a, q, fromMs, toMs, take);
        case "announcements": return (await import("@erpx/server-work_projection/adapters/calendar/announcements")).announcementItems(db, a, q, from, to, take);
        case "lifecycle": return (await import("@erpx/server-work_projection/adapters/calendar/lifecycle")).lifecycleItems(db, a, q, from, to, fromMs, toMs, take);
        case "legal": return (await import("@erpx/server-legal/public/legal/integrations")).legalCalendar(db, actor, q, from, to, take);
    }
}
