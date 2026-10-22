import { z } from 'zod';
import { filtersSchema, type MetricId } from "@erpx/server-runtime/public/rules/canvas-domain";
import type { CustomerActivityItem } from '@erpx/server-customer_360/public/customer-timeline';
import type { SocialActivityEvent } from '@erpx/server-collaboration/public/social-activity';
import { BusinessRuleError } from '@erpx/server-shared/index';
import { operationsMetrics, type OperationsMetricId } from '@erpx/server-runtime/domain/executive/operations-metric-ids';
import type { maintenanceDowntimeEvidenceRows } from '@erpx/server-maintenance/public/asset-maintenance';
export const providerId = "finance-invoices";
export const payrollSummaryMetric = "payroll-summary" as const;
export const payrollSummaryProviderId = "people-payroll-posted-runs";
export const payrollSummaryFields = ["period_start", "period_end", "pay_date", "currency", "status", "version", "net_minor"];
export const approvedLeaveMetric = "approved-leave-duration" as const;
export const authorizedOvertimeMetric = "authorized-overtime-duration" as const;
export const peopleLeaveOvertimeProviderId = "people-leave-overtime-requests";
export const approvedLeaveFields = ["number", "start_date", "end_date", "calculated_units", "days", "state", "version"];
export const authorizedOvertimeFields = ["handoff.work_date", "handoff.minutes", "handoff.request_version", "state", "version"];
export const hotelOccupancyMetric = "hotel-occupancy" as const;
export const hotelOccupancyProviderId = "pms-night-audit";
export const hotelOccupancyFields = ["property.id", "property.name", "property.currency", "night.date", "night.state", "night.roomMetrics.sold", "night.roomMetrics.capacity", "night.version"];
export const hotelRoomRevenueMetrics = ["hotel-adr", "hotel-revpar"] as const;
export type HotelRoomRevenueMetric = typeof hotelRoomRevenueMetrics[number];
export const hotelRoomRevenueFields = [...hotelOccupancyFields, "night.roomRevenueMinor", "entry.kind", "entry.date", "entry.net", "entry.sourceKey", "entry.originalId"];
export const hotelGuestAccountBalanceMetric = "hotel-guest-account-balance" as const;
export const hotelGuestAccountBalanceProviderId = "pms-open-guestAccounts";
export const hotelGuestAccountBalanceFields = ["guest_account.id", "guest_account.state", "guest_account.version", "property.name", "property.currency", "posted_receivable_balance_minor"];
export const hotelNightAuditBlockersMetric = "hotel-night-audit-blockers" as const;
export const hotelNightAuditBlockersProviderId = "pms-night-audit-blockers";
export const hotelNightAuditBlockersFields = ["property.id", "property.name", "property.branch_id", "job.business_date", "job.state", "job.error_code", "night.version"];
export const procurementProviderId = "procurement-supplier-bills";
export const salesProviderId = "commerce-sales-orders";
export const qualityProviderId = "quality-records";
export const financeMetrics = ["receivables", "payables", "invoiced-sales"] as const;
export type FinanceMetricId = typeof financeMetrics[number];
export const salesMetrics = ["sales-orders", "sales-cancellations", "sales-cancellation-value"] as const;
export type SalesMetricId = typeof salesMetrics[number];
export const salesFulfillmentMetric = "sales-fulfillment-rate" as const;
export const fieldVisitMetric = "field-visit-completion" as const;
export const manufacturingCostMetric = "production-cost" as const;
export const customerActivityMetric = "recent-customer-activity" as const;
export const socialActivityMetric = "collaboration-thread-activity" as const;
export const legalMetrics = ["active-legal-obligations", "active-legal-holds"] as const;
export type LegalMetricId = typeof legalMetrics[number];
export const legalProviderId = "legal-obligations-holds";
export const maintenanceDowntimeMetric = "asset-downtime" as const;
export const maintenanceProviderId = "maintenance-downtime-work-orders";
export const supportIncidentMetric = "active-support-incidents" as const;
export const supportIncidentProviderId = "support-incidents";
export const supportIncidentFields = ["title", "severity", "status", "due_date", "created_at", "updated_at", "version"];
export const supportTicketMetrics = ["open-support-tickets", "overdue-support-sla"] as const;
export type SupportTicketMetricId = typeof supportTicketMetrics[number];
export const supportTicketProviderId = "support-tickets";
export const supportTicketFields = ["title", "status", "due_date", "updated_at", "version"];
export const supportTicketSlaFields = [...supportTicketFields, "sla_clock_json.state", "sla_clock_json.dueAt"];
export const duplicateReviewMetric = "duplicate-reviews-pending" as const;
export const duplicateReviewProviderId = "customer-duplicate-reviews";
export const duplicateReviewFields = ["left_customer.name", "left_customer.code", "right_customer.name", "right_customer.code", "status", "created_at", "updated_at", "version"];
export const closeBlockersMetric = "close-blockers" as const;
export const closeBlockersProviderId = "finance-close-checks";
export const closeBlockersFields = ["check_key", "count_value", "status", "checked_at", "source_version", "period.name", "period.end_date", "period.state"];
export const identityChangeMetric = "identity-change-events" as const;
export const identityChangeProviderId = "customer-identity-events";
export const identityChangeFields = ["action", "created_at", "version", "customer_version", "customer.name", "customer.code", "change.status", "change.version"];
export const organizationNodeMetric = "active-organization-nodes" as const;
export const pendingStructureMetric = "pending-structure-changes" as const;
export const organizationProviderId = "organization-records";
export const organizationNodeFields = ["kind", "slug", "display_name", "status", "branch.name", "parent.name", "hierarchy_revision"];
export const pendingStructureFields = ["action", "node_kind", "display_name", "state", "version", "created_at", "updated_at", "branch.name"];
export const activeAssignmentsMetric = "active-assignments" as const;
export const pendingAccessReviewsMetric = "pending-access-reviews" as const;
export const activeAssignmentsProviderId = "identity-access-assignments";
export const pendingAccessReviewsProviderId = "identity-access-reviews";
export const activeSubscriptionsMetric = "active-subscriptions" as const;
export const overdueInvoicesMetric = "overdue-invoices" as const;
export const activeSubscriptionsProviderId = "commercial-subscriptions";
export const overdueInvoicesProviderId = "commercial-overdue-invoices";
export const activeSubscriptionFields = ["plan.name", "status", "started_at", "effective_at", "billing_accounts.version"];
export const legacySubscriptionFields = ["status", "current_period_end", "updated_at"];
export const overdueInvoiceFields = ["kind", "status", "due_at", "total_satang", "updated_at", "billing_periods.month"];
export const activeAssignmentFields = ["user.id", "user.name", "role", "role.name", "scope_kind", "scope_node_id", "scope.name", "branch.name", "source", "valid_from", "valid_until", "version"];
export const pendingAccessReviewFields = ["campaign.name", "campaign.status", "campaign.due_at", "campaign.version", "user.id", "user.name", "role", "scope_kind", "scope_node_id", "scope.name", "branch.name", "source", "valid_until", "assignment_version", "decision"];
export const maintenanceSearchFields = ["title", "status", "asset_code", "asset_name", "downtime_started_at", "downtime_ended_at", "version"];
export const maintenanceDetailFields = maintenanceSearchFields;
export const qualityMetrics = ["quality-holds", "quality-releases", "inspection-pass-rate"] as const;
export const searchableMetrics = new Set<MetricId>(["receivables", "payables", "invoiced-sales"]);
export const searchFields = ["invoice_number", "kind", "invoice_date", "due_date", "currency", "total_minor", "outstanding_minor", "status", "version"];
export const detailFields = [...searchFields, "subtotal_minor", "tax_minor", "posted_at", "party_name"];
export const procurementSearchFields = ["reference", "supplier_name", "order_number", "invoice_date", "due_date", "currency", "subtotal_minor", "tax_minor", "status", "version"];
export const procurementDetailFields = [...procurementSearchFields, "expected_subtotal_minor", "quantity_matches", "fulfillment_version", "receipt_count"];
export const salesSearchFields = ["number", "order_date", "cancelled_at", "currency", "total_minor", "status", "version", "prior_approval"];
export const salesDetailFields = [...salesSearchFields, "subtotal_minor", "tax_minor"];
export const salesOrderSearchFields = ["number", "order_date", "currency", "subtotal_minor", "tax_minor", "total_minor", "status", "version"];
export const salesOrderDetailFields = salesOrderSearchFields;
export const commonFilters = { filters: filtersSchema, query: z.string().trim().max(100).default(""), cursor: z.string().max(2000).optional(), limit: z.coerce.number().int().min(1).max(50).default(25) };
export const querySchema = z.object({
    metric: z.enum(financeMetrics),
    ...commonFilters,
}).strict();
export const procurementQuerySchema = z.object({ metric: z.literal("invoice-match-exceptions"), ...commonFilters }).strict();
export const salesQuerySchema = z.object({ metric: z.enum(salesMetrics), ...commonFilters }).strict();
export const salesFulfillmentQuerySchema = z.object({ metric: z.literal(salesFulfillmentMetric), ...commonFilters }).strict();
export const fieldVisitQuerySchema = z.object({ metric: z.literal(fieldVisitMetric), ...commonFilters }).strict();
export const manufacturingCostQuerySchema = z.object({ metric: z.literal(manufacturingCostMetric), ...commonFilters }).strict();
export const qualityQuerySchema = z.object({ metric: z.enum(qualityMetrics), ...commonFilters }).strict();
export const customerActivityQuerySchema = z.object({ metric: z.literal(customerActivityMetric), ...commonFilters }).strict();
export const socialActivityQuerySchema = z.object({ metric: z.literal(socialActivityMetric), ...commonFilters }).strict();
export const legalQuerySchema = z.object({ metric: z.enum(legalMetrics), ...commonFilters }).strict();
export const maintenanceQuerySchema = z.object({ metric: z.literal(maintenanceDowntimeMetric), ...commonFilters }).strict();
export const supportIncidentQuerySchema = z.object({ metric: z.literal(supportIncidentMetric), ...commonFilters }).strict();
export const supportTicketQuerySchema = z.object({ metric: z.enum(supportTicketMetrics), ...commonFilters }).strict();
export const duplicateReviewQuerySchema = z.object({ metric: z.literal(duplicateReviewMetric), ...commonFilters }).strict();
export const closeBlockersQuerySchema = z.object({ metric: z.literal(closeBlockersMetric), ...commonFilters }).strict();
export const identityChangeQuerySchema = z.object({ metric: z.literal(identityChangeMetric), ...commonFilters }).strict();
export const organizationNodeQuerySchema = z.object({ metric: z.literal(organizationNodeMetric), ...commonFilters }).strict();
export const pendingStructureQuerySchema = z.object({ metric: z.literal(pendingStructureMetric), ...commonFilters }).strict();
export const activeAssignmentsQuerySchema = z.object({ metric: z.literal(activeAssignmentsMetric), ...commonFilters }).strict();
export const pendingAccessReviewsQuerySchema = z.object({ metric: z.literal(pendingAccessReviewsMetric), ...commonFilters }).strict();
export const activeSubscriptionsQuerySchema = z.object({ metric: z.literal(activeSubscriptionsMetric), ...commonFilters }).strict();
export const overdueInvoicesQuerySchema = z.object({ metric: z.literal(overdueInvoicesMetric), ...commonFilters }).strict();
export const payrollSummaryQuerySchema = z.object({ metric: z.literal(payrollSummaryMetric), ...commonFilters }).strict();
export const peopleLeaveOvertimeQuerySchema = z.object({ metric: z.enum([approvedLeaveMetric, authorizedOvertimeMetric]), ...commonFilters }).strict();
export const hotelOccupancyQuerySchema = z.object({ metric: z.enum([hotelOccupancyMetric, ...hotelRoomRevenueMetrics]), ...commonFilters }).strict();
export const hotelGuestAccountBalanceQuerySchema = z.object({ metric: z.literal(hotelGuestAccountBalanceMetric), ...commonFilters }).strict();
export const hotelNightAuditBlockersQuerySchema = z.object({ metric: z.literal(hotelNightAuditBlockersMetric), ...commonFilters }).strict();
export const availableStockMetric = "available-stock" as const;
export const availableStockProviderId = "inventory-available-stock";
export const availableStockFields = ["sku", "product_name", "unit", "warehouse_name", "on_hand_milli", "reserved_milli", "quarantined_milli", "available_milli", "stock_version"];
export const availableStockQuerySchema = z.object({ metric: z.literal(availableStockMetric), ...commonFilters }).strict();
export const inventoryShortagesMetric = "inventory-shortages" as const;
export const inventoryShortagesProviderId = "inventory-reorder-shortages";
export const inventoryShortagesFields = ["sku", "product_name", "unit", "warehouse_name", "on_hand_milli", "reserved_milli", "quarantined_milli", "available_milli", "approved_reorder_point_milli", "policy_version", "stock_version"];
export const inventoryShortagesQuerySchema = z.object({ metric: z.literal(inventoryShortagesMetric), ...commonFilters }).strict();
export const stockedLocationsMetric = "inventory" as const;
export const stockedLocationsQuerySchema = z.object({ metric: z.literal(stockedLocationsMetric), ...commonFilters }).strict();
export const inventoryMovementsMetric = "inventory-movements" as const;
export const inventoryMovementsProviderId = "inventory-movements";
export const inventoryMovementFields = ["movement_date", "source_type", "source_number", "sku", "product_name", "unit", "warehouse_name", "quantity_milli"];
export const inventoryMovementsQuerySchema = z.object({ metric: z.literal(inventoryMovementsMetric), ...commonFilters }).strict();
export const operationsProviderId = "operations-staff-tasks";
export const operationsFields = ["title", "state", "due_at", "created_at", "updated_at", "version", "branch_id", "department_id"];
export const operationsEscalationFields = [...operationsFields, "staff_task_alert.created_at"];
export const operationsResolutionFields = [...operationsFields, "overdue_duration_ms"];
export const operationsQuerySchema = z.object({ metric: z.enum(operationsMetrics), ...commonFilters }).strict();
export const projectMilestoneMetric = "overdue-project-milestones" as const;
export const projectMilestoneProviderId = "projects-overdue-milestones";
export const projectMilestoneFields = ["title", "project.title", "due_at", "owner_user_id", "owner_name", "state", "version"];
export const projectMilestoneQuerySchema = z.object({ metric: z.literal(projectMilestoneMetric), ...commonFilters }).strict();
export const projectDependencyMetric = "project-dependency-blockers" as const;
export const projectDependencyProviderId = "projects-milestone-dependencies";
export const projectDependencyFields = ["project.title", "milestone.title", "milestone.due_at", "milestone.state", "milestone.version", "prerequisite.id", "prerequisite.title", "prerequisite.state", "prerequisite.version"];
export const projectDependencyQuerySchema = z.object({ metric: z.literal(projectDependencyMetric), ...commonFilters }).strict();
export const projectActualMetrics = ["project-actual-hours", "project-posted-cost"] as const;
export type ProjectActualMetric = typeof projectActualMetrics[number];
export const projectActualProviderId = "projects-approved-actuals";
export const projectActualHoursFields = ["project.title", "work_date", "minutes", "state", "version", "user_id"];
export const projectPostedCostFields = ["project.title", "journal.number", "journal.entry_date", "journal.currency", "journal.status", "account.code", "account.name", "amount_minor", "allocation.state", "allocation.version"];
export const projectActualQuerySchema = z.object({ metric: z.enum(projectActualMetrics), ...commonFilters }).strict();
export type EvidenceSearchInput = z.input<typeof querySchema>;
export type InvoiceEvidenceRow = {
    id: string;
    invoice_number: string;
    kind: "receivable" | "payable";
    invoice_date: string;
    due_date: string;
    currency: string;
    total_minor: string;
    outstanding_minor: string;
    status: string;
    version: number;
};
export function predicate(metric: FinanceMetricId) {
    if (metric === "payables")
        return { kind: "payable", statuses: ["posted", "partially_paid"] };
    if (metric === "receivables")
        return { kind: "receivable", statuses: ["posted", "partially_paid"] };
    return { kind: "receivable", statuses: ["posted", "partially_paid", "paid"] };
}
export function cursorValue(cursor?: string) {
    if (!cursor)
        return null;
    try {
        const value = JSON.parse(Buffer.from(cursor, "base64url").toString("utf8")) as {
            invoiceDate?: unknown;
            id?: unknown;
        };
        if (typeof value.invoiceDate !== "string" || typeof value.id !== "string")
            throw new Error();
        return { invoiceDate: z.string().date().parse(value.invoiceDate), id: z.string().min(1).max(200).parse(value.id) };
    }
    catch {
        throw new BusinessRuleError(422, "EVIDENCE_CURSOR_INVALID", "The source record cursor is invalid. Refresh the evidence list.");
    }
}
export function encodeCursor(row: Pick<InvoiceEvidenceRow, "invoice_date" | "id">) {
    return Buffer.from(JSON.stringify({ invoiceDate: row.invoice_date, id: row.id })).toString("base64url");
}
export type LegalEvidenceRow = {
    id: string;
    item_id: string;
    title: string;
    category: string;
    due_date: string | null;
    state: string;
    version: number;
    item_title: string;
    item_kind: string;
    item_version: number;
};
export type LegalHoldEvidenceRow = {
    id: string;
    item_id: string;
    state: string;
    created_at: number;
    item_title: string;
    item_kind: string;
    item_version: number;
};
export function legalEvidenceCursor(cursor?: string) {
    if (!cursor)
        return null;
    try {
        const value = JSON.parse(Buffer.from(cursor, "base64url").toString("utf8")) as {
            id?: unknown;
        };
        return { id: z.string().min(1).max(200).parse(value.id) };
    }
    catch {
        throw new BusinessRuleError(422, "EVIDENCE_CURSOR_INVALID", "The source record cursor is invalid. Refresh the evidence list.");
    }
}
export function encodeLegalEvidenceCursor(id: string) {
    return Buffer.from(JSON.stringify({ id })).toString("base64url");
}
export type PayrollSummaryEvidenceRow = {
    id: string;
    period_start: string;
    period_end: string;
    pay_date: string;
    currency: string;
    status: string;
    version: number;
    net_minor: string;
};
export function payrollSummaryCursor(cursor?: string) {
    if (!cursor)
        return null;
    try {
        const value = JSON.parse(Buffer.from(cursor, "base64url").toString("utf8")) as {
            periodEnd?: unknown;
            id?: unknown;
        };
        return { periodEnd: z.string().date().parse(value.periodEnd), id: z.string().min(1).max(200).parse(value.id) };
    }
    catch {
        throw new BusinessRuleError(422, "EVIDENCE_CURSOR_INVALID", "The source record cursor is invalid. Refresh the evidence list.");
    }
}
export function encodePayrollSummaryCursor(row: Pick<PayrollSummaryEvidenceRow, "period_end" | "id">) {
    return Buffer.from(JSON.stringify({ periodEnd: row.period_end, id: row.id })).toString("base64url");
}
export function peopleRequestCursor(cursor?: string) {
    if (!cursor)
        return null;
    try {
        const value = JSON.parse(Buffer.from(cursor, "base64url").toString("utf8")) as {
            date?: unknown;
            id?: unknown;
        };
        return { date: z.string().date().parse(value.date), id: z.string().min(1).max(200).parse(value.id) };
    }
    catch {
        throw new BusinessRuleError(422, "EVIDENCE_CURSOR_INVALID", "The source record cursor is invalid. Refresh the evidence list.");
    }
}
export function encodePeopleRequestCursor(date: string, id: string) {
    return Buffer.from(JSON.stringify({ date, id })).toString("base64url");
}
export type HotelOccupancyEvidenceRow = {
    id: string;
    property_id: string;
    property_name: string;
    property_currency: string;
    business_date: string;
    state: string;
    capacity: string;
    sold: string;
    version: number;
};
export function hotelOccupancyCursorValue(cursor?: string) {
    if (!cursor)
        return null;
    try {
        const value = JSON.parse(Buffer.from(cursor, "base64url").toString("utf8")) as {
            businessDate?: unknown;
            id?: unknown;
        };
        return { businessDate: z.string().date().parse(value.businessDate), id: z.string().min(1).max(200).parse(value.id) };
    }
    catch {
        throw new BusinessRuleError(422, "EVIDENCE_CURSOR_INVALID", "The source record cursor is invalid. Refresh the evidence list.");
    }
}
export function encodeHotelOccupancyCursor(row: Pick<HotelOccupancyEvidenceRow, "business_date" | "id">) {
    return Buffer.from(JSON.stringify({ businessDate: row.business_date, id: row.id })).toString("base64url");
}
export type HotelGuestAccountBalanceEvidenceRow = {
    id: string;
    property_id: string;
    property_name: string;
    currency: string;
    state: "open" | "credit";
    balance_minor: string;
    version: number;
};
export function hotelGuestAccountBalanceCursorValue(cursor?: string) {
    if (!cursor)
        return null;
    try {
        const value = JSON.parse(Buffer.from(cursor, "base64url").toString("utf8")) as {
            propertyId?: unknown;
            id?: unknown;
        };
        return { propertyId: z.string().min(1).max(200).parse(value.propertyId), id: z.string().min(1).max(200).parse(value.id) };
    }
    catch {
        throw new BusinessRuleError(422, "EVIDENCE_CURSOR_INVALID", "The source record cursor is invalid. Refresh the evidence list.");
    }
}
export function encodeHotelGuestAccountBalanceCursor(row: Pick<HotelGuestAccountBalanceEvidenceRow, "property_id" | "id">) {
    return Buffer.from(JSON.stringify({ propertyId: row.property_id, id: row.id })).toString("base64url");
}
export type HotelNightAuditBlockerEvidenceRow = {
    id: string;
    property_id: string;
    property_name: string;
    business_date: string;
    state: "blocked";
    error_code: string;
    version: number;
};
export function hotelNightAuditBlockerCursorValue(cursor?: string) {
    if (!cursor)
        return null;
    try {
        const value = JSON.parse(Buffer.from(cursor, "base64url").toString("utf8")) as {
            businessDate?: unknown;
            id?: unknown;
        };
        return { businessDate: z.iso.date().parse(value.businessDate), id: z.string().min(1).max(200).parse(value.id) };
    }
    catch {
        throw new BusinessRuleError(422, "EVIDENCE_CURSOR_INVALID", "The source record cursor is invalid. Refresh the evidence list.");
    }
}
export function encodeHotelNightAuditBlockerCursor(row: Pick<HotelNightAuditBlockerEvidenceRow, "business_date" | "id">) {
    return Buffer.from(JSON.stringify({ businessDate: row.business_date, id: row.id })).toString("base64url");
}
export type SupplierBillEvidenceRow = {
    id: string;
    reference: string;
    invoice_date: string;
    due_date: string;
    currency: string;
    total_minor: string;
    status: string;
    version: number;
    supplier_name: string;
    order_number: string;
    expected_subtotal_minor: string;
    subtotal_minor: string;
    tax_minor: string;
    quantity_matches: boolean;
};
export type SalesOrderEvidenceRow = {
    id: string;
    number: string;
    order_date: string;
    updated_at: number | string;
    currency: string;
    total_minor: string;
    subtotal_minor: string;
    tax_minor: string;
    status: string;
    version: number;
    prior_approval: boolean;
};
export type SalesEvidenceCursor = {
    cancelledAt: number;
    id: string;
} | {
    orderDate: string;
    id: string;
};
export function salesCursorValue(cursor?: string, metric?: SalesMetricId): SalesEvidenceCursor | null {
    if (!cursor)
        return null;
    try {
        const value = JSON.parse(Buffer.from(cursor, "base64url").toString("utf8")) as {
            cancelledAt?: unknown;
            orderDate?: unknown;
            id?: unknown;
        };
        const id = z.string().min(1).max(200).parse(value.id);
        if (metric === "sales-orders") {
            if (typeof value.orderDate !== "string")
                throw new Error();
            return { orderDate: z.iso.date().parse(value.orderDate), id };
        }
        if (typeof value.cancelledAt !== "number" || !Number.isSafeInteger(value.cancelledAt))
            throw new Error();
        return { cancelledAt: value.cancelledAt, id };
    }
    catch {
        throw new BusinessRuleError(422, "EVIDENCE_CURSOR_INVALID", "The source record cursor is invalid. Refresh the evidence list.");
    }
}
export function encodeSalesCursor(row: Pick<SalesOrderEvidenceRow, "order_date" | "updated_at" | "id">, metric: SalesMetricId) {
    return Buffer.from(JSON.stringify(metric === "sales-orders"
        ? { orderDate: row.order_date, id: row.id }
        : { cancelledAt: Number(row.updated_at), id: row.id })).toString("base64url");
}
export type SalesFulfillmentEvidenceRow = {
    id: string;
    number: string;
    order_date: string;
    status: string;
    version: number;
};
export const salesFulfillmentFields = ["number", "order_date", "status", "version"];
export function salesFulfillmentCursorValue(cursor?: string) {
    if (!cursor)
        return null;
    try {
        const value = JSON.parse(Buffer.from(cursor, "base64url").toString("utf8")) as {
            orderDate?: unknown;
            id?: unknown;
        };
        if (typeof value.orderDate !== "string" || typeof value.id !== "string")
            throw new Error();
        return { orderDate: z.string().date().parse(value.orderDate), id: z.string().min(1).max(200).parse(value.id) };
    }
    catch {
        throw new BusinessRuleError(422, "EVIDENCE_CURSOR_INVALID", "The source record cursor is invalid. Refresh the evidence list.");
    }
}
export function encodeSalesFulfillmentCursor(row: Pick<SalesFulfillmentEvidenceRow, "order_date" | "id">) {
    return Buffer.from(JSON.stringify({ orderDate: row.order_date, id: row.id })).toString("base64url");
}
export type FieldVisitEvidenceRow = {
    id: string;
    scheduled_at: number | string;
    visit_completed_at: number | string | null;
    status: string;
    version: number;
};
export const fieldVisitFields = ["scheduled_at", "visit_completed_at", "status", "version"];
export function fieldVisitCursorValue(cursor?: string) {
    if (!cursor)
        return null;
    try {
        const value = JSON.parse(Buffer.from(cursor, "base64url").toString("utf8")) as {
            scheduledAt?: unknown;
            id?: unknown;
        };
        if (typeof value.scheduledAt !== "number" || !Number.isSafeInteger(value.scheduledAt) || typeof value.id !== "string")
            throw new Error();
        return { scheduledAt: value.scheduledAt, id: z.string().min(1).max(200).parse(value.id) };
    }
    catch {
        throw new BusinessRuleError(422, "EVIDENCE_CURSOR_INVALID", "The source record cursor is invalid. Refresh the evidence list.");
    }
}
export function encodeFieldVisitCursor(row: Pick<FieldVisitEvidenceRow, "scheduled_at" | "id">) {
    return Buffer.from(JSON.stringify({ scheduledAt: Number(row.scheduled_at), id: row.id })).toString("base64url");
}
export type QualityInspectionRow = {
    id: string;
    hold_id: string;
    result: "pass" | "fail";
    created_at: number | string;
    reviewed_version: number | string;
    product_name: string;
    warehouse_name: string;
};
export const qualitySearchFields = ["result", "created_at", "reviewed_version", "product_name", "warehouse_name"];
export const qualityDetailFields = [...qualitySearchFields, "evidence", "inspected_by", "hold_id"];
export function qualityCursorValue(cursor?: string) {
    if (!cursor)
        return null;
    try {
        const value = JSON.parse(Buffer.from(cursor, "base64url").toString("utf8")) as {
            createdAt?: unknown;
            id?: unknown;
        };
        if (typeof value.createdAt !== "number" || !Number.isSafeInteger(value.createdAt) || typeof value.id !== "string")
            throw new Error();
        return { createdAt: value.createdAt, id: z.string().min(1).max(200).parse(value.id) };
    }
    catch {
        throw new BusinessRuleError(422, "EVIDENCE_CURSOR_INVALID", "The source record cursor is invalid. Refresh the evidence list.");
    }
}
export function encodeQualityCursor(row: Pick<QualityInspectionRow, "created_at" | "id">) {
    return Buffer.from(JSON.stringify({ createdAt: Number(row.created_at), id: row.id })).toString("base64url");
}
export type ManufacturingCostEvidenceRow = {
    id: string;
    order_id: string;
    movement_kind: string;
    amount_minor: string;
    posted_at: number | string;
    version: number;
    journal_id: string;
    journal_status: string;
    currency: string;
    order_status: string;
    order_version: number;
    product_name: string;
    warehouse_name: string;
};
export const manufacturingCostProviderId = "manufacturing-cost-movements";
export const manufacturingCostSearchFields = ["order_id", "movement_kind", "amount_minor", "posted_at", "journal_status", "currency", "version"];
export const manufacturingCostDetailFields = [...manufacturingCostSearchFields, "order_status", "product_name", "warehouse_name", "order_version"];
export function manufacturingCostCursorValue(cursor?: string) {
    if (!cursor)
        return null;
    try {
        const value = JSON.parse(Buffer.from(cursor, "base64url").toString("utf8")) as {
            postedAt?: unknown;
            id?: unknown;
        };
        if (typeof value.postedAt !== "number" || !Number.isSafeInteger(value.postedAt) || typeof value.id !== "string")
            throw new Error();
        return { postedAt: value.postedAt, id: z.string().min(1).max(200).parse(value.id) };
    }
    catch {
        throw new BusinessRuleError(422, "EVIDENCE_CURSOR_INVALID", "The source record cursor is invalid. Refresh the evidence list.");
    }
}
export function encodeManufacturingCostCursor(row: Pick<ManufacturingCostEvidenceRow, "posted_at" | "id">) {
    return Buffer.from(JSON.stringify({ postedAt: Number(row.posted_at), id: row.id })).toString("base64url");
}
export const customerActivityProviderId = "customer-360-timeline";
export const customerActivityFields = ["source_id", "source_type", "title", "status", "version", "at", "href", "customer_id", "customer_name"];
export function customerActivityCursor(raw?: string) {
    if (!raw)
        return null;
    try {
        const value = JSON.parse(Buffer.from(raw, "base64url").toString("utf8")) as {
            at?: unknown;
            id?: unknown;
        };
        if (typeof value.at !== "number" || !Number.isSafeInteger(value.at) || typeof value.id !== "string")
            throw new Error();
        return { at: value.at, id: z.string().min(1).max(250).parse(value.id) };
    }
    catch {
        throw new BusinessRuleError(422, "EVIDENCE_CURSOR_INVALID", "The source record cursor is invalid. Refresh the evidence list.");
    }
}
export function encodeCustomerActivityCursor(row: Pick<CustomerActivityItem, "at" | "id">) {
    return Buffer.from(JSON.stringify({ at: Number(row.at), id: row.id })).toString("base64url");
}
export const socialActivityProviderId = "collaboration-social-activity";
export const socialActivityFields = ["id", "source_id", "source_type", "title", "at", "version", "activity_actor", "thread_id", "href"];
export function socialActivityCursor(raw?: string) {
    if (!raw)
        return null;
    try {
        const value = JSON.parse(Buffer.from(raw, "base64url").toString("utf8")) as {
            at?: unknown;
            id?: unknown;
        };
        if (typeof value.at !== "number" || !Number.isSafeInteger(value.at) || typeof value.id !== "string")
            throw new Error();
        return { at: value.at, id: z.string().min(1).max(260).parse(value.id) };
    }
    catch {
        throw new BusinessRuleError(422, "EVIDENCE_CURSOR_INVALID", "The source record cursor is invalid. Refresh the evidence list.");
    }
}
export function encodeSocialActivityCursor(row: Pick<SocialActivityEvent, "at" | "id">) {
    return Buffer.from(JSON.stringify({ at: Number(row.at), id: row.id })).toString("base64url");
}
export type SupportIncidentEvidenceRow = {
    id: string;
    title: string;
    severity: "critical" | "high" | "normal";
    status: string;
    due_date: string;
    created_at: number;
    updated_at: number;
    version: number;
};
export function supportIncidentCursorValue(cursor?: string) {
    if (!cursor)
        return null;
    try {
        const value = JSON.parse(Buffer.from(cursor, "base64url").toString("utf8")) as {
            updatedAt?: unknown;
            id?: unknown;
        };
        if (typeof value.updatedAt !== "number" || !Number.isSafeInteger(value.updatedAt))
            throw new Error();
        return { updatedAt: value.updatedAt, id: z.string().min(1).max(200).parse(value.id) };
    }
    catch {
        throw new BusinessRuleError(422, "EVIDENCE_CURSOR_INVALID", "The source record cursor is invalid. Refresh the evidence list.");
    }
}
export function encodeSupportIncidentCursor(row: Pick<SupportIncidentEvidenceRow, "updated_at" | "id">) {
    return Buffer.from(JSON.stringify({ updatedAt: Number(row.updated_at), id: row.id })).toString("base64url");
}
export function encodeSupportIncidentRecordId(id: string) {
    return Buffer.from(JSON.stringify({ incidentId: id })).toString("base64url");
}
export function decodeSupportIncidentRecordId(id: string) {
    try {
        const value = JSON.parse(Buffer.from(id, "base64url").toString("utf8")) as {
            incidentId?: unknown;
        };
        return z.string().min(1).max(200).parse(value.incidentId);
    }
    catch {
        throw new BusinessRuleError(404, "EVIDENCE_NOT_FOUND", "This source record is unavailable in the selected role, scope or filters.");
    }
}
export function supportIncidentNumber(value: number | string | null | undefined, field: string) {
    const parsed = Number(value);
    if (value === null || value === undefined || !Number.isSafeInteger(parsed))
        throw new BusinessRuleError(500, "EVIDENCE_VALUE_INVALID", `The source returned an invalid ${field}.`);
    return parsed;
}
export type SupportTicketEvidenceRow = {
    id: string;
    title: string;
    status: string;
    due_date: string;
    updated_at: number | string;
    version: number | string;
    sla_state?: string | null;
    sla_due_at?: string | null;
};
export function supportTicketCursorValue(cursor?: string) {
    if (!cursor)
        return null;
    try {
        const value = JSON.parse(Buffer.from(cursor, "base64url").toString("utf8")) as {
            updatedAt?: unknown;
            id?: unknown;
        };
        if (typeof value.updatedAt !== "number" || !Number.isSafeInteger(value.updatedAt))
            throw new Error();
        return { updatedAt: value.updatedAt, id: z.string().min(1).max(200).parse(value.id) };
    }
    catch {
        throw new BusinessRuleError(422, "EVIDENCE_CURSOR_INVALID", "The source record cursor is invalid. Refresh the evidence list.");
    }
}
export function encodeSupportTicketCursor(row: Pick<SupportTicketEvidenceRow, "updated_at" | "id">) {
    return Buffer.from(JSON.stringify({ updatedAt: Number(row.updated_at), id: row.id })).toString("base64url");
}
export function encodeSupportTicketRecordId(id: string) {
    return Buffer.from(JSON.stringify({ ticketId: id })).toString("base64url");
}
export function decodeSupportTicketRecordId(id: string) {
    try {
        const value = JSON.parse(Buffer.from(id, "base64url").toString("utf8")) as {
            ticketId?: unknown;
        };
        return z.string().min(1).max(200).parse(value.ticketId);
    }
    catch {
        throw new BusinessRuleError(404, "EVIDENCE_NOT_FOUND", "This source record is unavailable in the selected role, scope or filters.");
    }
}
export function supportTicketNumber(value: number | string | null | undefined, field: string) {
    const parsed = Number(value);
    if (value === null || value === undefined || !Number.isSafeInteger(parsed))
        throw new BusinessRuleError(500, "EVIDENCE_VALUE_INVALID", `The source returned an invalid ${field}.`);
    return parsed;
}
export function supportTicketFieldsFor(metric: SupportTicketMetricId) {
    return metric === "overdue-support-sla" ? supportTicketSlaFields : supportTicketFields;
}
export type DuplicateReviewEvidenceRow = {
    id: string;
    left_name: string;
    left_code: string;
    right_name: string;
    right_code: string;
    status: string;
    created_at: number | string;
    updated_at: number | string;
    version: number | string;
};
export function duplicateReviewCursorValue(cursor?: string) {
    if (!cursor)
        return null;
    try {
        const value = JSON.parse(Buffer.from(cursor, "base64url").toString("utf8")) as {
            updatedAt?: unknown;
            id?: unknown;
        };
        if (typeof value.updatedAt !== "number" || !Number.isSafeInteger(value.updatedAt))
            throw new Error();
        return { updatedAt: value.updatedAt, id: z.string().min(1).max(200).parse(value.id) };
    }
    catch {
        throw new BusinessRuleError(422, "EVIDENCE_CURSOR_INVALID", "The source record cursor is invalid. Refresh the evidence list.");
    }
}
export function encodeDuplicateReviewCursor(row: Pick<DuplicateReviewEvidenceRow, "updated_at" | "id">) {
    return Buffer.from(JSON.stringify({ updatedAt: Number(row.updated_at), id: row.id })).toString("base64url");
}
export function encodeDuplicateReviewRecordId(id: string) {
    return Buffer.from(JSON.stringify({ reviewId: id })).toString("base64url");
}
export function decodeDuplicateReviewRecordId(id: string) {
    try {
        const value = JSON.parse(Buffer.from(id, "base64url").toString("utf8")) as {
            reviewId?: unknown;
        };
        return z.string().min(1).max(200).parse(value.reviewId);
    }
    catch {
        throw new BusinessRuleError(404, "EVIDENCE_NOT_FOUND", "This source record is unavailable in the selected role, scope or filters.");
    }
}
export function duplicateReviewNumber(value: number | string, field: string) {
    const parsed = Number(value);
    if (!Number.isSafeInteger(parsed))
        throw new BusinessRuleError(500, "EVIDENCE_VALUE_INVALID", `The source returned an invalid ${field}.`);
    return parsed;
}
export type IdentityChangeEvidenceRow = {
    id: string;
    change_id: string;
    action: string;
    customer_version: number | string;
    created_at: number | string;
    version: number | string;
    change_status: string;
    change_version: number | string;
    customer_name: string;
    customer_code: string;
};
export function identityChangeCursorValue(cursor?: string) {
    if (!cursor)
        return null;
    try {
        const value = JSON.parse(Buffer.from(cursor, "base64url").toString("utf8")) as {
            at?: unknown;
            id?: unknown;
        };
        if (typeof value.at !== "number" || !Number.isSafeInteger(value.at))
            throw new Error();
        return { at: value.at, id: z.string().min(1).max(200).parse(value.id) };
    }
    catch {
        throw new BusinessRuleError(422, "EVIDENCE_CURSOR_INVALID", "The source record cursor is invalid. Refresh the evidence list.");
    }
}
export function encodeIdentityChangeCursor(row: Pick<IdentityChangeEvidenceRow, "created_at" | "id">) {
    return Buffer.from(JSON.stringify({ at: Number(row.created_at), id: row.id })).toString("base64url");
}
export function encodeIdentityChangeRecordId(id: string) {
    return Buffer.from(JSON.stringify({ eventId: id })).toString("base64url");
}
export function decodeIdentityChangeRecordId(id: string) {
    try {
        const value = JSON.parse(Buffer.from(id, "base64url").toString("utf8")) as {
            eventId?: unknown;
        };
        return z.string().min(1).max(200).parse(value.eventId);
    }
    catch {
        throw new BusinessRuleError(404, "EVIDENCE_NOT_FOUND", "This source record is unavailable in the selected role, scope or filters.");
    }
}
export function identityChangeNumber(value: number | string, field: string) {
    const parsed = Number(value);
    if (!Number.isSafeInteger(parsed))
        throw new BusinessRuleError(500, "EVIDENCE_VALUE_INVALID", `The source returned an invalid ${field}.`);
    return parsed;
}
export type OrganizationNodeEvidenceRow = {
    id: string;
    kind: string;
    slug: string;
    display_name: string;
    parent_name: string | null;
    branch_name: string | null;
    status: string;
    hierarchy_revision: number | string;
};
export type PendingStructureEvidenceRow = {
    id: string;
    action: string;
    node_kind: string;
    display_name: string;
    state: string;
    version: number | string;
    created_at: number | string;
    updated_at: number | string;
    branch_name: string;
};
export function organizationEvidenceCursor(cursor?: string) {
    if (!cursor)
        return null;
    try {
        const value = JSON.parse(Buffer.from(cursor, "base64url").toString("utf8")) as {
            name?: unknown;
            at?: unknown;
            id?: unknown;
        };
        return {
            name: value.name === undefined ? undefined : z.string().max(200).parse(value.name),
            at: value.at === undefined ? undefined : z.number().int().safe().parse(value.at),
            id: z.string().min(1).max(200).parse(value.id),
        };
    }
    catch {
        throw new BusinessRuleError(422, "EVIDENCE_CURSOR_INVALID", "The source record cursor is invalid. Refresh the evidence list.");
    }
}
export function encodeOrganizationRecordId(metric: typeof organizationNodeMetric | typeof pendingStructureMetric, id: string) {
    return Buffer.from(JSON.stringify({ metric, id })).toString("base64url");
}
export function decodeOrganizationRecordId(recordId: string, metric: typeof organizationNodeMetric | typeof pendingStructureMetric) {
    try {
        const value = JSON.parse(Buffer.from(recordId, "base64url").toString("utf8")) as {
            metric?: unknown;
            id?: unknown;
        };
        if (value.metric !== metric)
            throw new Error();
        return z.string().min(1).max(200).parse(value.id);
    }
    catch {
        throw new BusinessRuleError(404, "EVIDENCE_NOT_FOUND", "This source record is unavailable in the selected role, scope or filters.");
    }
}
export function organizationEvidenceNumber(value: number | string, field: string) {
    const parsed = Number(value);
    if (!Number.isSafeInteger(parsed))
        throw new BusinessRuleError(500, "EVIDENCE_VALUE_INVALID", `The source returned an invalid ${field}.`);
    return parsed;
}
export type ActiveAssignmentEvidenceRow = {
    id: string;
    user_id: string;
    role: string;
    role_name: string;
    scope_kind: string;
    scope_node_id: string | null;
    scope_name: string;
    branch_name: string;
    source: string;
    valid_from: number | string;
    valid_until: number | string;
    version: number | string;
};
export type PendingAccessReviewEvidenceRow = {
    id: string;
    campaign_id: string;
    assignment_id: string;
    assignment_version: number | string;
    user_id: string;
    role: string;
    scope_kind: string;
    scope_node_id: string | null;
    scope_name: string;
    branch_name: string;
    source: string;
    valid_until: number | string;
    decision: string;
    campaign_name: string;
    campaign_status: string;
    campaign_due_at: number | string;
    campaign_version: number | string;
    campaign_created_at: number | string;
};
export function identityAccessCursor(cursor?: string) {
    if (!cursor)
        return null;
    try {
        const value = JSON.parse(Buffer.from(cursor, "base64url").toString("utf8")) as {
            userId?: unknown;
            role?: unknown;
            createdAt?: unknown;
            recordId?: unknown;
        };
        if (value.userId !== undefined || value.role !== undefined) {
            return { userId: z.string().min(1).max(200).parse(value.userId), role: z.string().min(1).max(200).parse(value.role), createdAt: undefined, recordId: z.string().min(1).max(200).parse(value.recordId) };
        }
        return { userId: undefined, role: undefined, createdAt: z.number().int().safe().parse(value.createdAt), recordId: z.string().min(1).max(200).parse(value.recordId) };
    }
    catch {
        throw new BusinessRuleError(422, "EVIDENCE_CURSOR_INVALID", "The source record cursor is invalid. Refresh the evidence list.");
    }
}
export function encodeIdentityAccessRecordId(metric: typeof activeAssignmentsMetric | typeof pendingAccessReviewsMetric, id: string) {
    return Buffer.from(JSON.stringify({ metric, id })).toString("base64url");
}
export function decodeIdentityAccessRecordId(recordId: string, metric: typeof activeAssignmentsMetric | typeof pendingAccessReviewsMetric) {
    try {
        const value = JSON.parse(Buffer.from(recordId, "base64url").toString("utf8")) as {
            metric?: unknown;
            id?: unknown;
        };
        if (value.metric !== metric)
            throw new Error();
        return z.string().min(1).max(200).parse(value.id);
    }
    catch {
        throw new BusinessRuleError(404, "EVIDENCE_NOT_FOUND", "This source record is unavailable in the selected role, scope or filters.");
    }
}
export type CommercialSubscriptionRow = {
    source_kind: "enrollment" | "legacy";
    record_id: string;
    plan_name: string;
    status: string;
    started_at: number | string | null;
    effective_at: number | string | null;
    record_version: number | string;
};
export type OverdueInvoiceEvidenceRow = {
    id: string;
    kind: "main" | "adjustment";
    status: "open" | "uncollectible";
    total_satang: string | number;
    due_at: string | number;
    updated_at: string | number;
    month: string;
};
export function commercialCursor(cursor?: string) {
    if (!cursor)
        return null;
    try {
        const value = JSON.parse(Buffer.from(cursor, "base64url").toString("utf8")) as {
            sourceKind?: unknown;
            recordId?: unknown;
        };
        const sourceKind = z.enum(["enrollment", "legacy"]).parse(value.sourceKind);
        const recordId = z.string().min(1).max(200).parse(value.recordId);
        return { sourceKind, recordId };
    }
    catch {
        throw new BusinessRuleError(422, "EVIDENCE_CURSOR_INVALID", "The source record cursor is invalid. Refresh the evidence list.");
    }
}
export function encodeCommercialSubscriptionId(sourceKind: CommercialSubscriptionRow["source_kind"], recordId: string) {
    return Buffer.from(JSON.stringify({ sourceKind, recordId })).toString("base64url");
}
export function decodeCommercialSubscriptionId(recordId: string) {
    try {
        const value = JSON.parse(Buffer.from(recordId, "base64url").toString("utf8")) as {
            sourceKind?: unknown;
            recordId?: unknown;
        };
        return { sourceKind: z.enum(["enrollment", "legacy"]).parse(value.sourceKind), recordId: z.string().min(1).max(200).parse(value.recordId) };
    }
    catch {
        throw new BusinessRuleError(404, "EVIDENCE_NOT_FOUND", "This source record is unavailable in the selected role, scope or filters.");
    }
}
export function encodeOverdueInvoiceId(id: string) {
    return Buffer.from(JSON.stringify({ metric: overdueInvoicesMetric, id })).toString("base64url");
}
export function decodeOverdueInvoiceId(recordId: string) {
    try {
        const value = JSON.parse(Buffer.from(recordId, "base64url").toString("utf8")) as {
            metric?: unknown;
            id?: unknown;
        };
        if (value.metric !== overdueInvoicesMetric)
            throw new Error();
        return z.string().min(1).max(200).parse(value.id);
    }
    catch {
        throw new BusinessRuleError(404, "EVIDENCE_NOT_FOUND", "This source record is unavailable in the selected role, scope or filters.");
    }
}
export type CloseBlockerEvidenceRow = {
    id: string;
    period_id: string;
    period_name: string;
    end_date: string;
    period_state: string;
    check_key: string;
    source_version: string;
    status: string;
    count_value: string | number;
    checked_at: number | string;
    resolved_at: number | string | null;
};
export function closeBlockerCursorValue(cursor?: string) {
    if (!cursor)
        return null;
    try {
        const value = JSON.parse(Buffer.from(cursor, "base64url").toString("utf8")) as {
            endDate?: unknown;
            checkKey?: unknown;
            id?: unknown;
        };
        return {
            endDate: z.string().date().parse(value.endDate),
            checkKey: z.string().min(1).max(200).parse(value.checkKey),
            id: z.string().min(1).max(200).parse(value.id),
        };
    }
    catch {
        throw new BusinessRuleError(422, "EVIDENCE_CURSOR_INVALID", "The source record cursor is invalid. Refresh the evidence list.");
    }
}
export function encodeCloseBlockerCursor(row: Pick<CloseBlockerEvidenceRow, "end_date" | "check_key" | "id">) {
    return Buffer.from(JSON.stringify({ endDate: row.end_date, checkKey: row.check_key, id: row.id })).toString("base64url");
}
export function encodeCloseBlockerRecordId(id: string) {
    return Buffer.from(JSON.stringify({ checkId: id })).toString("base64url");
}
export function decodeCloseBlockerRecordId(id: string) {
    try {
        const value = JSON.parse(Buffer.from(id, "base64url").toString("utf8")) as {
            checkId?: unknown;
        };
        return z.string().min(1).max(200).parse(value.checkId);
    }
    catch {
        throw new BusinessRuleError(404, "EVIDENCE_NOT_FOUND", "This source record is unavailable in the selected role, scope or filters.");
    }
}
export function closeBlockerNumber(value: number | string, field: string) {
    const parsed = Number(value);
    if (!Number.isSafeInteger(parsed))
        throw new BusinessRuleError(500, "EVIDENCE_VALUE_INVALID", `The source returned an invalid ${field}.`);
    return parsed;
}
export function closeBlockerCount(value: string | number) {
    const parsed = String(value);
    if (!/^\d+$/.test(parsed))
        throw new BusinessRuleError(500, "EVIDENCE_VALUE_INVALID", "The source returned an invalid blocker count.");
    return parsed;
}
export type MaintenanceEvidenceRow = Awaited<ReturnType<typeof maintenanceDowntimeEvidenceRows>>[number];
export function maintenanceCursorValue(cursor?: string) {
    if (!cursor)
        return null;
    try {
        const value = JSON.parse(Buffer.from(cursor, "base64url").toString("utf8")) as {
            endedAt?: unknown;
            id?: unknown;
        };
        if (typeof value.endedAt !== "number" || !Number.isSafeInteger(value.endedAt) || typeof value.id !== "string")
            throw new Error();
        return { endedAt: value.endedAt, id: z.string().min(1).max(200).parse(value.id) };
    }
    catch {
        throw new BusinessRuleError(422, "EVIDENCE_CURSOR_INVALID", "The source record cursor is invalid. Refresh the evidence list.");
    }
}
export function encodeMaintenanceCursor(row: MaintenanceEvidenceRow) {
    return Buffer.from(JSON.stringify({ endedAt: Number(row.downtime_ended_at), id: row.id })).toString("base64url");
}
export function maintenanceEvidenceNumber(value: string, field: string) {
    const parsed = Number(value);
    if (!Number.isSafeInteger(parsed))
        throw new BusinessRuleError(500, "EVIDENCE_VALUE_INVALID", `The source returned an invalid ${field}.`);
    return parsed;
}
export type AvailableStockEvidenceRow = {
    warehouse_id: string;
    product_id: string;
    sku: string;
    product_name: string;
    unit: string;
    warehouse_name: string;
    on_hand_milli: string;
    reserved_milli: string;
    quarantined_milli: string;
    available_milli: string;
    stock_version: number;
};
export type InventoryShortageEvidenceRow = {
    warehouse_id: string;
    product_id: string;
    sku: string;
    product_name: string;
    unit: string;
    warehouse_name: string;
    on_hand_milli: string;
    reserved_milli: string;
    quarantined_milli: string;
    available_milli: string;
    approved_reorder_point_milli: string;
    policy_version: number;
    stock_version: number;
};
export type InventoryMovementEvidenceRow = {
    id: string;
    source_type: string;
    source_id: string;
    source_number: string;
    movement_date: string;
    quantity_milli: string;
    unit: string;
    sku: string;
    product_name: string;
    warehouse_id: string;
    warehouse_name: string;
};
export function inventoryMovementCursorValue(cursor?: string) {
    if (!cursor)
        return null;
    try {
        const value = JSON.parse(Buffer.from(cursor, "base64url").toString("utf8")) as {
            movementDate?: unknown;
            id?: unknown;
        };
        return { movementDate: z.string().date().parse(value.movementDate), id: z.string().min(1).max(200).parse(value.id) };
    }
    catch {
        throw new BusinessRuleError(422, "EVIDENCE_CURSOR_INVALID", "The source record cursor is invalid. Refresh the evidence list.");
    }
}
export function encodeInventoryMovementCursor(row: Pick<InventoryMovementEvidenceRow, "movement_date" | "id">) {
    return Buffer.from(JSON.stringify({ movementDate: row.movement_date, id: row.id })).toString("base64url");
}
export function encodeInventoryMovementRecordId(row: Pick<InventoryMovementEvidenceRow, "id">) {
    return Buffer.from(JSON.stringify({ movementId: row.id })).toString("base64url");
}
export function decodeInventoryMovementRecordId(id: string) {
    try {
        const value = JSON.parse(Buffer.from(id, "base64url").toString("utf8")) as {
            movementId?: unknown;
        };
        return { movementId: z.string().min(1).max(200).parse(value.movementId) };
    }
    catch {
        throw new BusinessRuleError(404, "EVIDENCE_NOT_FOUND", "This source record is unavailable in the selected role, scope or filters.");
    }
}
export function inventoryMovementHref(row: Pick<InventoryMovementEvidenceRow, "id" | "source_type" | "source_id">) {
    const sourceId = encodeURIComponent(row.source_id);
    if (row.source_type === "stock_count")
        return `/stock-counts/${sourceId}`;
    if (row.source_type === "transfer")
        return `/stock-transfers/${sourceId}`;
    if (row.source_type === "transit")
        return `/stock-transit/${sourceId}`;
    if (row.source_type.startsWith("pos_"))
        return `/pos?movement=${encodeURIComponent(row.id)}`;
    if (row.source_type === "pms")
        return `/pms/inventory?property=${sourceId}`;
    return `/commerce/orders/${sourceId}`;
}
export function availableStockCursorValue(cursor?: string) {
    if (!cursor)
        return null;
    try {
        const value = JSON.parse(Buffer.from(cursor, "base64url").toString("utf8")) as {
            warehouseId?: unknown;
            productId?: unknown;
        };
        return {
            warehouseId: z.string().min(1).max(200).parse(value.warehouseId),
            productId: z.string().min(1).max(200).parse(value.productId),
        };
    }
    catch {
        throw new BusinessRuleError(422, "EVIDENCE_CURSOR_INVALID", "The source record cursor is invalid. Refresh the evidence list.");
    }
}
export function encodeAvailableStockCursor(row: AvailableStockEvidenceRow) {
    return Buffer.from(JSON.stringify({ warehouseId: row.warehouse_id, productId: row.product_id })).toString("base64url");
}
export function encodeAvailableStockRecordId(row: Pick<AvailableStockEvidenceRow, "warehouse_id" | "product_id">) {
    return Buffer.from(JSON.stringify({ warehouseId: row.warehouse_id, productId: row.product_id })).toString("base64url");
}
export function decodeAvailableStockRecordId(id: string) {
    try {
        const value = JSON.parse(Buffer.from(id, "base64url").toString("utf8")) as {
            warehouseId?: unknown;
            productId?: unknown;
        };
        return {
            warehouseId: z.string().min(1).max(200).parse(value.warehouseId),
            productId: z.string().min(1).max(200).parse(value.productId),
        };
    }
    catch {
        throw new BusinessRuleError(404, "EVIDENCE_NOT_FOUND", "This source record is unavailable in the selected role, scope or filters.");
    }
}
export function milliQuantity(value: string) {
    if (!/^-?\d+$/.test(value))
        throw new BusinessRuleError(500, "EVIDENCE_VALUE_INVALID", "The source returned an invalid stock quantity.");
    const number = BigInt(value), negative = number < BigInt(0), absolute = negative ? -number : number;
    return `${negative ? "-" : ""}${absolute / BigInt(1000)}.${String(absolute % BigInt(1000)).padStart(3, "0")}`;
}
export type ProjectMilestoneEvidenceRow = {
    id: string;
    project_id: string;
    project_title: string;
    title: string;
    due_at: number;
    owner_user_id: string;
    owner_name: string;
    state: "planned" | "in_progress";
    version: number;
};
export function projectMilestoneCursor(value?: string) {
    if (!value)
        return null;
    try {
        const decoded = JSON.parse(Buffer.from(value, "base64url").toString("utf8")) as {
            dueAt?: unknown;
            id?: unknown;
        };
        if (typeof decoded.dueAt !== "number" || !Number.isSafeInteger(decoded.dueAt))
            throw new Error();
        return { dueAt: decoded.dueAt, id: z.string().min(1).max(200).parse(decoded.id) };
    }
    catch {
        throw new BusinessRuleError(422, "EVIDENCE_CURSOR_INVALID", "The source record cursor is invalid. Refresh the evidence list.");
    }
}
export type ProjectDependencyEvidenceRow = {
    id: string;
    project_id: string;
    project_title: string;
    title: string;
    due_at: number;
    state: "planned" | "in_progress";
    version: number;
    blockers_json: string;
};
export type ProjectDependencyEvidenceBlocker = {
    id: string;
    title: string;
    state: string;
    version: number;
};
export function projectDependencyBlockers(value: string): ProjectDependencyEvidenceBlocker[] {
    try {
        const parsed = JSON.parse(value) as unknown;
        if (!Array.isArray(parsed))
            return [];
        return parsed.filter((item): item is ProjectDependencyEvidenceBlocker => !!item && typeof item === "object"
            && typeof (item as ProjectDependencyEvidenceBlocker).id === "string"
            && typeof (item as ProjectDependencyEvidenceBlocker).title === "string"
            && typeof (item as ProjectDependencyEvidenceBlocker).state === "string"
            && Number.isSafeInteger((item as ProjectDependencyEvidenceBlocker).version));
    }
    catch {
        return [];
    }
}
export type ProjectActualEvidenceRow = {
    id: string;
    project_id: string;
    project_title: string;
    sort_date: string;
    state: string;
    version: number;
    work_date?: string;
    minutes?: number;
    user_id?: string;
    journal_id?: string;
    journal_number?: string;
    entry_date?: string;
    currency?: string;
    amount_minor?: string;
    account_code?: string;
    account_name?: string;
};
export function projectActualCursor(value?: string) {
    if (!value)
        return null;
    try {
        const decoded = JSON.parse(Buffer.from(value, "base64url").toString("utf8")) as {
            date?: unknown;
            id?: unknown;
        };
        return { date: z.string().date().parse(decoded.date), id: z.string().min(1).max(200).parse(decoded.id) };
    }
    catch {
        throw new BusinessRuleError(422, "EVIDENCE_CURSOR_INVALID", "The source record cursor is invalid. Refresh the evidence list.");
    }
}
export type OperationsEvidenceRow = {
    id: string;
    title: string;
    state: string;
    due_at: number | null;
    created_at: number;
    updated_at: number;
    version: number;
    branch_id: string | null;
    department_id: string | null;
    exception_duration_ms: string | null;
    alert_created_at: number | null;
    sort_at: number;
};
export function operationsEvidenceFields(metric: OperationsMetricId) {
    return metric === "operations-escalations" ? operationsEscalationFields
        : metric === "exception-resolution-time" ? operationsResolutionFields : operationsFields;
}
export function operationsRecordId(metric: OperationsMetricId, id: string) {
    return Buffer.from(JSON.stringify({ metric, id })).toString("base64url");
}
export function decodeOperationsRecordId(value: string, metric: OperationsMetricId) {
    try {
        const decoded = JSON.parse(Buffer.from(value, "base64url").toString("utf8")) as {
            metric?: unknown;
            id?: unknown;
        };
        if (decoded.metric !== metric || typeof decoded.id !== "string")
            throw new Error();
        return z.string().min(1).max(200).parse(decoded.id);
    }
    catch {
        throw new BusinessRuleError(422, "EVIDENCE_RECORD_INVALID", "The source record reference is invalid. Refresh the evidence list.");
    }
}
export function operationsEvidenceCursor(value?: string, metric?: OperationsMetricId) {
    if (!value)
        return null;
    try {
        const decoded = JSON.parse(Buffer.from(value, "base64url").toString("utf8")) as {
            metric?: unknown;
            sortAt?: unknown;
            id?: unknown;
        };
        if (decoded.metric !== metric || typeof decoded.sortAt !== "number" || !Number.isSafeInteger(decoded.sortAt))
            throw new Error();
        return { sortAt: decoded.sortAt, id: z.string().min(1).max(200).parse(decoded.id) };
    }
    catch {
        throw new BusinessRuleError(422, "EVIDENCE_CURSOR_INVALID", "The source record cursor is invalid. Refresh the evidence list.");
    }
}
