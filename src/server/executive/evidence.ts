import "server-only";
import { z } from "zod";
import { hasPermission } from "@/lib/permissions";
import { hasModuleAccess } from "@/data/module-access";
import { filtersSchema, type Filters, type MetricId } from "@/lib/executive/canvas";
import type { WorkContext } from "../authorization";
import type { SqlDatabase } from "../runtime/types";
import { sha256 } from "../crypto";
import { ApiError } from "../http";
import { currentAuthority } from "./access";
import { metricCatalogFor } from "./metrics";
import { assertProductionModuleEnabled } from "../hospitality-rollout";
import { getSupplierBill } from "../supplier-bill-matching";
import { getQualityHold } from "../quality";
import { fieldServiceDetail } from "../field-service";
import { productionDetail } from "../production";
import { customerTimelineEvents, type CustomerActivityItem } from "../customer-timeline";
import { socialActivityEvents, type SocialActivityEvent } from "../social-activity";
import { visibility as legalVisibility } from "../legal/access";
import { maintenanceDowntimeEvidenceRows } from "../asset-maintenance";
import { getSupportIncident } from "../support-incidents";
import { canReadOrganizationChanges } from "../organization-work";
import { organizationChangeHref } from "@/lib/organization";
import { runtimeEnv } from "../runtime";
import { operationsMetrics, operationsTaskCohort, operationsTaskHref, operationsTaskVisibility, type OperationsMetricId } from "./operations-access";
import { loadPms } from "../pms";
import type { PmsRecord } from "@/lib/pms-model";
import { getOvertime } from "../overtime";
import { participantSql } from "../approvals";
import { projectDependencyBlockerCohort, projectMilestoneHref, projectMilestoneOverdueCohort } from "./project-milestone-access";
import { agentEvidenceMetrics, readAgentEvidence, searchAgentEvidence } from "./agent-evidence";
import { crmEvidenceMetrics, readCrmEvidence, searchCrmEvidence } from "./crm-evidence";
import { brandEvidenceMetrics, readBrandEvidence, searchBrandEvidence } from "./brand-evidence";
import { learningEvidenceMetrics, readLearningEvidence, searchLearningEvidence } from "./learning-evidence";
import { readTalentEvidence, searchTalentEvidence, talentEvidenceMetrics } from "./talent-evidence";
import { posEvidenceMetrics, readPosEvidence, searchPosEvidence } from "./pos-evidence";
import { marketingEvidenceMetrics, readMarketingEvidence, searchMarketingEvidence } from "./marketing-evidence";
import { planningEvidenceMetrics, readPlanningEvidence, searchPlanningEvidence } from "./planning-evidence";

const providerId = "finance-invoices";
const payrollSummaryMetric = "payroll-summary" as const;
const payrollSummaryProviderId = "people-payroll-posted-runs";
const payrollSummaryFields = ["period_start", "period_end", "pay_date", "currency", "status", "version", "net_minor"];
const approvedLeaveMetric = "approved-leave-duration" as const;
const authorizedOvertimeMetric = "authorized-overtime-duration" as const;
const peopleLeaveOvertimeProviderId = "people-leave-overtime-requests";
const approvedLeaveFields = ["number", "start_date", "end_date", "calculated_units", "days", "state", "version"];
const authorizedOvertimeFields = ["handoff.work_date", "handoff.minutes", "handoff.request_version", "state", "version"];
const hotelOccupancyMetric = "hotel-occupancy" as const;
const hotelOccupancyProviderId = "pms-night-audit";
const hotelOccupancyFields = ["property.id", "property.name", "property.currency", "night.date", "night.state", "night.roomMetrics.sold", "night.roomMetrics.capacity", "night.version"];
const hotelRoomRevenueMetrics = ["hotel-adr", "hotel-revpar"] as const;
type HotelRoomRevenueMetric = typeof hotelRoomRevenueMetrics[number];
const hotelRoomRevenueFields = [...hotelOccupancyFields, "night.roomRevenueMinor", "entry.kind", "entry.date", "entry.net", "entry.sourceKey", "entry.originalId"];
const hotelFolioBalanceMetric = "hotel-folio-balance" as const;
const hotelFolioBalanceProviderId = "pms-open-folios";
const hotelFolioBalanceFields = ["folio.id", "folio.state", "folio.version", "property.name", "property.currency", "posted_receivable_balance_minor"];
const hotelNightAuditBlockersMetric = "hotel-night-audit-blockers" as const;
const hotelNightAuditBlockersProviderId = "pms-night-audit-blockers";
const hotelNightAuditBlockersFields = ["property.id", "property.name", "property.branch_id", "job.business_date", "job.state", "job.error_code", "night.version"];
const procurementProviderId = "procurement-supplier-bills";
const salesProviderId = "commerce-sales-orders";
const qualityProviderId = "quality-records";
const financeMetrics = ["receivables", "payables", "invoiced-sales"] as const;
type FinanceMetricId = typeof financeMetrics[number];
const salesMetrics = ["sales-orders", "sales-cancellations", "sales-cancellation-value"] as const;
type SalesMetricId = typeof salesMetrics[number];
const salesFulfillmentMetric = "sales-fulfillment-rate" as const;
const fieldVisitMetric = "field-visit-completion" as const;
const manufacturingCostMetric = "production-cost" as const;
const customerActivityMetric = "recent-customer-activity" as const;
const socialActivityMetric = "collaboration-thread-activity" as const;
const legalMetrics = ["active-legal-obligations", "active-legal-holds"] as const;
type LegalMetricId = typeof legalMetrics[number];
const legalProviderId = "legal-obligations-holds";
const maintenanceDowntimeMetric = "asset-downtime" as const;
const maintenanceProviderId = "maintenance-downtime-work-orders";
const supportIncidentMetric = "active-support-incidents" as const;
const supportIncidentProviderId = "support-incidents";
const supportIncidentFields = ["title", "severity", "status", "due_date", "created_at", "updated_at", "version"];
const supportTicketMetrics = ["open-support-tickets", "overdue-support-sla"] as const;
type SupportTicketMetricId = typeof supportTicketMetrics[number];
const supportTicketProviderId = "support-tickets";
const supportTicketFields = ["title", "status", "due_date", "updated_at", "version"];
const supportTicketSlaFields = [...supportTicketFields, "sla_clock_json.state", "sla_clock_json.dueAt"];
const duplicateReviewMetric = "duplicate-reviews-pending" as const;
const duplicateReviewProviderId = "customer-duplicate-reviews";
const duplicateReviewFields = ["left_customer.name", "left_customer.code", "right_customer.name", "right_customer.code", "status", "created_at", "updated_at", "version"];
const closeBlockersMetric = "close-blockers" as const;
const closeBlockersProviderId = "finance-close-checks";
const closeBlockersFields = ["check_key", "count_value", "status", "checked_at", "source_version", "period.name", "period.end_date", "period.state"];
const identityChangeMetric = "identity-change-events" as const;
const identityChangeProviderId = "customer-identity-events";
const identityChangeFields = ["action", "created_at", "version", "customer_version", "customer.name", "customer.code", "change.status", "change.version"];
const organizationNodeMetric = "active-organization-nodes" as const;
const pendingStructureMetric = "pending-structure-changes" as const;
const organizationProviderId = "organization-records";
const organizationNodeFields = ["kind", "slug", "display_name", "status", "branch.name", "parent.name", "hierarchy_revision"];
const pendingStructureFields = ["action", "node_kind", "display_name", "state", "version", "created_at", "updated_at", "branch.name"];
const activeAssignmentsMetric = "active-assignments" as const;
const pendingAccessReviewsMetric = "pending-access-reviews" as const;
const activeAssignmentsProviderId = "identity-access-assignments";
const pendingAccessReviewsProviderId = "identity-access-reviews";
const activeSubscriptionsMetric = "active-subscriptions" as const;
const overdueInvoicesMetric = "overdue-invoices" as const;
const activeSubscriptionsProviderId = "commercial-subscriptions";
const overdueInvoicesProviderId = "commercial-overdue-invoices";
const activeSubscriptionFields = ["plan.name", "status", "started_at", "effective_at", "billing_accounts.version"];
const legacySubscriptionFields = ["status", "current_period_end", "updated_at"];
const overdueInvoiceFields = ["kind", "status", "due_at", "total_satang", "updated_at", "billing_periods.month"];
const activeAssignmentFields = ["user.id", "user.name", "role", "role.name", "scope_kind", "scope_node_id", "scope.name", "branch.name", "source", "valid_from", "valid_until", "version"];
const pendingAccessReviewFields = ["campaign.name", "campaign.status", "campaign.due_at", "campaign.version", "user.id", "user.name", "role", "scope_kind", "scope_node_id", "scope.name", "branch.name", "source", "valid_until", "assignment_version", "decision"];
const maintenanceSearchFields = ["title", "status", "asset_code", "asset_name", "downtime_started_at", "downtime_ended_at", "version"];
const maintenanceDetailFields = maintenanceSearchFields;
const qualityMetrics = ["quality-holds", "quality-releases", "inspection-pass-rate"] as const;
const searchableMetrics = new Set<MetricId>(["receivables", "payables", "invoiced-sales"]);
const searchFields = ["invoice_number", "kind", "invoice_date", "due_date", "currency", "total_minor", "outstanding_minor", "status", "version"];
const detailFields = [...searchFields, "subtotal_minor", "tax_minor", "posted_at", "party_name"];
const procurementSearchFields = ["reference", "supplier_name", "order_number", "invoice_date", "due_date", "currency", "subtotal_minor", "tax_minor", "status", "version"];
const procurementDetailFields = [...procurementSearchFields, "expected_subtotal_minor", "quantity_matches", "fulfillment_version", "receipt_count"];
const salesSearchFields = ["number", "order_date", "cancelled_at", "currency", "total_minor", "status", "version", "prior_approval"];
const salesDetailFields = [...salesSearchFields, "subtotal_minor", "tax_minor"];
const salesOrderSearchFields = ["number", "order_date", "currency", "subtotal_minor", "tax_minor", "total_minor", "status", "version"];
const salesOrderDetailFields = salesOrderSearchFields;
const commonFilters = { filters: filtersSchema, query: z.string().trim().max(100).default(""), cursor: z.string().max(2000).optional(), limit: z.coerce.number().int().min(1).max(50).default(25) };
const querySchema = z.object({
  metric: z.enum(financeMetrics),
  ...commonFilters,
}).strict();
const procurementQuerySchema = z.object({ metric: z.literal("invoice-match-exceptions"), ...commonFilters }).strict();
const salesQuerySchema = z.object({ metric: z.enum(salesMetrics), ...commonFilters }).strict();
const salesFulfillmentQuerySchema = z.object({ metric: z.literal(salesFulfillmentMetric), ...commonFilters }).strict();
const fieldVisitQuerySchema = z.object({ metric: z.literal(fieldVisitMetric), ...commonFilters }).strict();
const manufacturingCostQuerySchema = z.object({ metric: z.literal(manufacturingCostMetric), ...commonFilters }).strict();
const qualityQuerySchema = z.object({ metric: z.enum(qualityMetrics), ...commonFilters }).strict();
const customerActivityQuerySchema = z.object({ metric: z.literal(customerActivityMetric), ...commonFilters }).strict();
const socialActivityQuerySchema = z.object({ metric: z.literal(socialActivityMetric), ...commonFilters }).strict();
const legalQuerySchema = z.object({ metric: z.enum(legalMetrics), ...commonFilters }).strict();
const maintenanceQuerySchema = z.object({ metric: z.literal(maintenanceDowntimeMetric), ...commonFilters }).strict();
const supportIncidentQuerySchema = z.object({ metric: z.literal(supportIncidentMetric), ...commonFilters }).strict();
const supportTicketQuerySchema = z.object({ metric: z.enum(supportTicketMetrics), ...commonFilters }).strict();
const duplicateReviewQuerySchema = z.object({ metric: z.literal(duplicateReviewMetric), ...commonFilters }).strict();
const closeBlockersQuerySchema = z.object({ metric: z.literal(closeBlockersMetric), ...commonFilters }).strict();
const identityChangeQuerySchema = z.object({ metric: z.literal(identityChangeMetric), ...commonFilters }).strict();
const organizationNodeQuerySchema = z.object({ metric: z.literal(organizationNodeMetric), ...commonFilters }).strict();
const pendingStructureQuerySchema = z.object({ metric: z.literal(pendingStructureMetric), ...commonFilters }).strict();
const activeAssignmentsQuerySchema = z.object({ metric: z.literal(activeAssignmentsMetric), ...commonFilters }).strict();
const pendingAccessReviewsQuerySchema = z.object({ metric: z.literal(pendingAccessReviewsMetric), ...commonFilters }).strict();
const activeSubscriptionsQuerySchema = z.object({ metric: z.literal(activeSubscriptionsMetric), ...commonFilters }).strict();
const overdueInvoicesQuerySchema = z.object({ metric: z.literal(overdueInvoicesMetric), ...commonFilters }).strict();
const payrollSummaryQuerySchema = z.object({ metric: z.literal(payrollSummaryMetric), ...commonFilters }).strict();
const peopleLeaveOvertimeQuerySchema = z.object({ metric: z.enum([approvedLeaveMetric, authorizedOvertimeMetric]), ...commonFilters }).strict();
const hotelOccupancyQuerySchema = z.object({ metric: z.enum([hotelOccupancyMetric, ...hotelRoomRevenueMetrics]), ...commonFilters }).strict();
const hotelFolioBalanceQuerySchema = z.object({ metric: z.literal(hotelFolioBalanceMetric), ...commonFilters }).strict();
const hotelNightAuditBlockersQuerySchema = z.object({ metric: z.literal(hotelNightAuditBlockersMetric), ...commonFilters }).strict();
const availableStockMetric = "available-stock" as const;
const availableStockProviderId = "inventory-available-stock";
const availableStockFields = ["sku", "product_name", "unit", "warehouse_name", "on_hand_milli", "reserved_milli", "quarantined_milli", "available_milli", "stock_version"];
const availableStockQuerySchema = z.object({ metric: z.literal(availableStockMetric), ...commonFilters }).strict();
const inventoryShortagesMetric = "inventory-shortages" as const;
const inventoryShortagesProviderId = "inventory-reorder-shortages";
const inventoryShortagesFields = ["sku", "product_name", "unit", "warehouse_name", "on_hand_milli", "reserved_milli", "quarantined_milli", "available_milli", "approved_reorder_point_milli", "policy_version", "stock_version"];
const inventoryShortagesQuerySchema = z.object({ metric: z.literal(inventoryShortagesMetric), ...commonFilters }).strict();
const stockedLocationsMetric = "inventory" as const;
const stockedLocationsQuerySchema = z.object({ metric: z.literal(stockedLocationsMetric), ...commonFilters }).strict();
const inventoryMovementsMetric = "inventory-movements" as const;
const inventoryMovementsProviderId = "inventory-movements";
const inventoryMovementFields = ["movement_date", "source_type", "source_number", "sku", "product_name", "unit", "warehouse_name", "quantity_milli"];
const inventoryMovementsQuerySchema = z.object({ metric: z.literal(inventoryMovementsMetric), ...commonFilters }).strict();
const operationsProviderId = "operations-staff-tasks";
const operationsFields = ["title", "state", "due_at", "created_at", "updated_at", "version", "branch_id", "department_id"];
const operationsEscalationFields = [...operationsFields, "staff_task_alert.created_at"];
const operationsResolutionFields = [...operationsFields, "overdue_duration_ms"];
const operationsQuerySchema = z.object({ metric: z.enum(operationsMetrics), ...commonFilters }).strict();
const projectMilestoneMetric = "overdue-project-milestones" as const;
const projectMilestoneProviderId = "projects-overdue-milestones";
const projectMilestoneFields = ["title", "project.title", "due_at", "owner_user_id", "owner_name", "state", "version"];
const projectMilestoneQuerySchema = z.object({ metric: z.literal(projectMilestoneMetric), ...commonFilters }).strict();
const projectDependencyMetric = "project-dependency-blockers" as const;
const projectDependencyProviderId = "projects-milestone-dependencies";
const projectDependencyFields = ["project.title", "milestone.title", "milestone.due_at", "milestone.state", "milestone.version", "prerequisite.id", "prerequisite.title", "prerequisite.state", "prerequisite.version"];
const projectDependencyQuerySchema = z.object({ metric: z.literal(projectDependencyMetric), ...commonFilters }).strict();
const projectActualMetrics = ["project-actual-hours", "project-posted-cost"] as const;
type ProjectActualMetric = typeof projectActualMetrics[number];
const projectActualProviderId = "projects-approved-actuals";
const projectActualHoursFields = ["project.title", "work_date", "minutes", "state", "version", "user_id"];
const projectPostedCostFields = ["project.title", "journal.number", "journal.entry_date", "journal.currency", "journal.status", "account.code", "account.name", "amount_minor", "allocation.state", "allocation.version"];
const projectActualQuerySchema = z.object({ metric: z.enum(projectActualMetrics), ...commonFilters }).strict();
export type EvidenceSearchInput = z.input<typeof querySchema>;

type InvoiceEvidenceRow = {
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

function predicate(metric: FinanceMetricId) {
  if (metric === "payables") return { kind: "payable", statuses: ["posted", "partially_paid"] };
  if (metric === "receivables") return { kind: "receivable", statuses: ["posted", "partially_paid"] };
  return { kind: "receivable", statuses: ["posted", "partially_paid", "paid"] };
}

async function requireFinanceEvidence(db: SqlDatabase, actor: WorkContext, metric: FinanceMetricId) {
  await currentAuthority(db, actor);
  if (!searchableMetrics.has(metric) || !hasPermission(actor, "finance.read") || actor.branchId || actor.departmentId)
    throw new ApiError(403, "EVIDENCE_FORBIDDEN", "This source record is not available in the current role or scope.");
  const catalog = await metricCatalogFor(db, actor);
  if (!catalog.some((item) => item.id === metric && item.available))
    throw new ApiError(403, "EVIDENCE_FORBIDDEN", "This source record is not available in the current role or scope.");
  await assertProductionModuleEnabled(db, actor.companyId, "finance");
}

function cursorValue(cursor?: string) {
  if (!cursor) return null;
  try {
    const value = JSON.parse(Buffer.from(cursor, "base64url").toString("utf8")) as { invoiceDate?: unknown; id?: unknown };
    if (typeof value.invoiceDate !== "string" || typeof value.id !== "string") throw new Error();
    return { invoiceDate: z.string().date().parse(value.invoiceDate), id: z.string().min(1).max(200).parse(value.id) };
  } catch {
    throw new ApiError(422, "EVIDENCE_CURSOR_INVALID", "The source record cursor is invalid. Refresh the evidence list.");
  }
}

function encodeCursor(row: Pick<InvoiceEvidenceRow, "invoice_date" | "id">) {
  return Buffer.from(JSON.stringify({ invoiceDate: row.invoice_date, id: row.id })).toString("base64url");
}

async function audit(db: SqlDatabase, actor: WorkContext, provider: string, metric: MetricId, action: "search" | "open", rows: { id: string; version: number | null }[], fields: string[], queryHash: string) {
  if (!rows.length && action === "search") return;
  const now = Date.now();
  const auditRows = rows.length ? rows : [{ id: "", version: 0 }];
  await db.prepare(`INSERT INTO executive_evidence_access_events
    (id,company_id,actor_id,assignment_id,provider_id,metric_id,action,record_id,record_version,field_ids,query_hash,created_at)
    SELECT 'executive-evidence-'||gen_random_uuid()::text,?,?,?,?,?,?,NULLIF(item.record_id,''),NULLIF(item.record_version,0),?::jsonb,?,?
    FROM jsonb_to_recordset(?::jsonb) AS item(record_id text,record_version bigint)`)
    .bind(actor.companyId, actor.userId, actor.assignmentId, provider, metric, action, JSON.stringify(fields), queryHash, now, JSON.stringify(auditRows.map((row) => ({ record_id: row.id, record_version: row.version })))).run();
}

type LegalEvidenceRow = {
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
type LegalHoldEvidenceRow = {
  id: string;
  item_id: string;
  state: string;
  created_at: number;
  item_title: string;
  item_kind: string;
  item_version: number;
};

async function requireLegalEvidence(db: SqlDatabase, actor: WorkContext, metric: LegalMetricId) {
  await currentAuthority(db, actor);
  if (!hasPermission(actor, "legal.read"))
    throw new ApiError(403, "EVIDENCE_FORBIDDEN", "This source record is not available in the current role or scope.");
  const catalog = await metricCatalogFor(db, actor);
  if (!catalog.some((item) => item.id === metric && item.available))
    throw new ApiError(403, "EVIDENCE_FORBIDDEN", "This source record is not available in the current role or scope.");
}

function legalEvidenceCursor(cursor?: string) {
  if (!cursor) return null;
  try {
    const value = JSON.parse(Buffer.from(cursor, "base64url").toString("utf8")) as { id?: unknown };
    return { id: z.string().min(1).max(200).parse(value.id) };
  } catch {
    throw new ApiError(422, "EVIDENCE_CURSOR_INVALID", "The source record cursor is invalid. Refresh the evidence list.");
  }
}

function encodeLegalEvidenceCursor(id: string) {
  return Buffer.from(JSON.stringify({ id })).toString("base64url");
}

function activeSchedulePredicate() {
  return `i.kind IN ('contract','obligation','license','ip','matter')
    AND i.state NOT IN ('archived','closed','cancelled','terminated','expired','completed')
    AND s.state NOT IN ('completed','waived','cancelled','closed','acknowledged','accepted','billed')`;
}

async function searchLegalEvidence(db: SqlDatabase, actor: WorkContext, raw: unknown) {
  const input = legalQuerySchema.parse(raw);
  await requireLegalEvidence(db, actor, input.metric);
  const visibility = legalVisibility(actor), cursor = legalEvidenceCursor(input.cursor);
  const isObligation = input.metric === "active-legal-obligations";
  const conditions = [visibility.sql, isObligation
    ? activeSchedulePredicate()
    : "h.state IN ('active','release_requested')"];
  const values: unknown[] = [...visibility.values];
  if (input.query) {
    conditions.push(isObligation
      ? "(position(lower(?) in lower(s.title||' '||s.category||' '||i.title))>0)"
      : "(position(lower(?) in lower(i.title||' '||i.kind||' '||h.state))>0)");
    values.push(input.query);
  }
  if (cursor) {
    conditions.push(`${isObligation ? "s" : "h"}.id>?`);
    values.push(cursor.id);
  }
  const rows = isObligation
    ? (await db.prepare(`SELECT s.id,s.item_id,s.title,s.category,s.due_date,s.state,s.version,
          i.title item_title,i.kind item_kind,i.version item_version
        FROM legal_obligation_schedule s JOIN legal_items i ON i.id=s.item_id
        WHERE ${conditions.join(" AND ")} ORDER BY s.id LIMIT ?`).bind(...values, input.limit + 1).all<LegalEvidenceRow>()).results
    : (await db.prepare(`SELECT h.id,h.item_id,h.state,h.created_at,
          i.title item_title,i.kind item_kind,i.version item_version
        FROM legal_holds h JOIN legal_items i ON i.id=h.item_id
        WHERE ${conditions.join(" AND ")} ORDER BY h.id LIMIT ?`).bind(...values, input.limit + 1).all<LegalHoldEvidenceRow>()).results;
  const hasMore = rows.length > input.limit, items = rows.slice(0, input.limit);
  const fields = isObligation
    ? ["title", "category", "due_date", "state", "version", "legal_item.title", "legal_item.kind", "legal_item.version"]
    : ["state", "created_at", "legal_item.title", "legal_item.kind", "legal_item.version"];
  const queryHash = await sha256(JSON.stringify({ providerId: legalProviderId, metric: input.metric, filters: input.filters, query: input.query, cursor: input.cursor ?? null }));
  await currentAuthority(db, actor);
  await audit(db, actor, legalProviderId, input.metric, "search", items.flatMap((row) => [
    ...(isObligation ? [{ id: row.id, version: Number((row as LegalEvidenceRow).version) }] : []),
    { id: row.item_id, version: Number(row.item_version) },
  ]), fields, queryHash);
  await currentAuthority(db, actor);
  return {
    provider: legalProviderId,
    metric: input.metric,
    fields,
    items: items.map((row) => ({
      id: row.id,
      label: isObligation ? (row as LegalEvidenceRow).title : row.item_title,
      summary: isObligation
        ? `${(row as LegalEvidenceRow).category} · ${row.state}${(row as LegalEvidenceRow).due_date ? ` · Due ${(row as LegalEvidenceRow).due_date}` : ""} · ${row.item_kind}: ${row.item_title}`
        : `Legal hold · ${row.state} · ${row.item_kind}: ${row.item_title}`,
      status: row.state,
      version: Number("version" in row ? row.version : row.item_version),
      href: `/legal?record=${encodeURIComponent(row.item_id)}`,
    })),
    hasMore,
    nextCursor: hasMore && items.length ? encodeLegalEvidenceCursor(items.at(-1)!.id) : null,
    queryHash,
  };
}

async function readLegalEvidence(db: SqlDatabase, actor: WorkContext, metric: LegalMetricId, recordId: string, filters: Filters) {
  await requireLegalEvidence(db, actor, metric);
  const visibility = legalVisibility(actor), isObligation = metric === "active-legal-obligations";
  const row = isObligation
    ? await db.prepare(`SELECT s.id,s.item_id,s.title,s.category,s.due_date,s.state,s.version,
          i.title item_title,i.kind item_kind,i.version item_version
        FROM legal_obligation_schedule s JOIN legal_items i ON i.id=s.item_id
        WHERE s.id=? AND ${visibility.sql} AND ${activeSchedulePredicate()}`)
      .bind(recordId, ...visibility.values).first<LegalEvidenceRow>()
    : await db.prepare(`SELECT h.id,h.item_id,h.state,h.created_at,
          i.title item_title,i.kind item_kind,i.version item_version
        FROM legal_holds h JOIN legal_items i ON i.id=h.item_id
        WHERE h.id=? AND ${visibility.sql} AND h.state IN ('active','release_requested')`)
      .bind(recordId, ...visibility.values).first<LegalHoldEvidenceRow>();
  if (!row) throw new ApiError(404, "EVIDENCE_NOT_FOUND", "This source record is unavailable in the selected role, scope or current state.");
  const record = isObligation
    ? { id: row.id, item_id: row.item_id, title: (row as LegalEvidenceRow).title, category: (row as LegalEvidenceRow).category, due_date: (row as LegalEvidenceRow).due_date, status: row.state, version: Number((row as LegalEvidenceRow).version), item_title: row.item_title, item_kind: row.item_kind, item_version: Number(row.item_version), href: `/legal?record=${encodeURIComponent(row.item_id)}` }
    : { id: row.id, item_id: row.item_id, status: row.state, created_at: Number((row as LegalHoldEvidenceRow).created_at), item_title: row.item_title, item_kind: row.item_kind, item_version: Number(row.item_version), href: `/legal?record=${encodeURIComponent(row.item_id)}` };
  const recordVersion = Number("version" in row ? row.version : row.item_version);
  const fields = isObligation
    ? ["title", "category", "due_date", "state", "version", "legal_item.title", "legal_item.kind", "legal_item.version"]
    : ["state", "created_at", "legal_item.title", "legal_item.kind", "legal_item.version"];
  const queryHash = await sha256(JSON.stringify({ providerId: legalProviderId, metric, filters, recordId }));
  await currentAuthority(db, actor);
  await audit(db, actor, legalProviderId, metric, "open", [
    { id: row.id, version: isObligation ? recordVersion : null },
    { id: row.item_id, version: Number(row.item_version) },
  ], fields, queryHash);
  await currentAuthority(db, actor);
  return { provider: legalProviderId, metric, fields, record, queryHash };
}

export async function searchFinanceEvidence(db: SqlDatabase, actor: WorkContext, raw: unknown) {
  const input = querySchema.parse(raw);
  await requireFinanceEvidence(db, actor, input.metric);
  const type = predicate(input.metric), cursor = cursorValue(input.cursor);
  const conditions = ["company_id=?", "kind=?", "currency=?", "invoice_date BETWEEN ? AND ?", `status IN (${type.statuses.map(() => "?").join(",")})`];
  const values: unknown[] = [actor.companyId, type.kind, input.filters.currency, input.filters.from, input.filters.to, ...type.statuses];
  if (input.query) {
    conditions.push("position(lower(?) in lower(invoice_number))>0");
    values.push(input.query);
  }
  if (cursor) {
    conditions.push("(invoice_date,id)<(?,?)");
    values.push(cursor.invoiceDate, cursor.id);
  }
  const rows = (await db.prepare(`SELECT id,invoice_number,kind,invoice_date,due_date,currency,total_minor::text total_minor,
      outstanding_minor::text outstanding_minor,status,version
    FROM finance_invoices WHERE ${conditions.join(" AND ")}
    ORDER BY invoice_date DESC,id DESC LIMIT ?`).bind(...values, input.limit + 1).all<InvoiceEvidenceRow>()).results;
  const hasMore = rows.length > input.limit, items = rows.slice(0, input.limit);
  const queryHash = await sha256(JSON.stringify({ providerId, metric: input.metric, filters: input.filters, query: input.query, cursor: input.cursor ?? null }));
  await currentAuthority(db, actor);
  await audit(db, actor, providerId, input.metric, "search", items.map(({ id, version }) => ({ id, version })), searchFields, queryHash);
  await currentAuthority(db, actor);
  return {
    provider: providerId,
    metric: input.metric,
    fields: searchFields,
    items: items.map((row) => ({ ...row, label: row.invoice_number, summary: `${row.kind} · ${row.invoice_date} · ${row.status}`, amount: row.total_minor, href: `/finance/invoices/${encodeURIComponent(row.id)}` })),
    hasMore,
    nextCursor: hasMore && items.length ? encodeCursor(items.at(-1)!) : null,
    queryHash,
  };
}

export async function readFinanceEvidence(db: SqlDatabase, actor: WorkContext, metric: FinanceMetricId, recordId: string, filters: Filters) {
  await requireFinanceEvidence(db, actor, metric);
  const type = predicate(metric);
  const row = await db.prepare(`SELECT i.id,i.invoice_number,i.kind,i.invoice_date,i.due_date,i.currency,i.subtotal_minor::text subtotal_minor,
      i.tax_minor::text tax_minor,i.total_minor::text total_minor,i.outstanding_minor::text outstanding_minor,i.status,i.version,i.posted_at,p.name party_name
    FROM finance_invoices i JOIN finance_parties p ON p.id=i.party_id AND p.company_id=i.company_id
    WHERE i.id=? AND i.company_id=? AND i.kind=? AND i.currency=? AND i.invoice_date BETWEEN ? AND ?
      AND i.status IN (${type.statuses.map(() => "?").join(",")})`)
    .bind(recordId, actor.companyId, type.kind, filters.currency, filters.from, filters.to, ...type.statuses).first<InvoiceEvidenceRow & { subtotal_minor: string; tax_minor: string; posted_at: number | null; party_name: string }>();
  if (!row) throw new ApiError(404, "EVIDENCE_NOT_FOUND", "This source record is unavailable in the selected role, scope or filters.");
  const queryHash = await sha256(JSON.stringify({ providerId, metric, filters, recordId }));
  await currentAuthority(db, actor);
  await audit(db, actor, providerId, metric, "open", [{ id: row.id, version: row.version }], detailFields, queryHash);
  await currentAuthority(db, actor);
  return {
    provider: providerId,
    metric,
    fields: detailFields,
    record: { ...row, href: `/finance/invoices/${encodeURIComponent(row.id)}` },
    queryHash,
  };
}

type PayrollSummaryEvidenceRow = {
  id: string;
  period_start: string;
  period_end: string;
  pay_date: string;
  currency: string;
  status: string;
  version: number;
  net_minor: string;
};

async function requirePayrollSummaryEvidence(db: SqlDatabase, actor: WorkContext) {
  await currentAuthority(db, actor);
  if (!hasPermission(actor, "payroll.read") || !hasPermission(actor, "module.people_payroll.read") || actor.branchId || actor.departmentId)
    throw new ApiError(403, "EVIDENCE_FORBIDDEN", "This source record is not available in the current role or scope.");
  const catalog = await metricCatalogFor(db, actor);
  if (!catalog.some((item) => item.id === payrollSummaryMetric && item.available))
    throw new ApiError(403, "EVIDENCE_FORBIDDEN", "This source record is not available in the current role or scope.");
  await assertProductionModuleEnabled(db, actor.companyId, "people_payroll");
}

function payrollSummaryCursor(cursor?: string) {
  if (!cursor) return null;
  try {
    const value = JSON.parse(Buffer.from(cursor, "base64url").toString("utf8")) as { periodEnd?: unknown; id?: unknown };
    return { periodEnd: z.string().date().parse(value.periodEnd), id: z.string().min(1).max(200).parse(value.id) };
  } catch {
    throw new ApiError(422, "EVIDENCE_CURSOR_INVALID", "The source record cursor is invalid. Refresh the evidence list.");
  }
}

function encodePayrollSummaryCursor(row: Pick<PayrollSummaryEvidenceRow, "period_end" | "id">) {
  return Buffer.from(JSON.stringify({ periodEnd: row.period_end, id: row.id })).toString("base64url");
}

function payrollSummaryRows(db: SqlDatabase, actor: WorkContext, filters: Filters, query = "", cursor?: { periodEnd: string; id: string }, recordId?: string, limit = 25) {
  const conditions = [
    "r.company_id=?",
    "r.currency=?",
    "r.period_end BETWEEN ? AND ?",
    "r.status='posted'",
    "EXISTS(SELECT 1 FROM hr_payroll_postings p JOIN finance_journals j ON j.id=p.journal_id WHERE p.run_id=r.id AND j.company_id=r.company_id AND j.status='posted')",
    "EXISTS(SELECT 1 FROM hr_payroll_items posted_item WHERE posted_item.run_id=r.id AND posted_item.status='posted')",
  ];
  const values: unknown[] = [actor.companyId, filters.currency, filters.from, filters.to];
  if (query) {
    conditions.push("(position(lower(?) in lower(r.id||' '||r.period_start||' '||r.period_end||' '||r.pay_date||' '||r.currency||' '||r.status))>0)");
    values.push(query);
  }
  if (cursor) {
    conditions.push("(r.period_end,r.id)<(?,?)");
    values.push(cursor.periodEnd, cursor.id);
  }
  if (recordId) {
    conditions.push("r.id=?");
    values.push(recordId);
  }
  return db.prepare(`SELECT r.id,r.period_start,r.period_end,r.pay_date,r.currency,r.status,r.version,
      COALESCE((SELECT sum(i.net_minor::numeric)::text FROM hr_payroll_items i WHERE i.run_id=r.id AND i.status='posted'),'0') net_minor
    FROM hr_payroll_runs r WHERE ${conditions.join(" AND ")}
    ORDER BY r.period_end DESC,r.id DESC LIMIT ?`).bind(...values, recordId ? 1 : limit);
}

async function searchPayrollSummaryEvidence(db: SqlDatabase, actor: WorkContext, input: z.infer<typeof payrollSummaryQuerySchema>) {
  await requirePayrollSummaryEvidence(db, actor);
  const cursor = payrollSummaryCursor(input.cursor);
  const rows = (await payrollSummaryRows(db, actor, input.filters, input.query, cursor ?? undefined, undefined, input.limit + 1).all<PayrollSummaryEvidenceRow>()).results;
  const hasMore = rows.length > input.limit, items = rows.slice(0, input.limit);
  const queryHash = await sha256(JSON.stringify({ providerId: payrollSummaryProviderId, metric: input.metric, filters: input.filters, scope: [actor.companyId, actor.assignmentId], query: input.query, cursor: input.cursor ?? null }));
  await currentAuthority(db, actor);
  await audit(db, actor, payrollSummaryProviderId, input.metric, "search", items.map((row) => ({ id: row.id, version: row.version })), payrollSummaryFields, queryHash);
  await currentAuthority(db, actor);
  return {
    provider: payrollSummaryProviderId,
    metric: input.metric,
    fields: payrollSummaryFields,
    items: items.map((row) => ({
      id: row.id,
      label: `${row.period_start} to ${row.period_end}`,
      summary: `Posted · ${row.currency} · Pay date ${row.pay_date}`,
      amount: row.net_minor,
      currency: row.currency,
      status: row.status,
      version: row.version,
      href: `/people/payroll/runs/${encodeURIComponent(row.id)}`,
    })),
    hasMore,
    nextCursor: hasMore && items.length ? encodePayrollSummaryCursor(items.at(-1)!) : null,
    queryHash,
  };
}

async function readPayrollSummaryEvidence(db: SqlDatabase, actor: WorkContext, recordId: string, filters: Filters) {
  await requirePayrollSummaryEvidence(db, actor);
  const row = await payrollSummaryRows(db, actor, filters, "", undefined, recordId, 1).first<PayrollSummaryEvidenceRow>();
  if (!row) throw new ApiError(404, "EVIDENCE_NOT_FOUND", "This source record is unavailable in the selected role, scope or filters.");
  const record = { ...row, href: `/people/payroll/runs/${encodeURIComponent(row.id)}` };
  const queryHash = await sha256(JSON.stringify({ providerId: payrollSummaryProviderId, metric: payrollSummaryMetric, filters, scope: [actor.companyId, actor.assignmentId], recordId }));
  await currentAuthority(db, actor);
  await audit(db, actor, payrollSummaryProviderId, payrollSummaryMetric, "open", [{ id: row.id, version: row.version }], payrollSummaryFields, queryHash);
  await currentAuthority(db, actor);
  return { provider: payrollSummaryProviderId, metric: payrollSummaryMetric, fields: payrollSummaryFields, record, queryHash };
}

async function requirePeopleLeaveOvertimeEvidence(db: SqlDatabase, actor: WorkContext, metric: typeof approvedLeaveMetric | typeof authorizedOvertimeMetric) {
  await currentAuthority(db, actor);
  if (!hasPermission(actor, "people.read") || !hasPermission(actor, "module.people_payroll.read"))
    throw new ApiError(403, "EVIDENCE_FORBIDDEN", "This source record is not available in the current role or scope.");
  const catalog = await metricCatalogFor(db, actor);
  if (!catalog.some((item) => item.id === metric && item.available))
    throw new ApiError(403, "EVIDENCE_FORBIDDEN", "This source record is not available in the current role or scope.");
  await assertProductionModuleEnabled(db, actor.companyId, "people_payroll");
}

function peopleRequestCursor(cursor?: string) {
  if (!cursor) return null;
  try {
    const value = JSON.parse(Buffer.from(cursor, "base64url").toString("utf8")) as { date?: unknown; id?: unknown };
    return { date: z.string().date().parse(value.date), id: z.string().min(1).max(200).parse(value.id) };
  } catch {
    throw new ApiError(422, "EVIDENCE_CURSOR_INVALID", "The source record cursor is invalid. Refresh the evidence list.");
  }
}

function encodePeopleRequestCursor(date: string, id: string) {
  return Buffer.from(JSON.stringify({ date, id })).toString("base64url");
}

function approvedLeaveEvidenceRows(db: SqlDatabase, actor: WorkContext, filters: Filters, query = "", cursor?: { date: string; id: string }, recordId?: string, limit = 25) {
  const conditions = [
    "l.company_id=?", "l.state='approved'", "l.start_date BETWEEN ? AND ?",
    "(?::text IS NULL OR l.branch_id=?)", "(?::text IS NULL OR l.department_id=?)",
    `(l.created_by=? OR l.decided_by=? OR ${participantSql("leave", "l", "?", "?")} OR ${participantSql("leave_cancellation", "l", "?", "?")})`,
  ];
  const values: unknown[] = [actor.companyId, filters.from, filters.to, actor.branchId, actor.branchId, actor.departmentId, actor.departmentId,
    actor.userId, actor.userId, actor.userId, actor.assignmentId, actor.userId, actor.assignmentId];
  if (query) {
    conditions.push("position(lower(?) in lower(l.id||' '||l.number||' '||l.start_date||' '||COALESCE(l.end_date,l.start_date)||' '||l.state))>0");
    values.push(query);
  }
  if (cursor) {
    conditions.push("(l.start_date,l.id)<(?,?)");
    values.push(cursor.date, cursor.id);
  }
  if (recordId) {
    conditions.push("l.id=?");
    values.push(recordId);
  }
  return db.prepare(`SELECT l.id,l.number,l.start_date,COALESCE(l.end_date,l.start_date) end_date,
      COALESCE(l.calculated_units::numeric,l.days::numeric)::text units,l.days,l.state,l.version
    FROM leave_requests l WHERE ${conditions.join(" AND ")}
    ORDER BY l.start_date DESC,l.id DESC LIMIT ?`).bind(...values, recordId ? 1 : limit);
}

function authorizedOvertimeEvidenceRows(db: SqlDatabase, actor: WorkContext, filters: Filters, query = "", cursor?: { date: string; id: string }, recordId?: string, limit = 25) {
  const conditions = [
    "o.company_id=?", "o.state='approved'", "h.work_date BETWEEN ? AND ?",
    "(?::text IS NULL OR o.branch_id=?)", "(?::text IS NULL OR o.department_id=?)",
    `(o.created_by=? OR ${participantSql("overtime", "o", "?", "?")})`,
  ];
  const values: unknown[] = [actor.companyId, filters.from, filters.to, actor.branchId, actor.branchId, actor.departmentId, actor.departmentId,
    actor.userId, actor.userId, actor.assignmentId];
  if (query) {
    conditions.push("position(lower(?) in lower(o.id||' '||h.work_date||' '||o.state))>0");
    values.push(query);
  }
  if (cursor) {
    conditions.push("(h.work_date,o.id)<(?,?)");
    values.push(cursor.date, cursor.id);
  }
  if (recordId) {
    conditions.push("o.id=?");
    values.push(recordId);
  }
  return db.prepare(`SELECT o.id,h.work_date,h.minutes::text minutes,h.request_version,o.state,o.version
    FROM overtime_payroll_handoffs h JOIN overtime_requests o ON o.id=h.request_id AND o.company_id=h.company_id
    WHERE ${conditions.join(" AND ")}
    ORDER BY h.work_date DESC,o.id DESC LIMIT ?`).bind(...values, recordId ? 1 : limit);
}

async function searchPeopleLeaveOvertimeEvidence(db: SqlDatabase, actor: WorkContext, raw: unknown) {
  const input = peopleLeaveOvertimeQuerySchema.parse(raw), cursor = peopleRequestCursor(input.cursor);
  await requirePeopleLeaveOvertimeEvidence(db, actor, input.metric);
  const leave = input.metric === approvedLeaveMetric;
  const fields = leave ? approvedLeaveFields : authorizedOvertimeFields;
  const rows = (await (leave
    ? approvedLeaveEvidenceRows(db, actor, input.filters, input.query, cursor ?? undefined, undefined, input.limit + 1)
    : authorizedOvertimeEvidenceRows(db, actor, input.filters, input.query, cursor ?? undefined, undefined, input.limit + 1))
    .all<{
      id: string; number?: string; start_date?: string; end_date?: string; units?: string; days?: number;
      work_date?: string; minutes?: string; request_version?: number; state: string; version: number;
    }>()).results;
  const hasMore = rows.length > input.limit, items = rows.slice(0, input.limit);
  const queryHash = await sha256(JSON.stringify({ providerId: peopleLeaveOvertimeProviderId, metric: input.metric, filters: input.filters, scope: [actor.companyId, actor.assignmentId, actor.branchId, actor.departmentId, actor.userId], query: input.query, cursor: input.cursor ?? null }));
  await currentAuthority(db, actor);
  await audit(db, actor, peopleLeaveOvertimeProviderId, input.metric, "search", items.map((row) => ({ id: row.id, version: row.version })), fields, queryHash);
  await currentAuthority(db, actor);
  return {
    provider: peopleLeaveOvertimeProviderId,
    metric: input.metric,
    fields,
    items: items.map((row) => leave ? ({
      id: row.id,
      label: row.number!,
      summary: `Approved leave · ${row.start_date} to ${row.end_date} · ${row.units} days`,
      status: row.state,
      version: row.version,
      href: `/tasks/people_payroll/${encodeURIComponent(row.id)}`,
    }) : ({
      id: row.id,
      label: `Overtime · ${row.work_date}`,
      summary: `Approved payroll handoff · ${row.minutes} minutes`,
      status: row.state,
      version: row.version,
      href: `/overtime/${encodeURIComponent(row.id)}`,
    })),
    hasMore,
    nextCursor: hasMore && items.length ? encodePeopleRequestCursor(leave ? items.at(-1)!.start_date! : items.at(-1)!.work_date!, items.at(-1)!.id) : null,
    queryHash,
  };
}

async function readPeopleLeaveOvertimeEvidence(db: SqlDatabase, actor: WorkContext, metric: typeof approvedLeaveMetric | typeof authorizedOvertimeMetric, recordId: string, filters: Filters) {
  await requirePeopleLeaveOvertimeEvidence(db, actor, metric);
  const leave = metric === approvedLeaveMetric;
  const fields = leave ? approvedLeaveFields : authorizedOvertimeFields;
  let record: Record<string, unknown>;
  if (leave) {
    // Keep evidence reads on the exact same personal cohort as search and the
    // aggregate. `getLeave` also serves the separately-authorized medical
    // evidence workflow, whose broader `leave.evidence.read` grant must not
    // widen this executive metric.
    const row = await approvedLeaveEvidenceRows(db, actor, filters, "", undefined, recordId, 1)
      .first<{
        id: string; number: string; start_date: string; end_date: string;
        units: string; days: number; state: string; version: number;
      }>();
    if (!row) {
      const visible = await db.prepare(`SELECT 1 FROM leave_requests l
        WHERE l.id=? AND l.company_id=?
          AND (l.created_by=? OR l.decided_by=? OR ${participantSql("leave", "l", "?", "?")}
            OR ${participantSql("leave_cancellation", "l", "?", "?")})`)
        .bind(recordId, actor.companyId, actor.userId, actor.userId, actor.userId, actor.assignmentId, actor.userId, actor.assignmentId)
        .first();
      if (!visible) throw new ApiError(404, "NOT_FOUND", "Leave request not found.");
      throw new ApiError(404, "EVIDENCE_NOT_FOUND", "This source record is unavailable in the selected role, scope or filters.");
    }
    record = {
      id: row.id, number: row.number, start_date: row.start_date, end_date: row.end_date,
      units: row.units, state: row.state, version: row.version,
      href: `/tasks/people_payroll/${encodeURIComponent(row.id)}`,
    };
  } else {
    const row = await getOvertime(actor, recordId, db);
    const handoff = row.handoff as { work_date?: string; minutes?: number; request_version?: number } | null;
    if (row.state !== "approved" || !handoff || !handoff.work_date || handoff.work_date < filters.from || handoff.work_date > filters.to
      || handoff.request_version !== row.version
      || actor.branchId && row.branch_id !== actor.branchId
      || actor.departmentId && row.department_id !== actor.departmentId)
      throw new ApiError(404, "EVIDENCE_NOT_FOUND", "This source record is unavailable in the selected role, scope or filters.");
    record = {
      id: row.id, work_date: handoff.work_date, minutes: String(handoff.minutes ?? 0), request_version: handoff.request_version,
      state: row.state, version: row.version, href: `/overtime/${encodeURIComponent(row.id)}`,
    };
  }
  const queryHash = await sha256(JSON.stringify({ providerId: peopleLeaveOvertimeProviderId, metric, filters, scope: [actor.companyId, actor.assignmentId, actor.branchId, actor.departmentId, actor.userId], recordId }));
  await currentAuthority(db, actor);
  await audit(db, actor, peopleLeaveOvertimeProviderId, metric, "open", [{ id: recordId, version: Number(record.version) }], fields, queryHash);
  await currentAuthority(db, actor);
  return { provider: peopleLeaveOvertimeProviderId, metric, fields, record, queryHash };
}

type HotelOccupancyEvidenceRow = {
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

async function requireHotelOccupancyEvidence(db: SqlDatabase, actor: WorkContext, metric: typeof hotelOccupancyMetric | HotelRoomRevenueMetric) {
  await currentAuthority(db, actor);
  if (!hasPermission(actor, "pms.read") || !hasPermission(actor, "pms.reports") || actor.departmentId || actor.beta)
    throw new ApiError(403, "EVIDENCE_FORBIDDEN", "This source record is not available in the current role or scope.");
  const catalog = await metricCatalogFor(db, actor);
  if (!catalog.some((item) => item.id === metric && item.available))
    throw new ApiError(403, "EVIDENCE_FORBIDDEN", "This source record is not available in the current role or scope.");
  await assertProductionModuleEnabled(db, actor.companyId, "pms");
}

function hotelOccupancyCursorValue(cursor?: string) {
  if (!cursor) return null;
  try {
    const value = JSON.parse(Buffer.from(cursor, "base64url").toString("utf8")) as { businessDate?: unknown; id?: unknown };
    return { businessDate: z.string().date().parse(value.businessDate), id: z.string().min(1).max(200).parse(value.id) };
  } catch {
    throw new ApiError(422, "EVIDENCE_CURSOR_INVALID", "The source record cursor is invalid. Refresh the evidence list.");
  }
}

function encodeHotelOccupancyCursor(row: Pick<HotelOccupancyEvidenceRow, "business_date" | "id">) {
  return Buffer.from(JSON.stringify({ businessDate: row.business_date, id: row.id })).toString("base64url");
}

function hotelOccupancyValidSql(alias = "r") {
  const capacity = `(${alias}.data_json::jsonb->'roomMetrics'->>'capacity')`;
  const sold = `(${alias}.data_json::jsonb->'roomMetrics'->>'sold')`;
  return `(CASE WHEN ${capacity} ~ '^[1-9][0-9]{0,8}$' AND ${sold} ~ '^(0|[1-9][0-9]{0,8})$' THEN CASE WHEN ${capacity}::numeric>0 THEN ${sold}::numeric<=${capacity}::numeric ELSE FALSE END ELSE FALSE END)`;
}

function hotelOccupancyRows(db: SqlDatabase, actor: WorkContext, filters: Filters, metric: typeof hotelOccupancyMetric | HotelRoomRevenueMetric = hotelOccupancyMetric, query = "", cursor?: { businessDate: string; id: string }, recordId?: string, limit = 25) {
  const conditions = [
    "p.company_id=?",
    "(?::text IS NULL OR p.branch_id=?)",
    "r.kind='night'",
    "r.data_json::jsonb->>'state'='completed'",
    "r.data_json::jsonb->>'date' BETWEEN ? AND ?",
    hotelOccupancyValidSql(),
  ];
  const values: unknown[] = [actor.companyId, actor.branchId, actor.branchId, filters.from, filters.to];
  if (metric !== hotelOccupancyMetric) {
    conditions.push("p.config_json::jsonb->>'currency'=?");
    values.push(filters.currency);
  }
  if (query) {
    conditions.push("position(lower(?) in lower(concat_ws(' ',r.id,p.name,r.data_json::jsonb->>'date'))) > 0");
    values.push(query);
  }
  if (cursor) {
    conditions.push("(r.data_json::jsonb->>'date',r.id)<(?,?)");
    values.push(cursor.businessDate, cursor.id);
  }
  if (recordId) {
    conditions.push("r.id=?");
    values.push(recordId);
  }
  return db.prepare(`SELECT r.id,r.property_id,p.name property_name,p.config_json::jsonb->>'currency' property_currency,r.data_json::jsonb->>'date' business_date,
      r.data_json::jsonb->>'state' state,r.data_json::jsonb->'roomMetrics'->>'capacity' capacity,
      r.data_json::jsonb->'roomMetrics'->>'sold' sold,r.version
    FROM pms_records r JOIN pms_properties p ON p.id=r.property_id
    WHERE ${conditions.join(" AND ")}
    ORDER BY r.data_json::jsonb->>'date' DESC,r.id DESC LIMIT ?`).bind(...values, recordId ? 1 : limit + 1);
}

async function searchHotelOccupancyEvidence(db: SqlDatabase, actor: WorkContext, input: z.infer<typeof hotelOccupancyQuerySchema>) {
  await requireHotelOccupancyEvidence(db, actor, input.metric);
  const cursor = hotelOccupancyCursorValue(input.cursor);
  const rows = (await hotelOccupancyRows(db, actor, input.filters, input.metric, input.query, cursor ?? undefined, undefined, input.limit).all<HotelOccupancyEvidenceRow>()).results;
  const hasMore = rows.length > input.limit, items = rows.slice(0, input.limit);
  const queryHash = await sha256(JSON.stringify({ providerId: hotelOccupancyProviderId, metric: input.metric, filters: input.filters, scope: [actor.companyId, actor.assignmentId, actor.branchId], query: input.query, cursor: input.cursor ?? null }));
  await currentAuthority(db, actor);
  await audit(db, actor, hotelOccupancyProviderId, input.metric, "search", items.map(({ id, version }) => ({ id, version })), hotelOccupancyFields, queryHash);
  await currentAuthority(db, actor);
  return {
    provider: hotelOccupancyProviderId,
    metric: input.metric,
    fields: hotelOccupancyFields,
    items: items.map((row) => ({
      id: row.id,
      label: `${row.property_name} · ${row.business_date}`,
      summary: `Completed Night Audit · ${row.sold} sold / ${row.capacity} sellable room nights`,
      status: row.state,
      version: row.version,
      href: `/pms/reports?property=${encodeURIComponent(row.property_id)}`,
    })),
    hasMore,
    nextCursor: hasMore && items.length ? encodeHotelOccupancyCursor(items.at(-1)!) : null,
    queryHash,
  };
}

async function hotelNightRoomRevenue(db: SqlDatabase, actor: WorkContext, propertyId: string, businessDate: string) {
  return db.prepare(`WITH RECURSIVE candidate_entries AS (
      SELECT e.id,e.property_id,e.data_json::jsonb->>'net' net_raw,
        e.data_json::jsonb->>'sourceKey' source_key,e.data_json::jsonb->>'originalId' original_id
      FROM pms_records e JOIN pms_properties p ON p.id=e.property_id
      WHERE p.id=? AND p.company_id=? AND (?::text IS NULL OR p.branch_id=?)
        AND e.kind='entry' AND e.data_json::jsonb->>'date'=?
        AND e.data_json::jsonb->>'kind' IN ('charge','adjustment')
    ), lineage(entry_id,property_id,net_raw,current_id,source_key,original_id,visited) AS (
      SELECT id,property_id,net_raw,id,source_key,original_id,ARRAY[id]::text[] FROM candidate_entries
      UNION ALL
      SELECT l.entry_id,l.property_id,l.net_raw,parent.id,parent.data_json::jsonb->>'sourceKey',
        parent.data_json::jsonb->>'originalId',l.visited||parent.id
      FROM lineage l JOIN pms_records parent ON parent.id=l.original_id AND parent.property_id=l.property_id AND parent.kind='entry'
      WHERE COALESCE(l.source_key,'') NOT LIKE 'night:%' AND NOT (parent.id=ANY(l.visited))
    )
    SELECT COALESCE(sum(CASE WHEN e.net_raw ~ '^-?(0|[1-9][0-9]*)$' THEN e.net_raw::numeric ELSE 0::numeric END),0)::text revenue_minor,
      count(*) FILTER (WHERE e.net_raw IS NULL OR e.net_raw !~ '^-?(0|[1-9][0-9]*)$')::text invalid_revenue_count
    FROM candidate_entries e
    WHERE EXISTS(SELECT 1 FROM lineage l WHERE l.entry_id=e.id AND l.source_key LIKE 'night:%')`)
    .bind(propertyId, actor.companyId, actor.branchId, actor.branchId, businessDate)
    .first<{ revenue_minor: string; invalid_revenue_count: string }>();
}

async function readHotelOccupancyEvidence(db: SqlDatabase, actor: WorkContext, metric: typeof hotelOccupancyMetric | HotelRoomRevenueMetric, recordId: string, filters: Filters) {
  await requireHotelOccupancyEvidence(db, actor, metric);
  const row = await hotelOccupancyRows(db, actor, filters, metric, "", undefined, recordId, 1).first<HotelOccupancyEvidenceRow>();
  if (!row) throw new ApiError(404, "EVIDENCE_NOT_FOUND", "This source record is unavailable in the selected role, scope or filters.");
  const capacity = Number(row.capacity), sold = Number(row.sold);
  if (row.state !== "completed" || !Number.isSafeInteger(capacity) || capacity < 1 || !Number.isSafeInteger(sold) || sold < 0 || sold > capacity)
    throw new ApiError(404, "EVIDENCE_NOT_FOUND", "This source record is unavailable in the selected role, scope or filters.");
  const record = {
    id: row.id,
    property_id: row.property_id,
    property_name: row.property_name,
    currency: row.property_currency,
    business_date: row.business_date,
    state: row.state,
    sold: String(sold),
    capacity: String(capacity),
    version: row.version,
    href: `/pms/reports?property=${encodeURIComponent(row.property_id)}`,
  };
  const revenue = metric === hotelOccupancyMetric ? null : await hotelNightRoomRevenue(db, actor, row.property_id, row.business_date);
  if (metric !== hotelOccupancyMetric) {
    Object.assign(record, { room_revenue_minor: revenue?.revenue_minor ?? "0", invalid_revenue_count: revenue?.invalid_revenue_count ?? "0" });
  }
  const fields = metric === hotelOccupancyMetric ? hotelOccupancyFields : hotelRoomRevenueFields;
  const queryHash = await sha256(JSON.stringify({ providerId: hotelOccupancyProviderId, metric, filters, scope: [actor.companyId, actor.assignmentId, actor.branchId], recordId }));
  await currentAuthority(db, actor);
  await audit(db, actor, hotelOccupancyProviderId, metric, "open", [{ id: record.id, version: record.version }], fields, queryHash);
  await currentAuthority(db, actor);
  return { provider: hotelOccupancyProviderId, metric, fields, record, queryHash };
}

type HotelFolioBalanceEvidenceRow = {
  id: string;
  property_id: string;
  property_name: string;
  currency: string;
  state: "open" | "credit";
  balance_minor: string;
  version: number;
};

async function requireHotelFolioBalanceEvidence(db: SqlDatabase, actor: WorkContext) {
  await currentAuthority(db, actor);
  if (!hasPermission(actor, "pms.read") || !hasPermission(actor, "pms.reports") || actor.departmentId || actor.beta)
    throw new ApiError(403, "EVIDENCE_FORBIDDEN", "This source record is not available in the current role or scope.");
  const catalog = await metricCatalogFor(db, actor);
  if (!catalog.some((item) => item.id === hotelFolioBalanceMetric && item.available))
    throw new ApiError(403, "EVIDENCE_FORBIDDEN", "This source record is not available in the current role or scope.");
  await assertProductionModuleEnabled(db, actor.companyId, "pms");
}

function hotelFolioBalanceCursorValue(cursor?: string) {
  if (!cursor) return null;
  try {
    const value = JSON.parse(Buffer.from(cursor, "base64url").toString("utf8")) as { propertyId?: unknown; id?: unknown };
    return { propertyId: z.string().min(1).max(200).parse(value.propertyId), id: z.string().min(1).max(200).parse(value.id) };
  } catch {
    throw new ApiError(422, "EVIDENCE_CURSOR_INVALID", "The source record cursor is invalid. Refresh the evidence list.");
  }
}

function encodeHotelFolioBalanceCursor(row: Pick<HotelFolioBalanceEvidenceRow, "property_id" | "id">) {
  return Buffer.from(JSON.stringify({ propertyId: row.property_id, id: row.id })).toString("base64url");
}

function hotelFolioBalanceRows(
  db: SqlDatabase,
  actor: WorkContext,
  filters: Filters,
  query = "",
  cursor?: { propertyId: string; id: string },
  recordId?: string,
  limit = 25,
) {
  const conditions = [
    "p.company_id=?",
    "(?::text IS NULL OR p.branch_id=?)",
    "p.config_json::jsonb->>'currency'=?",
    "f.kind='folio'",
    "f.data_json::jsonb->>'state' IN ('open','credit')",
    "e.kind='entry'",
    "j.company_id=p.company_id",
    "j.status='posted'",
    "l.account_id=p.config_json::jsonb->'accounts'->>'receivable'",
  ];
  const values: unknown[] = [actor.companyId, actor.branchId, actor.branchId, filters.currency];
  if (query) {
    conditions.push("position(lower(?) in lower(concat_ws(' ',f.id,p.name,p.config_json::jsonb->>'currency',f.data_json::jsonb->>'state'))) > 0");
    values.push(query);
  }
  if (cursor) {
    conditions.push("(p.id,f.id)>(?,?)");
    values.push(cursor.propertyId, cursor.id);
  }
  if (recordId) {
    conditions.push("f.id=?");
    values.push(recordId);
  }
  return db.prepare(`SELECT f.id,p.id property_id,p.name property_name,
      p.config_json::jsonb->>'currency' currency,f.data_json::jsonb->>'state' state,
      sum(l.debit_minor::numeric-l.credit_minor::numeric)::text balance_minor,f.version
    FROM pms_records f
    JOIN pms_properties p ON p.id=f.property_id
    JOIN pms_records e ON e.property_id=p.id AND e.kind='entry' AND e.data_json::jsonb->>'folioId'=f.id
    JOIN pms_postings pp ON pp.property_id=p.id AND pp.record_id=e.id
    JOIN finance_journals j ON j.id=pp.journal_id
    JOIN finance_journal_lines l ON l.journal_id=j.id
    WHERE ${conditions.join(" AND ")}
    GROUP BY f.id,p.id,p.name,p.config_json,f.data_json,f.version
    HAVING sum(l.debit_minor::numeric-l.credit_minor::numeric)<>0
    ORDER BY p.id ASC,f.id ASC LIMIT ?`).bind(...values, recordId ? 1 : limit + 1);
}

async function searchHotelFolioBalanceEvidence(db: SqlDatabase, actor: WorkContext, input: z.infer<typeof hotelFolioBalanceQuerySchema>) {
  await requireHotelFolioBalanceEvidence(db, actor);
  const cursor = hotelFolioBalanceCursorValue(input.cursor);
  const rows = (await hotelFolioBalanceRows(db, actor, input.filters, input.query, cursor ?? undefined, undefined, input.limit).all<HotelFolioBalanceEvidenceRow>()).results;
  const hasMore = rows.length > input.limit, items = rows.slice(0, input.limit);
  const queryHash = await sha256(JSON.stringify({ providerId: hotelFolioBalanceProviderId, metric: input.metric, currency: input.filters.currency, scope: [actor.companyId, actor.assignmentId, actor.branchId], query: input.query, cursor: input.cursor ?? null }));
  await currentAuthority(db, actor);
  await audit(db, actor, hotelFolioBalanceProviderId, input.metric, "search", items.map(({ id, version }) => ({ id, version })), hotelFolioBalanceFields, queryHash);
  await currentAuthority(db, actor);
  return {
    provider: hotelFolioBalanceProviderId,
    metric: input.metric,
    fields: hotelFolioBalanceFields,
    items: items.map((row) => ({
      id: row.id,
      label: `${row.property_name} · Open folio`,
      summary: `${row.balance_minor} ${row.currency} posted balance · ${row.state}`,
      status: row.state,
      version: row.version,
      amount: row.balance_minor,
      currency: row.currency,
      href: `/pms/folios?property=${encodeURIComponent(row.property_id)}&folio=${encodeURIComponent(row.id)}`,
    })),
    hasMore,
    nextCursor: hasMore && items.length ? encodeHotelFolioBalanceCursor(items.at(-1)!) : null,
    queryHash,
  };
}

async function readHotelFolioBalanceEvidence(db: SqlDatabase, actor: WorkContext, recordId: string, filters: Filters) {
  await requireHotelFolioBalanceEvidence(db, actor);
  const row = await hotelFolioBalanceRows(db, actor, filters, "", undefined, recordId, 1).first<HotelFolioBalanceEvidenceRow>();
  if (!row) throw new ApiError(404, "EVIDENCE_NOT_FOUND", "This source record is unavailable in the selected role, scope or filters.");
  const snapshot = await loadPms(db, actor, row.property_id);
  const folio = snapshot.records.find((candidate): candidate is PmsRecord<"folio"> => candidate.id === row.id && candidate.kind === "folio");
  if (!folio || folio.version !== Number(row.version) || !["open", "credit"].includes(folio.data.state))
    throw new ApiError(404, "EVIDENCE_NOT_FOUND", "This source record is unavailable in the selected role, scope or filters.");
  const record = {
    id: folio.id,
    property_name: snapshot.property.name,
    currency: row.currency,
    state: folio.data.state,
    posted_receivable_balance_minor: row.balance_minor,
    version: folio.version,
    href: `/pms/folios?property=${encodeURIComponent(snapshot.property.id)}&folio=${encodeURIComponent(folio.id)}`,
  };
  const queryHash = await sha256(JSON.stringify({ providerId: hotelFolioBalanceProviderId, metric: hotelFolioBalanceMetric, currency: filters.currency, scope: [actor.companyId, actor.assignmentId, actor.branchId], recordId }));
  await currentAuthority(db, actor);
  await audit(db, actor, hotelFolioBalanceProviderId, hotelFolioBalanceMetric, "open", [{ id: record.id, version: record.version }], hotelFolioBalanceFields, queryHash);
  await currentAuthority(db, actor);
  return { provider: hotelFolioBalanceProviderId, metric: hotelFolioBalanceMetric, fields: hotelFolioBalanceFields, record, queryHash };
}

type HotelNightAuditBlockerEvidenceRow = {
  id: string;
  property_id: string;
  property_name: string;
  business_date: string;
  state: "blocked";
  error_code: string;
  version: number;
};

async function requireHotelNightAuditBlockerEvidence(db: SqlDatabase, actor: WorkContext) {
  await currentAuthority(db, actor);
  if (!hasPermission(actor, "pms.read") || !hasPermission(actor, "pms.reports") || actor.departmentId || actor.beta)
    throw new ApiError(403, "EVIDENCE_FORBIDDEN", "This source record is not available in the current role or scope.");
  const catalog = await metricCatalogFor(db, actor);
  if (!catalog.some((item) => item.id === hotelNightAuditBlockersMetric && item.available))
    throw new ApiError(403, "EVIDENCE_FORBIDDEN", "This source record is not available in the current role or scope.");
  await assertProductionModuleEnabled(db, actor.companyId, "pms");
}

function hotelNightAuditBlockerCursorValue(cursor?: string) {
  if (!cursor) return null;
  try {
    const value = JSON.parse(Buffer.from(cursor, "base64url").toString("utf8")) as { businessDate?: unknown; id?: unknown };
    return { businessDate: z.iso.date().parse(value.businessDate), id: z.string().min(1).max(200).parse(value.id) };
  } catch {
    throw new ApiError(422, "EVIDENCE_CURSOR_INVALID", "The source record cursor is invalid. Refresh the evidence list.");
  }
}

function encodeHotelNightAuditBlockerCursor(row: Pick<HotelNightAuditBlockerEvidenceRow, "business_date" | "id">) {
  return Buffer.from(JSON.stringify({ businessDate: row.business_date, id: row.id })).toString("base64url");
}

function hotelNightAuditBlockerRows(
  db: SqlDatabase,
  actor: WorkContext,
  filters: Filters,
  query = "",
  cursor?: { businessDate: string; id: string },
  recordId?: string,
  limit = 25,
) {
  const conditions = [
    "p.company_id=?",
    "(?::text IS NULL OR p.branch_id=?)",
    "j.business_date BETWEEN ? AND ?",
    "j.state='blocked'",
    "r.kind='night'",
    "r.data_json::jsonb->>'date'=j.business_date",
    "r.data_json::jsonb->>'state'='blocked'",
  ];
  const values: unknown[] = [actor.companyId, actor.branchId, actor.branchId, filters.from, filters.to];
  if (query) {
    conditions.push("position(lower(?) in lower(concat_ws(' ',p.name,j.business_date,j.error_code,j.id))) > 0");
    values.push(query);
  }
  if (cursor) {
    conditions.push("(j.business_date,j.id)<(?,?)");
    values.push(cursor.businessDate, cursor.id);
  }
  if (recordId) {
    conditions.push("j.id=?");
    values.push(recordId);
  }
  return db.prepare(`SELECT j.id,p.id property_id,p.name property_name,j.business_date,j.state,
      COALESCE(j.error_code,'PMS_JOB_FAILED') error_code,r.version
    FROM pms_jobs j JOIN pms_properties p ON p.id=j.property_id
    JOIN pms_records r ON r.id=j.id AND r.property_id=j.property_id
    WHERE ${conditions.join(" AND ")}
    ORDER BY j.business_date DESC,j.id DESC LIMIT ?`).bind(...values, recordId ? 1 : limit + 1);
}

async function searchHotelNightAuditBlockerEvidence(db: SqlDatabase, actor: WorkContext, input: z.infer<typeof hotelNightAuditBlockersQuerySchema>) {
  await requireHotelNightAuditBlockerEvidence(db, actor);
  const cursor = hotelNightAuditBlockerCursorValue(input.cursor);
  const rows = (await hotelNightAuditBlockerRows(db, actor, input.filters, input.query, cursor ?? undefined, undefined, input.limit).all<HotelNightAuditBlockerEvidenceRow>()).results;
  const hasMore = rows.length > input.limit, items = rows.slice(0, input.limit);
  const queryHash = await sha256(JSON.stringify({ providerId: hotelNightAuditBlockersProviderId, metric: input.metric, filters: input.filters, scope: [actor.companyId, actor.assignmentId, actor.branchId], query: input.query, cursor: input.cursor ?? null }));
  await currentAuthority(db, actor);
  await audit(db, actor, hotelNightAuditBlockersProviderId, input.metric, "search", items.map(({ id, version }) => ({ id, version })), hotelNightAuditBlockersFields, queryHash);
  await currentAuthority(db, actor);
  return {
    provider: hotelNightAuditBlockersProviderId,
    metric: input.metric,
    fields: hotelNightAuditBlockersFields,
    items: items.map((row) => ({
      id: row.id,
      label: `${row.property_name} · ${row.business_date}`,
      summary: `Blocked Night Audit job · ${row.error_code}`,
      status: row.state,
      version: row.version,
      href: `/pms/night-audit?property=${encodeURIComponent(row.property_id)}`,
    })),
    hasMore,
    nextCursor: hasMore && items.length ? encodeHotelNightAuditBlockerCursor(items.at(-1)!) : null,
    queryHash,
  };
}

async function readHotelNightAuditBlockerEvidence(db: SqlDatabase, actor: WorkContext, recordId: string, filters: Filters) {
  await requireHotelNightAuditBlockerEvidence(db, actor);
  const row = await hotelNightAuditBlockerRows(db, actor, filters, "", undefined, recordId, 1).first<HotelNightAuditBlockerEvidenceRow>();
  if (!row) throw new ApiError(404, "EVIDENCE_NOT_FOUND", "This source record is unavailable in the selected role, scope or filters.");
  const record = {
    id: row.id,
    property_id: row.property_id,
    property_name: row.property_name,
    business_date: row.business_date,
    state: row.state,
    error_code: row.error_code,
    version: Number(row.version),
    href: `/pms/night-audit?property=${encodeURIComponent(row.property_id)}`,
  };
  const queryHash = await sha256(JSON.stringify({ providerId: hotelNightAuditBlockersProviderId, metric: hotelNightAuditBlockersMetric, filters, scope: [actor.companyId, actor.assignmentId, actor.branchId], recordId }));
  await currentAuthority(db, actor);
  await audit(db, actor, hotelNightAuditBlockersProviderId, hotelNightAuditBlockersMetric, "open", [{ id: record.id, version: record.version }], hotelNightAuditBlockersFields, queryHash);
  await currentAuthority(db, actor);
  return { provider: hotelNightAuditBlockersProviderId, metric: hotelNightAuditBlockersMetric, fields: hotelNightAuditBlockersFields, record, queryHash };
}

type SupplierBillEvidenceRow = {
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

async function requireProcurementEvidence(db: SqlDatabase, actor: WorkContext) {
  await currentAuthority(db, actor);
  if (!hasPermission(actor, "finance.read"))
    throw new ApiError(403, "EVIDENCE_FORBIDDEN", "This source record is not available in the current role or scope.");
  const catalog = await metricCatalogFor(db, actor);
  if (!catalog.some((item) => item.id === "invoice-match-exceptions" && item.available))
    throw new ApiError(403, "EVIDENCE_FORBIDDEN", "This source record is not available in the current role or scope.");
  await assertProductionModuleEnabled(db, actor.companyId, "procurement");
}

function supplierBillConditions(actor: WorkContext, filters: Filters, query: string, cursor?: { invoiceDate: string; id: string }, recordId?: string) {
  const conditions = [
    "latest.company_id=?",
    "(?::text IS NULL OR latest.branch_id=?)",
    "(?::text IS NULL OR latest.department_id=?)",
    "latest.invoice_date BETWEEN ? AND ?",
    "latest.currency=?",
    `((latest.status='review' AND (NOT latest.quantity_matches OR latest.subtotal_minor<>latest.expected_subtotal_minor))
      OR (latest.status='returned' AND NOT EXISTS(
        SELECT 1 FROM procurement_bill_match_receipts returned_receipt
        JOIN procurement_bill_match_receipts replacement_receipt
          ON replacement_receipt.fulfillment_id=returned_receipt.fulfillment_id AND replacement_receipt.active=TRUE
        WHERE returned_receipt.match_id=latest.id AND replacement_receipt.match_id<>latest.id
      )))`,
  ];
  const values: unknown[] = [actor.companyId, actor.branchId, actor.branchId, actor.departmentId, actor.departmentId, filters.from, filters.to, filters.currency];
  if (query) { conditions.push("position(lower(?) in lower(latest.reference))>0"); values.push(query); }
  if (cursor) { conditions.push("(latest.invoice_date,latest.id)<(?,?)"); values.push(cursor.invoiceDate, cursor.id); }
  if (recordId) { conditions.push("latest.id=?"); values.push(recordId); }
  return { conditions, values };
}

async function supplierBillEvidenceRows(db: SqlDatabase, actor: WorkContext, filters: Filters, query: string, cursor?: { invoiceDate: string; id: string }, limit = 25, recordId?: string) {
  const scoped = supplierBillConditions(actor, filters, query, cursor, recordId);
  return (await db.prepare(`SELECT latest.id,latest.reference,latest.invoice_date,latest.due_date,latest.currency,
      (latest.subtotal_minor::numeric+latest.tax_minor::numeric)::text total_minor,latest.status,latest.version,
      p.name supplier_name,o.number order_number,latest.expected_subtotal_minor::text expected_subtotal_minor,
      latest.subtotal_minor::text subtotal_minor,latest.tax_minor::text tax_minor,latest.quantity_matches
    FROM procurement_bill_matches latest
    JOIN finance_parties p ON p.id=latest.supplier_id AND p.company_id=latest.company_id
    JOIN commerce_orders o ON o.id=latest.order_id AND o.company_id=latest.company_id
    WHERE ${scoped.conditions.join(" AND ")}
    ORDER BY latest.invoice_date DESC,latest.id DESC LIMIT ?`).bind(...scoped.values, recordId ? 1 : limit + 1).all<SupplierBillEvidenceRow>()).results;
}

async function searchSupplierBillEvidence(db: SqlDatabase, actor: WorkContext, input: z.infer<typeof procurementQuerySchema>) {
  await requireProcurementEvidence(db, actor);
  const cursor = cursorValue(input.cursor), rows = await supplierBillEvidenceRows(db, actor, input.filters, input.query, cursor ?? undefined, input.limit);
  const hasMore = rows.length > input.limit, items = rows.slice(0, input.limit);
  const queryHash = await sha256(JSON.stringify({ providerId: procurementProviderId, metric: input.metric, filters: input.filters, query: input.query, cursor: input.cursor ?? null }));
  await currentAuthority(db, actor);
  await audit(db, actor, procurementProviderId, input.metric, "search", items.map(({ id, version }) => ({ id, version })), procurementSearchFields, queryHash);
  await currentAuthority(db, actor);
  return {
    provider: procurementProviderId,
    metric: input.metric,
    fields: procurementSearchFields,
    items: items.map((row) => ({ id: row.id, invoice_number: row.reference, kind: "payable", invoice_date: row.invoice_date, due_date: row.due_date, currency: row.currency, total_minor: row.total_minor, outstanding_minor: "0", status: row.status, version: row.version, party_name: row.supplier_name, order_number: row.order_number, label: row.reference, summary: `${row.supplier_name} · ${row.order_number} · ${row.status}`, amount: row.total_minor, href: `/supplier-bills/${encodeURIComponent(row.id)}` })),
    hasMore,
    nextCursor: hasMore && items.length ? encodeCursor(items.at(-1)!) : null,
    queryHash,
  };
}

async function readSupplierBillEvidence(db: SqlDatabase, actor: WorkContext, recordId: string, filters: Filters) {
  await requireProcurementEvidence(db, actor);
  const [latest] = await supplierBillEvidenceRows(db, actor, filters, "", undefined, 1, recordId);
  if (!latest) throw new ApiError(404, "EVIDENCE_NOT_FOUND", "This source record is unavailable in the selected role, scope or filters.");
  const source = await getSupplierBill(actor, recordId);
  const record = {
    id: latest.id,
    reference: latest.reference,
    supplier_name: latest.supplier_name,
    order_number: latest.order_number,
    invoice_date: latest.invoice_date,
    due_date: latest.due_date,
    currency: latest.currency,
    expected_subtotal_minor: latest.expected_subtotal_minor,
    subtotal_minor: latest.subtotal_minor,
    tax_minor: latest.tax_minor,
    status: latest.status,
    quantity_matches: latest.quantity_matches,
    version: latest.version,
    fulfillment_version: Number(source.fulfillment_version),
    receipt_count: Number(source.receipt_count),
    href: `/supplier-bills/${encodeURIComponent(recordId)}`,
  };
  const queryHash = await sha256(JSON.stringify({ providerId: procurementProviderId, metric: "invoice-match-exceptions", filters, recordId }));
  await currentAuthority(db, actor);
  await audit(db, actor, procurementProviderId, "invoice-match-exceptions", "open", [{ id: record.id, version: record.version }], procurementDetailFields, queryHash);
  await currentAuthority(db, actor);
  return { provider: procurementProviderId, metric: "invoice-match-exceptions", fields: procurementDetailFields, record, queryHash };
}

type SalesOrderEvidenceRow = {
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

type SalesEvidenceCursor = { cancelledAt: number; id: string } | { orderDate: string; id: string };
function salesCursorValue(cursor?: string, metric?: SalesMetricId): SalesEvidenceCursor | null {
  if (!cursor) return null;
  try {
    const value = JSON.parse(Buffer.from(cursor, "base64url").toString("utf8")) as { cancelledAt?: unknown; orderDate?: unknown; id?: unknown };
    const id = z.string().min(1).max(200).parse(value.id);
    if (metric === "sales-orders") {
      if (typeof value.orderDate !== "string") throw new Error();
      return { orderDate: z.iso.date().parse(value.orderDate), id };
    }
    if (typeof value.cancelledAt !== "number" || !Number.isSafeInteger(value.cancelledAt)) throw new Error();
    return { cancelledAt: value.cancelledAt, id };
  } catch {
    throw new ApiError(422, "EVIDENCE_CURSOR_INVALID", "The source record cursor is invalid. Refresh the evidence list.");
  }
}

function encodeSalesCursor(row: Pick<SalesOrderEvidenceRow, "order_date" | "updated_at" | "id">, metric: SalesMetricId) {
  return Buffer.from(JSON.stringify(metric === "sales-orders"
    ? { orderDate: row.order_date, id: row.id }
    : { cancelledAt: Number(row.updated_at), id: row.id })).toString("base64url");
}

async function requireSalesEvidence(db: SqlDatabase, actor: WorkContext, metric: SalesMetricId) {
  await currentAuthority(db, actor);
  if (!hasPermission(actor, "commerce.read"))
    throw new ApiError(403, "EVIDENCE_FORBIDDEN", "This source record is not available in the current role or scope.");
  const catalog = await metricCatalogFor(db, actor);
  if (!catalog.some((item) => item.id === metric && item.available))
    throw new ApiError(403, "EVIDENCE_FORBIDDEN", "This source record is not available in the current role or scope.");
  await assertProductionModuleEnabled(db, actor.companyId, "sales");
}

function salesOrderConditions(actor: WorkContext, metric: SalesMetricId, filters: Filters, query: string, cursor?: SalesEvidenceCursor, recordId?: string) {
  const conditions = [
    "o.company_id=?",
    "(?::text IS NULL OR o.branch_id=?)",
    "(?::text IS NULL OR o.department_id=?)",
    "o.kind='sales'",
  ];
  const values: unknown[] = [actor.companyId, actor.branchId, actor.branchId, actor.departmentId, actor.departmentId];
  if (metric === "sales-orders") {
    conditions.push("o.state IN ('confirmed','partially_fulfilled','fulfilled')", "o.order_date BETWEEN ? AND ?", "o.currency=?");
    values.push(filters.from, filters.to, filters.currency);
  } else {
    conditions.push("o.state='cancelled'", "o.updated_at>=?", "o.updated_at<?");
    values.push(Date.parse(filters.from), Date.parse(filters.to) + 86400000);
    if (metric === "sales-cancellation-value") {
      conditions.push("o.approved_by IS NOT NULL", "o.currency=?");
      values.push(filters.currency);
    }
  }
  if (query) { conditions.push("position(lower(?) in lower(o.number))>0"); values.push(query); }
  if (cursor) {
    if (metric === "sales-orders" && "orderDate" in cursor) {
      conditions.push("(o.order_date,o.id)<(?,?)"); values.push(cursor.orderDate, cursor.id);
    } else if (metric !== "sales-orders" && "cancelledAt" in cursor) {
      conditions.push("(o.updated_at,o.id)<(?,?)"); values.push(cursor.cancelledAt, cursor.id);
    } else {
      throw new ApiError(422, "EVIDENCE_CURSOR_INVALID", "The source record cursor is invalid. Refresh the evidence list.");
    }
  }
  if (recordId) { conditions.push("o.id=?"); values.push(recordId); }
  return { conditions, values };
}

async function salesOrderEvidenceRows(db: SqlDatabase, actor: WorkContext, metric: SalesMetricId, filters: Filters, query: string, cursor?: SalesEvidenceCursor, limit = 25, recordId?: string) {
  const scoped = salesOrderConditions(actor, metric, filters, query, cursor, recordId);
  return (await db.prepare(`SELECT o.id,o.number,o.order_date,o.updated_at,o.currency,o.subtotal_minor::text subtotal_minor,
      o.tax_minor::text tax_minor,(o.subtotal_minor::numeric+o.tax_minor::numeric)::text total_minor,o.state status,o.version,
      (o.approved_by IS NOT NULL) prior_approval
    FROM commerce_orders o WHERE ${scoped.conditions.join(" AND ")}
    ORDER BY ${metric === "sales-orders" ? "o.order_date DESC,o.id DESC" : "o.updated_at DESC,o.id DESC"} LIMIT ?`).bind(...scoped.values, recordId ? 1 : limit + 1).all<SalesOrderEvidenceRow>()).results;
}

async function searchSalesOrderEvidence(db: SqlDatabase, actor: WorkContext, input: z.infer<typeof salesQuerySchema>) {
  await requireSalesEvidence(db, actor, input.metric);
  const cursor = salesCursorValue(input.cursor, input.metric), rows = await salesOrderEvidenceRows(db, actor, input.metric, input.filters, input.query, cursor ?? undefined, input.limit);
  const hasMore = rows.length > input.limit, items = rows.slice(0, input.limit);
  const queryHash = await sha256(JSON.stringify({ providerId: salesProviderId, metric: input.metric, filters: input.filters, query: input.query, cursor: input.cursor ?? null }));
  await currentAuthority(db, actor);
  const fields = input.metric === "sales-orders" ? salesOrderSearchFields : salesSearchFields;
  await audit(db, actor, salesProviderId, input.metric, "search", items.map(({ id, version }) => ({ id, version })), fields, queryHash);
  await currentAuthority(db, actor);
  return {
    provider: salesProviderId,
    metric: input.metric,
    fields,
    items: items.map((row) => ({
      id: row.id,
      ...(input.metric === "sales-orders" || input.metric === "sales-cancellation-value" ? { currency: row.currency, total_minor: row.total_minor, amount: row.total_minor } : {}),
      status: row.status, version: row.version,
      label: row.number,
      summary: input.metric === "sales-orders"
        ? `Sales order · ${row.order_date} · ${row.status}`
        : `Cancelled · ${new Date(Number(row.updated_at)).toISOString()} UTC${row.prior_approval ? " · Previously approved" : " · Draft"}`,
      href: `/commerce/orders/${encodeURIComponent(row.id)}`,
    })),
    hasMore,
    nextCursor: hasMore && items.length ? encodeSalesCursor(items.at(-1)!, input.metric) : null,
    queryHash,
  };
}

async function readSalesOrderEvidence(db: SqlDatabase, actor: WorkContext, metric: SalesMetricId, recordId: string, filters: Filters) {
  await requireSalesEvidence(db, actor, metric);
  const [row] = await salesOrderEvidenceRows(db, actor, metric, filters, "", undefined, 1, recordId);
  if (!row) throw new ApiError(404, "EVIDENCE_NOT_FOUND", "This source record is unavailable in the selected role, scope or filters.");
  const record = {
    id: row.id, number: row.number, order_date: row.order_date, ...(metric === "sales-orders" ? {} : { cancelled_at: Number(row.updated_at) }), currency: row.currency,
    subtotal_minor: row.subtotal_minor, tax_minor: row.tax_minor, total_minor: row.total_minor,
    status: row.status, version: row.version, ...(metric === "sales-orders" ? {} : { prior_approval: row.prior_approval }),
    href: `/commerce/orders/${encodeURIComponent(row.id)}`,
  };
  const queryHash = await sha256(JSON.stringify({ providerId: salesProviderId, metric, filters, recordId }));
  await currentAuthority(db, actor);
  const fields = metric === "sales-orders" ? salesOrderDetailFields : salesDetailFields;
  await audit(db, actor, salesProviderId, metric, "open", [{ id: row.id, version: row.version }], fields, queryHash);
  await currentAuthority(db, actor);
  return { provider: salesProviderId, metric, fields, record, queryHash };
}

type SalesFulfillmentEvidenceRow = { id: string; number: string; order_date: string; status: string; version: number };
const salesFulfillmentFields = ["number", "order_date", "status", "version"];

function salesFulfillmentCursorValue(cursor?: string) {
  if (!cursor) return null;
  try {
    const value = JSON.parse(Buffer.from(cursor, "base64url").toString("utf8")) as { orderDate?: unknown; id?: unknown };
    if (typeof value.orderDate !== "string" || typeof value.id !== "string") throw new Error();
    return { orderDate: z.string().date().parse(value.orderDate), id: z.string().min(1).max(200).parse(value.id) };
  } catch {
    throw new ApiError(422, "EVIDENCE_CURSOR_INVALID", "The source record cursor is invalid. Refresh the evidence list.");
  }
}

function encodeSalesFulfillmentCursor(row: Pick<SalesFulfillmentEvidenceRow, "order_date" | "id">) {
  return Buffer.from(JSON.stringify({ orderDate: row.order_date, id: row.id })).toString("base64url");
}

async function requireSalesFulfillmentEvidence(db: SqlDatabase, actor: WorkContext) {
  await currentAuthority(db, actor);
  if (!hasPermission(actor, "commerce.read"))
    throw new ApiError(403, "EVIDENCE_FORBIDDEN", "This source record is not available in the current role or scope.");
  const catalog = await metricCatalogFor(db, actor);
  if (!catalog.some((item) => item.id === salesFulfillmentMetric && item.available))
    throw new ApiError(403, "EVIDENCE_FORBIDDEN", "This source record is not available in the current role or scope.");
  await assertProductionModuleEnabled(db, actor.companyId, "sales");
}

function salesFulfillmentConditions(actor: WorkContext, filters: Filters, query: string, cursor?: { orderDate: string; id: string }, recordId?: string) {
  const conditions = [
    "o.company_id=?",
    "(?::text IS NULL OR o.branch_id=?)",
    "(?::text IS NULL OR o.department_id=?)",
    "o.kind='sales'",
    "o.state IN ('confirmed','partially_fulfilled','fulfilled')",
    "o.order_date BETWEEN ? AND ?",
    "EXISTS(SELECT 1 FROM commerce_order_lines l WHERE l.order_id=o.id)",
  ];
  const values: unknown[] = [actor.companyId, actor.branchId, actor.branchId, actor.departmentId, actor.departmentId, filters.from, filters.to];
  if (query) { conditions.push("position(lower(?) in lower(o.number))>0"); values.push(query); }
  if (cursor) { conditions.push("(o.order_date,o.id)<(?,?)"); values.push(cursor.orderDate, cursor.id); }
  if (recordId) { conditions.push("o.id=?"); values.push(recordId); }
  return { conditions, values };
}

async function salesFulfillmentEvidenceRows(db: SqlDatabase, actor: WorkContext, filters: Filters, query: string, cursor?: { orderDate: string; id: string }, limit = 25, recordId?: string) {
  const scoped = salesFulfillmentConditions(actor, filters, query, cursor, recordId);
  return (await db.prepare(`SELECT o.id,o.number,o.order_date,o.state status,o.version
    FROM commerce_orders o WHERE ${scoped.conditions.join(" AND ")}
    ORDER BY o.order_date DESC,o.id DESC LIMIT ?`).bind(...scoped.values, recordId ? 1 : limit + 1).all<SalesFulfillmentEvidenceRow>()).results;
}

async function searchSalesFulfillmentEvidence(db: SqlDatabase, actor: WorkContext, input: z.infer<typeof salesFulfillmentQuerySchema>) {
  await requireSalesFulfillmentEvidence(db, actor);
  const cursor = salesFulfillmentCursorValue(input.cursor);
  const rows = await salesFulfillmentEvidenceRows(db, actor, input.filters, input.query, cursor ?? undefined, input.limit);
  const hasMore = rows.length > input.limit, items = rows.slice(0, input.limit);
  const queryHash = await sha256(JSON.stringify({ providerId: salesProviderId, metric: input.metric, filters: input.filters, query: input.query, cursor: input.cursor ?? null }));
  await currentAuthority(db, actor);
  await audit(db, actor, salesProviderId, input.metric, "search", items.map(({ id, version }) => ({ id, version })), salesFulfillmentFields, queryHash);
  await currentAuthority(db, actor);
  return {
    provider: salesProviderId,
    metric: input.metric,
    fields: salesFulfillmentFields,
    items: items.map((row) => ({ id: row.id, label: row.number, summary: `${row.status} · ${row.order_date}`, status: row.status, version: row.version, href: `/commerce/orders/${encodeURIComponent(row.id)}` })),
    hasMore,
    nextCursor: hasMore && items.length ? encodeSalesFulfillmentCursor(items.at(-1)!) : null,
    queryHash,
  };
}

async function readSalesFulfillmentEvidence(db: SqlDatabase, actor: WorkContext, recordId: string, filters: Filters) {
  await requireSalesFulfillmentEvidence(db, actor);
  const [row] = await salesFulfillmentEvidenceRows(db, actor, filters, "", undefined, 1, recordId);
  if (!row) throw new ApiError(404, "EVIDENCE_NOT_FOUND", "This source record is unavailable in the selected role, scope or filters.");
  const record = { ...row, href: `/commerce/orders/${encodeURIComponent(row.id)}` };
  const queryHash = await sha256(JSON.stringify({ providerId: salesProviderId, metric: salesFulfillmentMetric, filters, recordId }));
  await currentAuthority(db, actor);
  await audit(db, actor, salesProviderId, salesFulfillmentMetric, "open", [{ id: row.id, version: row.version }], salesFulfillmentFields, queryHash);
  await currentAuthority(db, actor);
  return { provider: salesProviderId, metric: salesFulfillmentMetric, fields: salesFulfillmentFields, record, queryHash };
}

type FieldVisitEvidenceRow = { id: string; scheduled_at: number | string; visit_completed_at: number | string | null; status: string; version: number };
const fieldVisitFields = ["scheduled_at", "visit_completed_at", "status", "version"];

function fieldVisitCursorValue(cursor?: string) {
  if (!cursor) return null;
  try {
    const value = JSON.parse(Buffer.from(cursor, "base64url").toString("utf8")) as { scheduledAt?: unknown; id?: unknown };
    if (typeof value.scheduledAt !== "number" || !Number.isSafeInteger(value.scheduledAt) || typeof value.id !== "string") throw new Error();
    return { scheduledAt: value.scheduledAt, id: z.string().min(1).max(200).parse(value.id) };
  } catch {
    throw new ApiError(422, "EVIDENCE_CURSOR_INVALID", "The source record cursor is invalid. Refresh the evidence list.");
  }
}

function encodeFieldVisitCursor(row: Pick<FieldVisitEvidenceRow, "scheduled_at" | "id">) {
  return Buffer.from(JSON.stringify({ scheduledAt: Number(row.scheduled_at), id: row.id })).toString("base64url");
}

async function requireFieldVisitEvidence(db: SqlDatabase, actor: WorkContext) {
  await currentAuthority(db, actor);
  if (!hasPermission(actor, "records.read"))
    throw new ApiError(403, "EVIDENCE_FORBIDDEN", "This source record is not available in the current role or scope.");
  const catalog = await metricCatalogFor(db, actor);
  if (!catalog.some((item) => item.id === fieldVisitMetric && item.available))
    throw new ApiError(403, "EVIDENCE_FORBIDDEN", "This source record is not available in the current role or scope.");
  await assertProductionModuleEnabled(db, actor.companyId, "field_service");
}

function fieldVisitConditions(actor: WorkContext, filters: Filters, query: string, cursor?: { scheduledAt: number; id: string }, recordId?: string) {
  const conditions = [
    "o.company_id=?",
    "(?::text IS NULL OR o.branch_id=?)",
    "(?::text IS NULL OR o.department_id=?)",
    "o.scheduled_at IS NOT NULL",
    "o.status IN ('scheduled','dispatched','in_visit','exception','completed','billed')",
    "o.scheduled_at>=?",
    "o.scheduled_at<?",
  ];
  const values: unknown[] = [actor.companyId, actor.branchId, actor.branchId, actor.departmentId, actor.departmentId, Date.parse(filters.from), Date.parse(filters.to) + 86400000];
  if (query) { conditions.push("(position(lower(?) in lower(o.id))>0 OR position(lower(?) in lower(o.status))>0)"); values.push(query, query); }
  if (cursor) { conditions.push("(o.scheduled_at,o.id)<(?,?)"); values.push(cursor.scheduledAt, cursor.id); }
  if (recordId) { conditions.push("o.id=?"); values.push(recordId); }
  return { conditions, values };
}

async function fieldVisitEvidenceRows(db: SqlDatabase, actor: WorkContext, filters: Filters, query: string, cursor?: { scheduledAt: number; id: string }, limit = 25, recordId?: string) {
  const scoped = fieldVisitConditions(actor, filters, query, cursor, recordId);
  return (await db.prepare(`SELECT o.id,o.scheduled_at,o.visit_completed_at,o.status,o.version
    FROM field_service_orders o WHERE ${scoped.conditions.join(" AND ")}
    ORDER BY o.scheduled_at DESC,o.id DESC LIMIT ?`).bind(...scoped.values, recordId ? 1 : limit + 1).all<FieldVisitEvidenceRow>()).results;
}

async function searchFieldVisitEvidence(db: SqlDatabase, actor: WorkContext, input: z.infer<typeof fieldVisitQuerySchema>) {
  await requireFieldVisitEvidence(db, actor);
  const cursor = fieldVisitCursorValue(input.cursor);
  const rows = await fieldVisitEvidenceRows(db, actor, input.filters, input.query, cursor ?? undefined, input.limit);
  const hasMore = rows.length > input.limit, items = rows.slice(0, input.limit);
  const queryHash = await sha256(JSON.stringify({ providerId: "field-service-orders", metric: input.metric, filters: input.filters, query: input.query, cursor: input.cursor ?? null }));
  await currentAuthority(db, actor);
  await audit(db, actor, "field-service-orders", input.metric, "search", items.map(({ id, version }) => ({ id, version })), fieldVisitFields, queryHash);
  await currentAuthority(db, actor);
  return {
    provider: "field-service-orders",
    metric: input.metric,
    fields: fieldVisitFields,
    items: items.map((row) => ({ id: row.id, label: row.id, summary: `${row.status} · ${new Date(Number(row.scheduled_at)).toISOString()} UTC`, status: row.status, version: row.version, href: `/field-service/${encodeURIComponent(row.id)}` })),
    hasMore,
    nextCursor: hasMore && items.length ? encodeFieldVisitCursor(items.at(-1)!) : null,
    queryHash,
  };
}

async function readFieldVisitEvidence(db: SqlDatabase, actor: WorkContext, recordId: string, filters: Filters) {
  await requireFieldVisitEvidence(db, actor);
  let order: Record<string, unknown>;
  try {
    ({ order } = await fieldServiceDetail(actor, recordId));
  } catch (error) {
    if (error instanceof ApiError && error.status === 404) throw new ApiError(404, "EVIDENCE_NOT_FOUND", "This source record is unavailable in the selected role, scope or filters.");
    throw error;
  }
  const scheduledAt = Number(order.scheduled_at), status = String(order.status);
  const eligible = Number.isSafeInteger(scheduledAt) && scheduledAt >= Date.parse(filters.from) && scheduledAt < Date.parse(filters.to) + 86400000
    && ["scheduled", "dispatched", "in_visit", "exception", "completed", "billed"].includes(status);
  if (!eligible) throw new ApiError(404, "EVIDENCE_NOT_FOUND", "This source record is unavailable in the selected role, scope or filters.");
  const completedAt = order.visit_completed_at === null || order.visit_completed_at === undefined ? null : Number(order.visit_completed_at);
  const record = { id: recordId, scheduled_at: scheduledAt, visit_completed_at: completedAt, status, version: Number(order.version), href: `/field-service/${encodeURIComponent(recordId)}` };
  const queryHash = await sha256(JSON.stringify({ providerId: "field-service-orders", metric: fieldVisitMetric, filters, recordId }));
  await currentAuthority(db, actor);
  await audit(db, actor, "field-service-orders", fieldVisitMetric, "open", [{ id: recordId, version: record.version }], fieldVisitFields, queryHash);
  await currentAuthority(db, actor);
  return { provider: "field-service-orders", metric: fieldVisitMetric, fields: fieldVisitFields, record, queryHash };
}

type QualityInspectionRow = {
  id: string;
  hold_id: string;
  result: "pass" | "fail";
  created_at: number | string;
  reviewed_version: number | string;
  product_name: string;
  warehouse_name: string;
};

const qualitySearchFields = ["result", "created_at", "reviewed_version", "product_name", "warehouse_name"];
const qualityDetailFields = [...qualitySearchFields, "evidence", "inspected_by", "hold_id"];

function qualityCursorValue(cursor?: string) {
  if (!cursor) return null;
  try {
    const value = JSON.parse(Buffer.from(cursor, "base64url").toString("utf8")) as { createdAt?: unknown; id?: unknown };
    if (typeof value.createdAt !== "number" || !Number.isSafeInteger(value.createdAt) || typeof value.id !== "string") throw new Error();
    return { createdAt: value.createdAt, id: z.string().min(1).max(200).parse(value.id) };
  } catch {
    throw new ApiError(422, "EVIDENCE_CURSOR_INVALID", "The source record cursor is invalid. Refresh the evidence list.");
  }
}

function encodeQualityCursor(row: Pick<QualityInspectionRow, "created_at" | "id">) {
  return Buffer.from(JSON.stringify({ createdAt: Number(row.created_at), id: row.id })).toString("base64url");
}

async function requireQualityEvidence(db: SqlDatabase, actor: WorkContext, metric: typeof qualityMetrics[number]) {
  await currentAuthority(db, actor);
  if (!hasPermission(actor, "quality.read") || actor.departmentId)
    throw new ApiError(403, "EVIDENCE_FORBIDDEN", "This source record is not available in the current role or scope.");
  const catalog = await metricCatalogFor(db, actor);
  if (!catalog.some((item) => item.id === metric && item.available))
    throw new ApiError(403, "EVIDENCE_FORBIDDEN", "This source record is not available in the current role or scope.");
  await assertProductionModuleEnabled(db, actor.companyId, "quality");
}

async function searchQualityInspectionEvidence(db: SqlDatabase, actor: WorkContext, raw: unknown) {
  const input = qualityQuerySchema.parse(raw);
  await requireQualityEvidence(db, actor, input.metric);
  if (input.metric !== "inspection-pass-rate") return searchQualityHoldEvidence(db, actor, input);
  const cursor = qualityCursorValue(input.cursor);
  const conditions = [
    "h.company_id=?",
    "(?::text IS NULL OR h.branch_id=?)",
    "i.created_at>=?",
    "i.created_at<?",
  ];
  const values: unknown[] = [actor.companyId, actor.branchId, actor.branchId, Date.parse(input.filters.from), Date.parse(input.filters.to) + 86400000];
  if (input.query) {
    conditions.push("(position(lower(?) in lower(p.name))>0 OR position(lower(?) in lower(i.result))>0)");
    values.push(input.query, input.query);
  }
  if (cursor) {
    conditions.push("(i.created_at,i.id)<(?,?)");
    values.push(cursor.createdAt, cursor.id);
  }
  const rows = (await db.prepare(`SELECT i.id,i.hold_id,i.result,i.created_at,i.reviewed_version,p.name product_name,w.name warehouse_name
    FROM inventory_quality_inspections i
    JOIN inventory_quality_holds h ON h.id=i.hold_id
    JOIN commerce_products p ON p.id=h.product_id AND p.company_id=h.company_id
    JOIN commerce_warehouses w ON w.id=h.warehouse_id AND w.company_id=h.company_id
    WHERE ${conditions.join(" AND ")}
    ORDER BY i.created_at DESC,i.id DESC LIMIT ?`).bind(...values, input.limit + 1).all<QualityInspectionRow>()).results;
  const hasMore = rows.length > input.limit, items = rows.slice(0, input.limit);
  const queryHash = await sha256(JSON.stringify({ providerId: qualityProviderId, metric: input.metric, filters: input.filters, query: input.query, cursor: input.cursor ?? null }));
  await currentAuthority(db, actor);
  await audit(db, actor, qualityProviderId, input.metric, "search", items.map((row) => ({ id: row.id, version: Number(row.reviewed_version) })), qualitySearchFields, queryHash);
  await currentAuthority(db, actor);
  return {
    provider: qualityProviderId,
    metric: input.metric,
    fields: qualitySearchFields,
    items: items.map((row) => ({
      id: row.id,
      label: row.product_name,
      summary: `${row.result === "pass" ? "Passed" : "Failed"} · ${row.warehouse_name} · ${new Date(Number(row.created_at)).toISOString()} UTC`,
      status: row.result,
      version: Number(row.reviewed_version),
      href: `/quality/${encodeURIComponent(row.hold_id)}`,
    })),
    hasMore,
    nextCursor: hasMore && items.length ? encodeQualityCursor(items.at(-1)!) : null,
    queryHash,
  };
}

async function searchQualityHoldEvidence(db: SqlDatabase, actor: WorkContext, input: z.infer<typeof qualityQuerySchema>) {
  const releases = input.metric === "quality-releases";
  const cursor = qualityCursorValue(input.cursor);
  const timeColumn = releases ? "h.released_at" : "h.updated_at";
  const conditions = ["h.company_id=?", "(?::text IS NULL OR h.branch_id=?)", releases ? "h.status='released'" : "h.status NOT IN ('released','cancelled','disposed')"];
  const values: unknown[] = [actor.companyId, actor.branchId, actor.branchId];
  if (releases) {
    conditions.push("h.released_at>=?", "h.released_at<?");
    values.push(Date.parse(input.filters.from), Date.parse(input.filters.to) + 86400000);
  }
  if (input.query) {
    conditions.push("(position(lower(?) in lower(p.name))>0 OR position(lower(?) in lower(h.status))>0)");
    values.push(input.query, input.query);
  }
  if (cursor) {
    conditions.push(`(${timeColumn},h.id)<(?,?)`);
    values.push(cursor.createdAt, cursor.id);
  }
  const rows = (await db.prepare(`SELECT h.id,h.id hold_id,${timeColumn} created_at,h.version reviewed_version,h.status result,
      p.name product_name,w.name warehouse_name
    FROM inventory_quality_holds h
    JOIN commerce_products p ON p.id=h.product_id AND p.company_id=h.company_id
    JOIN commerce_warehouses w ON w.id=h.warehouse_id AND w.company_id=h.company_id
    WHERE ${conditions.join(" AND ")}
    ORDER BY ${timeColumn} DESC,h.id DESC LIMIT ?`).bind(...values, input.limit + 1).all<QualityInspectionRow>()).results;
  const hasMore = rows.length > input.limit, items = rows.slice(0, input.limit);
  const searchFields = releases
    ? ["status", "released_at", "version", "product_name", "warehouse_name"]
    : ["status", "updated_at", "version", "product_name", "warehouse_name"];
  const queryHash = await sha256(JSON.stringify({ providerId: qualityProviderId, metric: input.metric, filters: input.filters, query: input.query, cursor: input.cursor ?? null }));
  await currentAuthority(db, actor);
  await audit(db, actor, qualityProviderId, input.metric, "search", items.map((row) => ({ id: row.id, version: Number(row.reviewed_version) })), searchFields, queryHash);
  await currentAuthority(db, actor);
  return {
    provider: qualityProviderId,
    metric: input.metric,
    fields: searchFields,
    items: items.map((row) => ({
      id: row.id,
      label: row.product_name,
      summary: `${releases ? "Released" : "Open hold"} · ${row.warehouse_name} · ${row.result}`,
      status: row.result,
      version: Number(row.reviewed_version),
      href: `/quality/${encodeURIComponent(row.hold_id)}`,
    })),
    hasMore,
    nextCursor: hasMore && items.length ? encodeQualityCursor(items.at(-1)!) : null,
    queryHash,
  };
}

async function readQualityInspectionEvidence(db: SqlDatabase, actor: WorkContext, recordId: string, filters: Filters) {
  await requireQualityEvidence(db, actor, "inspection-pass-rate");
  const parent = await db.prepare(`SELECT i.hold_id FROM inventory_quality_inspections i
    JOIN inventory_quality_holds h ON h.id=i.hold_id
    WHERE i.id=? AND h.company_id=? AND (?::text IS NULL OR h.branch_id=?)
      AND i.created_at>=? AND i.created_at<?`)
    .bind(recordId, actor.companyId, actor.branchId, actor.branchId, Date.parse(filters.from), Date.parse(filters.to) + 86400000)
    .first<{ hold_id: string }>();
  if (!parent) throw new ApiError(404, "EVIDENCE_NOT_FOUND", "This source record is unavailable in the selected role, scope or filters.");

  // Reuse the Quality module's detail access check before reading evidence text from the immutable event.
  await getQualityHold(actor, parent.hold_id);
  const row = await db.prepare(`SELECT i.id,i.hold_id,i.result,i.evidence,i.inspected_by,i.created_at,i.reviewed_version,
      p.name product_name,w.name warehouse_name
    FROM inventory_quality_inspections i
    JOIN inventory_quality_holds h ON h.id=i.hold_id
    JOIN commerce_products p ON p.id=h.product_id AND p.company_id=h.company_id
    JOIN commerce_warehouses w ON w.id=h.warehouse_id AND w.company_id=h.company_id
    WHERE i.id=? AND i.hold_id=? AND h.company_id=? AND (?::text IS NULL OR h.branch_id=?)
      AND i.created_at>=? AND i.created_at<?`)
    .bind(recordId, parent.hold_id, actor.companyId, actor.branchId, actor.branchId, Date.parse(filters.from), Date.parse(filters.to) + 86400000)
    .first<QualityInspectionRow & { evidence: string; inspected_by: string }>();
  if (!row) throw new ApiError(404, "EVIDENCE_NOT_FOUND", "This source record is unavailable in the selected role, scope or filters.");
  const reviewedVersion = Number(row.reviewed_version);
  const record = {
    id: row.id,
    hold_id: row.hold_id,
    result: row.result,
    evidence: row.evidence,
    inspected_by: row.inspected_by,
    inspected_at: Number(row.created_at),
    reviewed_version: reviewedVersion,
    product_name: row.product_name,
    warehouse_name: row.warehouse_name,
    href: `/quality/${encodeURIComponent(row.hold_id)}`,
  };
  const queryHash = await sha256(JSON.stringify({ providerId: qualityProviderId, metric: "inspection-pass-rate", filters, recordId }));
  await currentAuthority(db, actor);
  await audit(db, actor, qualityProviderId, "inspection-pass-rate", "open", [{ id: row.id, version: reviewedVersion }], qualityDetailFields, queryHash);
  await currentAuthority(db, actor);
  return { provider: qualityProviderId, metric: "inspection-pass-rate", fields: qualityDetailFields, record, queryHash };
}

async function readQualityHoldEvidence(db: SqlDatabase, actor: WorkContext, metric: "quality-holds" | "quality-releases", recordId: string, filters: Filters) {
  await requireQualityEvidence(db, actor, metric);
  const hold = await getQualityHold(actor, recordId);
  const releasedAt = hold.released_at === null || hold.released_at === undefined ? null : Number(hold.released_at);
  const inSelectedPeriod = releasedAt !== null && releasedAt >= Date.parse(filters.from) && releasedAt < Date.parse(filters.to) + 86400000;
  const isReleased = hold.status === "released";
  if (metric === "quality-holds" ? ["released", "cancelled", "disposed"].includes(String(hold.status)) : !isReleased || !inSelectedPeriod)
    throw new ApiError(404, "EVIDENCE_NOT_FOUND", "This source record is unavailable in the selected role, scope or filters.");
  const fields = ["product_name", "warehouse_name", "reason", "status", "due_date", "created_at", "updated_at", "version", "released_by", "released_at", "release_reason"];
  const record = {
    id: String(hold.id),
    product_name: String(hold.product_name),
    warehouse_name: String(hold.warehouse_name),
    reason: String(hold.reason),
    status: String(hold.status),
    due_date: String(hold.due_date),
    created_at: Number(hold.created_at),
    updated_at: Number(hold.updated_at),
    version: Number(hold.version),
    released_by: hold.released_by === null ? null : String(hold.released_by),
    released_at: releasedAt,
    release_reason: hold.release_reason === null ? null : String(hold.release_reason),
    href: `/quality/${encodeURIComponent(String(hold.id))}`,
  };
  const queryHash = await sha256(JSON.stringify({ providerId: qualityProviderId, metric, filters, recordId }));
  await currentAuthority(db, actor);
  await audit(db, actor, qualityProviderId, metric, "open", [{ id: record.id, version: record.version }], fields, queryHash);
  await currentAuthority(db, actor);
  return { provider: qualityProviderId, metric, fields, record, queryHash };
}

type ManufacturingCostEvidenceRow = {
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

const manufacturingCostProviderId = "manufacturing-cost-movements";
const manufacturingCostSearchFields = ["order_id", "movement_kind", "amount_minor", "posted_at", "journal_status", "currency", "version"];
const manufacturingCostDetailFields = [...manufacturingCostSearchFields, "order_status", "product_name", "warehouse_name", "order_version"];

function manufacturingCostCursorValue(cursor?: string) {
  if (!cursor) return null;
  try {
    const value = JSON.parse(Buffer.from(cursor, "base64url").toString("utf8")) as { postedAt?: unknown; id?: unknown };
    if (typeof value.postedAt !== "number" || !Number.isSafeInteger(value.postedAt) || typeof value.id !== "string") throw new Error();
    return { postedAt: value.postedAt, id: z.string().min(1).max(200).parse(value.id) };
  } catch {
    throw new ApiError(422, "EVIDENCE_CURSOR_INVALID", "The source record cursor is invalid. Refresh the evidence list.");
  }
}

function encodeManufacturingCostCursor(row: Pick<ManufacturingCostEvidenceRow, "posted_at" | "id">) {
  return Buffer.from(JSON.stringify({ postedAt: Number(row.posted_at), id: row.id })).toString("base64url");
}

async function requireManufacturingCostEvidence(db: SqlDatabase, actor: WorkContext) {
  await currentAuthority(db, actor);
  if (!hasPermission(actor, "records.read"))
    throw new ApiError(403, "EVIDENCE_FORBIDDEN", "This source record is not available in the current role or scope.");
  const catalog = await metricCatalogFor(db, actor);
  if (!catalog.some((item) => item.id === manufacturingCostMetric && item.available))
    throw new ApiError(403, "EVIDENCE_FORBIDDEN", "This source record is not available in the current role or scope.");
  await assertProductionModuleEnabled(db, actor.companyId, "manufacturing");
}

function manufacturingCostConditions(actor: WorkContext, filters: Filters, query: string, cursor?: { postedAt: number; id: string }, recordId?: string) {
  const conditions = [
    "m.company_id=?",
    "(?::text IS NULL OR o.branch_id=?)",
    "o.status='completed'",
    "m.movement_kind IN ('material_issue','material_return','labor')",
    "j.status='posted'",
    "j.currency=?",
    "j.posted_at>=?",
    "j.posted_at<?",
    "cs.currency=j.currency",
  ];
  const values: unknown[] = [actor.companyId, actor.branchId, actor.branchId, filters.currency, Date.parse(filters.from), Date.parse(filters.to) + 86400000];
  if (query) {
    conditions.push("(position(lower(?) in lower(m.id))>0 OR position(lower(?) in lower(m.order_id))>0 OR position(lower(?) in lower(m.movement_kind))>0 OR position(lower(?) in lower(p.name))>0)");
    values.push(query, query, query, query);
  }
  if (cursor) { conditions.push("(j.posted_at,m.id)<(?,?)"); values.push(cursor.postedAt, cursor.id); }
  if (recordId) { conditions.push("m.id=?"); values.push(recordId); }
  return { conditions, values };
}

async function manufacturingCostEvidenceRows(db: SqlDatabase, actor: WorkContext, filters: Filters, query: string, cursor?: { postedAt: number; id: string }, limit = 25, recordId?: string) {
  const scoped = manufacturingCostConditions(actor, filters, query, cursor, recordId);
  return (await db.prepare(`SELECT m.id,m.order_id,m.movement_kind,m.amount_minor::text amount_minor,j.posted_at,m.version,m.journal_id,
      j.status journal_status,j.currency,o.status order_status,o.version order_version,p.name product_name,w.name warehouse_name
    FROM manufacturing_cost_movements m
    JOIN manufacturing_orders o ON o.id=m.order_id AND o.company_id=m.company_id
    JOIN manufacturing_specifications s ON s.id=o.specification_id
    JOIN commerce_products p ON p.id=s.output_product_id AND p.company_id=o.company_id
    JOIN commerce_warehouses w ON w.id=o.warehouse_id AND w.company_id=o.company_id
    JOIN finance_journals j ON j.id=m.journal_id AND j.company_id=m.company_id
    JOIN commerce_settings cs ON cs.company_id=o.company_id
    WHERE ${scoped.conditions.join(" AND ")}
    ORDER BY j.posted_at DESC,m.id DESC LIMIT ?`).bind(...scoped.values, recordId ? 1 : limit + 1).all<ManufacturingCostEvidenceRow>()).results;
}

async function searchManufacturingCostEvidence(db: SqlDatabase, actor: WorkContext, input: z.infer<typeof manufacturingCostQuerySchema>) {
  await requireManufacturingCostEvidence(db, actor);
  const cursor = manufacturingCostCursorValue(input.cursor);
  const rows = await manufacturingCostEvidenceRows(db, actor, input.filters, input.query, cursor ?? undefined, input.limit);
  const hasMore = rows.length > input.limit, items = rows.slice(0, input.limit);
  const queryHash = await sha256(JSON.stringify({ providerId: manufacturingCostProviderId, metric: input.metric, filters: input.filters, query: input.query, cursor: input.cursor ?? null }));
  await currentAuthority(db, actor);
  await audit(db, actor, manufacturingCostProviderId, input.metric, "search", items.map(({ id, version }) => ({ id, version })), manufacturingCostSearchFields, queryHash);
  await currentAuthority(db, actor);
  return {
    provider: manufacturingCostProviderId,
    metric: input.metric,
    fields: manufacturingCostSearchFields,
    items: items.map((row) => ({ id: row.id, label: row.order_id, summary: `${row.movement_kind.replaceAll("_", " ")} · ${row.product_name} · ${new Date(Number(row.posted_at)).toISOString().slice(0, 10)}`, amount: row.amount_minor, currency: row.currency, status: row.journal_status, version: row.version, href: `/manufacturing/${encodeURIComponent(row.order_id)}` })),
    hasMore,
    nextCursor: hasMore && items.length ? encodeManufacturingCostCursor(items.at(-1)!) : null,
    queryHash,
  };
}

async function readManufacturingCostEvidence(db: SqlDatabase, actor: WorkContext, recordId: string, filters: Filters) {
  await requireManufacturingCostEvidence(db, actor);
  const [row] = await manufacturingCostEvidenceRows(db, actor, filters, "", undefined, 1, recordId);
  if (!row) throw new ApiError(404, "EVIDENCE_NOT_FOUND", "This source record is unavailable in the selected role, scope or filters.");
  // Use the module detail reader as the final record-level authority check. Only the
  // cost movement projection below is returned to Executive; other order detail stays in its module.
  await productionDetail(actor, row.order_id);
  const record = {
    id: row.id, order_id: row.order_id, movement_kind: row.movement_kind, amount_minor: row.amount_minor,
    posted_at: Number(row.posted_at), journal_status: row.journal_status, currency: row.currency,
    order_status: row.order_status, order_version: Number(row.order_version), product_name: row.product_name,
    warehouse_name: row.warehouse_name, version: Number(row.version), href: `/manufacturing/${encodeURIComponent(row.order_id)}`,
  };
  const queryHash = await sha256(JSON.stringify({ providerId: manufacturingCostProviderId, metric: manufacturingCostMetric, filters, recordId }));
  await currentAuthority(db, actor);
  await audit(db, actor, manufacturingCostProviderId, manufacturingCostMetric, "open", [{ id: row.id, version: record.version }], manufacturingCostDetailFields, queryHash);
  await currentAuthority(db, actor);
  return { provider: manufacturingCostProviderId, metric: manufacturingCostMetric, fields: manufacturingCostDetailFields, record, queryHash };
}

const customerActivityProviderId = "customer-360-timeline";
const customerActivityFields = ["source_id", "source_type", "title", "status", "version", "at", "href", "customer_id", "customer_name"];

function customerActivityCursor(raw?: string) {
  if (!raw) return null;
  try {
    const value = JSON.parse(Buffer.from(raw, "base64url").toString("utf8")) as { at?: unknown; id?: unknown };
    if (typeof value.at !== "number" || !Number.isSafeInteger(value.at) || typeof value.id !== "string") throw new Error();
    return { at: value.at, id: z.string().min(1).max(250).parse(value.id) };
  } catch {
    throw new ApiError(422, "EVIDENCE_CURSOR_INVALID", "The source record cursor is invalid. Refresh the evidence list.");
  }
}

function encodeCustomerActivityCursor(row: Pick<CustomerActivityItem, "at" | "id">) {
  return Buffer.from(JSON.stringify({ at: Number(row.at), id: row.id })).toString("base64url");
}

async function requireCustomerActivityEvidence(db: SqlDatabase, actor: WorkContext) {
  await currentAuthority(db, actor);
  if (!hasPermission(actor, "commerce.read"))
    throw new ApiError(403, "EVIDENCE_FORBIDDEN", "This source record is not available in the current role or scope.");
  const catalog = await metricCatalogFor(db, actor);
  if (!catalog.some((item) => item.id === customerActivityMetric && item.available))
    throw new ApiError(403, "EVIDENCE_FORBIDDEN", "This source record is not available in the current role or scope.");
}

async function searchCustomerActivityEvidence(db: SqlDatabase, actor: WorkContext, input: z.infer<typeof customerActivityQuerySchema>) {
  await requireCustomerActivityEvidence(db, actor);
  const source = customerTimelineEvents(actor), cursor = customerActivityCursor(input.cursor);
  const conditions = ["e.at>=?", "e.at<?"];
  const values: unknown[] = [...source.values, Date.parse(input.filters.from), Date.parse(input.filters.to) + 86400000];
  if (input.query) {
    conditions.push("position(lower(?) in lower(concat_ws(' ',e.id,e.source_id,e.source_type,e.title,e.status,e.customer_name)))>0");
    values.push(input.query);
  }
  if (cursor) { conditions.push("(e.at,e.id)<(?,?)"); values.push(cursor.at, cursor.id); }
  const rows = (await db.prepare(`${source.sql} WHERE ${conditions.join(" AND ")} ORDER BY e.at DESC,e.id DESC LIMIT ?`)
    .bind(...values, input.limit + 1).all<CustomerActivityItem>()).results;
  const hasMore = rows.length > input.limit, items = rows.slice(0, input.limit);
  const queryHash = await sha256(JSON.stringify({ providerId: customerActivityProviderId, metric: input.metric, filters: input.filters, query: input.query, cursor: input.cursor ?? null }));
  await currentAuthority(db, actor);
  await audit(db, actor, customerActivityProviderId, input.metric, "search", items.map(({ id, version }) => ({ id, version: Number(version) })), customerActivityFields, queryHash);
  await currentAuthority(db, actor);
  return {
    provider: customerActivityProviderId,
    metric: input.metric,
    fields: customerActivityFields,
    items: items.map((row) => ({
      id: row.id,
      label: row.customer_name,
      summary: `${row.source_type.replaceAll("_", " ")} · ${row.title} · ${row.status} · ${new Date(Number(row.at)).toISOString().slice(0, 10)}`,
      status: row.status,
      version: Number(row.version),
      href: `/customers/${encodeURIComponent(row.customer_id)}`,
    })),
    hasMore,
    nextCursor: hasMore && items.length ? encodeCustomerActivityCursor(items.at(-1)!) : null,
    queryHash,
  };
}

async function readCustomerActivityEvidence(db: SqlDatabase, actor: WorkContext, recordId: string, filters: Filters) {
  await requireCustomerActivityEvidence(db, actor);
  const source = customerTimelineEvents(actor);
  const row = await db.prepare(`${source.sql} WHERE e.id=? AND e.at>=? AND e.at<? LIMIT 1`)
    .bind(...source.values, recordId, Date.parse(filters.from), Date.parse(filters.to) + 86400000).first<CustomerActivityItem>();
  if (!row) throw new ApiError(404, "EVIDENCE_NOT_FOUND", "This source record is unavailable in the selected role, scope or filters.");
  const queryHash = await sha256(JSON.stringify({ providerId: customerActivityProviderId, metric: customerActivityMetric, filters, recordId }));
  await currentAuthority(db, actor);
  await audit(db, actor, customerActivityProviderId, customerActivityMetric, "open", [{ id: row.id, version: Number(row.version) }], customerActivityFields, queryHash);
  await currentAuthority(db, actor);
  return { provider: customerActivityProviderId, metric: customerActivityMetric, fields: customerActivityFields, record: { ...row, version: Number(row.version), at: Number(row.at) }, queryHash };
}

const socialActivityProviderId = "collaboration-social-activity";
const socialActivityFields = ["id", "source_id", "source_type", "title", "at", "version", "activity_actor", "thread_id", "href"];

function socialActivityCursor(raw?: string) {
  if (!raw) return null;
  try {
    const value = JSON.parse(Buffer.from(raw, "base64url").toString("utf8")) as { at?: unknown; id?: unknown };
    if (typeof value.at !== "number" || !Number.isSafeInteger(value.at) || typeof value.id !== "string") throw new Error();
    return { at: value.at, id: z.string().min(1).max(260).parse(value.id) };
  } catch {
    throw new ApiError(422, "EVIDENCE_CURSOR_INVALID", "The source record cursor is invalid. Refresh the evidence list.");
  }
}

function encodeSocialActivityCursor(row: Pick<SocialActivityEvent, "at" | "id">) {
  return Buffer.from(JSON.stringify({ at: Number(row.at), id: row.id })).toString("base64url");
}

async function requireSocialActivityEvidence(db: SqlDatabase, actor: WorkContext) {
  await currentAuthority(db, actor);
  if (!hasPermission(actor, "records.read"))
    throw new ApiError(403, "EVIDENCE_FORBIDDEN", "This source record is not available in the current role or scope.");
  const catalog = await metricCatalogFor(db, actor);
  if (!catalog.some((item) => item.id === socialActivityMetric && item.available))
    throw new ApiError(403, "EVIDENCE_FORBIDDEN", "This source record is not available in the current role or scope.");
  await assertProductionModuleEnabled(db, actor.companyId, "collaboration");
}

async function searchSocialActivityEvidence(db: SqlDatabase, actor: WorkContext, input: z.infer<typeof socialActivityQuerySchema>) {
  await requireSocialActivityEvidence(db, actor);
  const source = socialActivityEvents(actor), cursor = socialActivityCursor(input.cursor);
  const conditions = ["e.at>=?", "e.at<?"];
  const values: unknown[] = [...source.values, Date.parse(input.filters.from), Date.parse(input.filters.to) + 86400000];
  if (input.query) {
    conditions.push("position(lower(?) in lower(concat_ws(' ',e.id,e.source_id,e.source_type,e.title,e.activity_actor,e.thread_id)))>0");
    values.push(input.query);
  }
  if (cursor) { conditions.push("(e.at,e.id)<(?,?)"); values.push(cursor.at, cursor.id); }
  const rows = (await db.prepare(`${source.sql} WHERE ${conditions.join(" AND ")} ORDER BY e.at DESC,e.id DESC LIMIT ?`)
    .bind(...values, input.limit + 1).all<SocialActivityEvent>()).results;
  const hasMore = rows.length > input.limit, items = rows.slice(0, input.limit);
  const queryHash = await sha256(JSON.stringify({ providerId: socialActivityProviderId, metric: input.metric, filters: input.filters, query: input.query, cursor: input.cursor ?? null }));
  await currentAuthority(db, actor);
  await audit(db, actor, socialActivityProviderId, input.metric, "search", items.map(({ id, version }) => ({ id, version: Number(version) })), socialActivityFields, queryHash);
  await currentAuthority(db, actor);
  return {
    provider: socialActivityProviderId,
    metric: input.metric,
    fields: socialActivityFields,
    items: items.map((row) => ({
      id: row.id,
      label: row.title,
      summary: `${row.source_type} · ${row.activity_actor} · ${new Date(Number(row.at)).toISOString().slice(0, 10)}`,
      status: row.source_type,
      version: Number(row.version),
      href: row.href,
    })),
    hasMore,
    nextCursor: hasMore && items.length ? encodeSocialActivityCursor(items.at(-1)!) : null,
    queryHash,
  };
}

async function readSocialActivityEvidence(db: SqlDatabase, actor: WorkContext, recordId: string, filters: Filters) {
  await requireSocialActivityEvidence(db, actor);
  const source = socialActivityEvents(actor);
  const row = await db.prepare(`${source.sql} WHERE e.id=? AND e.at>=? AND e.at<? LIMIT 1`)
    .bind(...source.values, recordId, Date.parse(filters.from), Date.parse(filters.to) + 86400000).first<SocialActivityEvent>();
  if (!row) throw new ApiError(404, "EVIDENCE_NOT_FOUND", "This source record is unavailable in the selected role, scope or filters.");
  const queryHash = await sha256(JSON.stringify({ providerId: socialActivityProviderId, metric: socialActivityMetric, filters, recordId }));
  await currentAuthority(db, actor);
  await audit(db, actor, socialActivityProviderId, socialActivityMetric, "open", [{ id: row.id, version: Number(row.version) }], socialActivityFields, queryHash);
  await currentAuthority(db, actor);
  return { provider: socialActivityProviderId, metric: socialActivityMetric, fields: socialActivityFields, record: { ...row, at: Number(row.at), version: Number(row.version) }, queryHash };
}

type SupportIncidentEvidenceRow = {
  id: string;
  title: string;
  severity: "critical" | "high" | "normal";
  status: string;
  due_date: string;
  created_at: number;
  updated_at: number;
  version: number;
};
function supportIncidentCursorValue(cursor?: string) {
  if (!cursor) return null;
  try {
    const value = JSON.parse(Buffer.from(cursor, "base64url").toString("utf8")) as { updatedAt?: unknown; id?: unknown };
    if (typeof value.updatedAt !== "number" || !Number.isSafeInteger(value.updatedAt)) throw new Error();
    return { updatedAt: value.updatedAt, id: z.string().min(1).max(200).parse(value.id) };
  } catch {
    throw new ApiError(422, "EVIDENCE_CURSOR_INVALID", "The source record cursor is invalid. Refresh the evidence list.");
  }
}
function encodeSupportIncidentCursor(row: Pick<SupportIncidentEvidenceRow, "updated_at" | "id">) {
  return Buffer.from(JSON.stringify({ updatedAt: Number(row.updated_at), id: row.id })).toString("base64url");
}
function encodeSupportIncidentRecordId(id: string) {
  return Buffer.from(JSON.stringify({ incidentId: id })).toString("base64url");
}
function decodeSupportIncidentRecordId(id: string) {
  try {
    const value = JSON.parse(Buffer.from(id, "base64url").toString("utf8")) as { incidentId?: unknown };
    return z.string().min(1).max(200).parse(value.incidentId);
  } catch {
    throw new ApiError(404, "EVIDENCE_NOT_FOUND", "This source record is unavailable in the selected role, scope or filters.");
  }
}
function supportIncidentNumber(value: number | string | null | undefined, field: string) {
  const parsed = Number(value);
  if (value === null || value === undefined || !Number.isSafeInteger(parsed)) throw new ApiError(500, "EVIDENCE_VALUE_INVALID", `The source returned an invalid ${field}.`);
  return parsed;
}
async function requireSupportIncidentEvidence(db: SqlDatabase, actor: WorkContext) {
  await currentAuthority(db, actor);
  const catalog = await metricCatalogFor(db, actor);
  if (!hasPermission(actor, "support.read") || !catalog.some((item) => item.id === supportIncidentMetric && item.available))
    throw new ApiError(403, "EVIDENCE_FORBIDDEN", "This source record is not available in the current role or scope.");
  await assertProductionModuleEnabled(db, actor.companyId, "support");
}
async function searchSupportIncidentEvidence(db: SqlDatabase, actor: WorkContext, input: z.infer<typeof supportIncidentQuerySchema>) {
  await requireSupportIncidentEvidence(db, actor);
  const cursor = supportIncidentCursorValue(input.cursor), conditions = [
    "t.company_id=?", "(?::text IS NULL OR t.branch_id=?)", "(?::text IS NULL OR t.department_id=?)",
    "t.status IN ('open','investigating','mitigated')",
  ];
  const values: unknown[] = [actor.companyId, actor.branchId, actor.branchId, actor.departmentId, actor.departmentId];
  if (input.query) {
    conditions.push("position(lower(?) in lower(t.title||' '||t.severity||' '||t.status||' '||t.id))>0");
    values.push(input.query);
  }
  if (cursor) {
    conditions.push("(t.updated_at<? OR (t.updated_at=? AND t.id>?))");
    values.push(cursor.updatedAt, cursor.updatedAt, cursor.id);
  }
  const rows = (await db.prepare(`SELECT t.id,t.title,t.severity,t.status,t.due_date,t.created_at,t.updated_at,t.version
    FROM support_incidents t WHERE ${conditions.join(" AND ")} ORDER BY t.updated_at DESC,t.id ASC LIMIT ?`)
    .bind(...values, input.limit + 1).all<SupportIncidentEvidenceRow>()).results;
  const hasMore = rows.length > input.limit, items = rows.slice(0, input.limit);
  const queryHash = await sha256(JSON.stringify({ providerId: supportIncidentProviderId, metric: input.metric, filters: input.filters, query: input.query, cursor: input.cursor ?? null, scope: [actor.companyId, actor.assignmentId, actor.branchId, actor.departmentId] }));
  await currentAuthority(db, actor);
  await audit(db, actor, supportIncidentProviderId, input.metric, "search", items.map((row) => ({ id: row.id, version: supportIncidentNumber(row.version, "version") })), supportIncidentFields, queryHash);
  await currentAuthority(db, actor);
  return {
    provider: supportIncidentProviderId,
    metric: input.metric,
    fields: supportIncidentFields,
    items: items.map((row) => ({
      id: encodeSupportIncidentRecordId(row.id),
      label: row.title,
      summary: `${row.severity} · ${row.status} · Due ${row.due_date}`,
      status: row.status,
      href: `/support/incidents/${encodeURIComponent(row.id)}`,
    })),
    hasMore,
    nextCursor: hasMore && items.length ? encodeSupportIncidentCursor(items.at(-1)!) : null,
    queryHash,
  };
}
async function readSupportIncidentEvidence(db: SqlDatabase, actor: WorkContext, recordId: string, filters: Filters) {
  await requireSupportIncidentEvidence(db, actor);
  const id = decodeSupportIncidentRecordId(recordId), row = await getSupportIncident(actor, id);
  if (!["open", "investigating", "mitigated"].includes(String(row.status)))
    throw new ApiError(404, "EVIDENCE_NOT_FOUND", "This source record is unavailable in the selected role, scope or filters.");
  const version = supportIncidentNumber(row.version, "version");
  const record = {
    id: recordId,
    title: String(row.title),
    severity: String(row.severity),
    status: String(row.status),
    due_date: String(row.due_date),
    created_at: supportIncidentNumber(row.created_at, "creation time"),
    updated_at: supportIncidentNumber(row.updated_at, "update time"),
    version,
    href: `/support/incidents/${encodeURIComponent(id)}`,
  };
  const queryHash = await sha256(JSON.stringify({ providerId: supportIncidentProviderId, metric: supportIncidentMetric, filters, scope: [actor.companyId, actor.assignmentId, actor.branchId, actor.departmentId], recordId }));
  await currentAuthority(db, actor);
  await audit(db, actor, supportIncidentProviderId, supportIncidentMetric, "open", [{ id, version }], supportIncidentFields, queryHash);
  await currentAuthority(db, actor);
  return { provider: supportIncidentProviderId, metric: supportIncidentMetric, fields: supportIncidentFields, record, queryHash };
}

type SupportTicketEvidenceRow = {
  id: string;
  title: string;
  status: string;
  due_date: string;
  updated_at: number | string;
  version: number | string;
  sla_state?: string | null;
  sla_due_at?: string | null;
};
function supportTicketCursorValue(cursor?: string) {
  if (!cursor) return null;
  try {
    const value = JSON.parse(Buffer.from(cursor, "base64url").toString("utf8")) as { updatedAt?: unknown; id?: unknown };
    if (typeof value.updatedAt !== "number" || !Number.isSafeInteger(value.updatedAt)) throw new Error();
    return { updatedAt: value.updatedAt, id: z.string().min(1).max(200).parse(value.id) };
  } catch {
    throw new ApiError(422, "EVIDENCE_CURSOR_INVALID", "The source record cursor is invalid. Refresh the evidence list.");
  }
}
function encodeSupportTicketCursor(row: Pick<SupportTicketEvidenceRow, "updated_at" | "id">) {
  return Buffer.from(JSON.stringify({ updatedAt: Number(row.updated_at), id: row.id })).toString("base64url");
}
function encodeSupportTicketRecordId(id: string) {
  return Buffer.from(JSON.stringify({ ticketId: id })).toString("base64url");
}
function decodeSupportTicketRecordId(id: string) {
  try {
    const value = JSON.parse(Buffer.from(id, "base64url").toString("utf8")) as { ticketId?: unknown };
    return z.string().min(1).max(200).parse(value.ticketId);
  } catch {
    throw new ApiError(404, "EVIDENCE_NOT_FOUND", "This source record is unavailable in the selected role, scope or filters.");
  }
}
function supportTicketNumber(value: number | string | null | undefined, field: string) {
  const parsed = Number(value);
  if (value === null || value === undefined || !Number.isSafeInteger(parsed)) throw new ApiError(500, "EVIDENCE_VALUE_INVALID", `The source returned an invalid ${field}.`);
  return parsed;
}
function supportTicketStatePredicate(metric: SupportTicketMetricId, alias = "t") {
  const base = `${alias}.status NOT IN ('closed','cancelled')`;
  if (metric === "open-support-tickets") return base;
  return `${base} AND ${alias}.sla_clock_json->>'state'='running' AND CASE
    WHEN ${alias}.sla_clock_json->>'dueAt' ~ '^[0-9]+$'
      THEN (${alias}.sla_clock_json->>'dueAt')::numeric < floor(extract(epoch FROM clock_timestamp())*1000)::numeric
    ELSE FALSE END`;
}
function supportTicketFieldsFor(metric: SupportTicketMetricId) {
  return metric === "overdue-support-sla" ? supportTicketSlaFields : supportTicketFields;
}
async function requireSupportTicketEvidence(db: SqlDatabase, actor: WorkContext, metric: SupportTicketMetricId) {
  await currentAuthority(db, actor);
  const catalog = await metricCatalogFor(db, actor);
  if (!hasPermission(actor, "support.read") || !catalog.some((item) => item.id === metric && item.available))
    throw new ApiError(403, "EVIDENCE_FORBIDDEN", "This source record is not available in the current role or scope.");
  await assertProductionModuleEnabled(db, actor.companyId, "support");
}
async function searchSupportTicketEvidence(db: SqlDatabase, actor: WorkContext, input: z.infer<typeof supportTicketQuerySchema>) {
  await requireSupportTicketEvidence(db, actor, input.metric);
  const cursor = supportTicketCursorValue(input.cursor), overdue = input.metric === "overdue-support-sla";
  const conditions = [
    "t.company_id=?", "(?::text IS NULL OR t.branch_id=?)", "(?::text IS NULL OR t.department_id=?)",
    supportTicketStatePredicate(input.metric),
  ];
  const values: unknown[] = [actor.companyId, actor.branchId, actor.branchId, actor.departmentId, actor.departmentId];
  if (input.query) {
    const querySource = overdue
      ? "concat_ws(' ',t.id,t.title,t.status,t.due_date,t.sla_clock_json->>'state',t.sla_clock_json->>'dueAt')"
      : "concat_ws(' ',t.id,t.title,t.status,t.due_date)";
    conditions.push(`position(lower(?) in lower(${querySource}))>0`);
    values.push(input.query);
  }
  if (cursor) {
    conditions.push("(t.updated_at<? OR (t.updated_at=? AND t.id>?))");
    values.push(cursor.updatedAt, cursor.updatedAt, cursor.id);
  }
  const rows = (await db.prepare(`SELECT t.id,t.title,t.status,t.due_date,t.updated_at,t.version${overdue ? ",t.sla_clock_json->>'state' sla_state,t.sla_clock_json->>'dueAt' sla_due_at" : ""}
    FROM support_tickets t WHERE ${conditions.join(" AND ")} ORDER BY t.updated_at DESC,t.id ASC LIMIT ?`)
    .bind(...values, input.limit + 1).all<SupportTicketEvidenceRow>()).results;
  const hasMore = rows.length > input.limit, items = rows.slice(0, input.limit), fields = supportTicketFieldsFor(input.metric);
  const queryHash = await sha256(JSON.stringify({ providerId: supportTicketProviderId, metric: input.metric, filters: input.filters, query: input.query, cursor: input.cursor ?? null, scope: [actor.companyId, actor.assignmentId, actor.branchId, actor.departmentId] }));
  await currentAuthority(db, actor);
  await audit(db, actor, supportTicketProviderId, input.metric, "search", items.map((row) => ({ id: row.id, version: supportTicketNumber(row.version, "version") })), fields, queryHash);
  await currentAuthority(db, actor);
  return {
    provider: supportTicketProviderId,
    metric: input.metric,
    fields,
    items: items.map((row) => {
      const dueAt = overdue ? supportTicketNumber(row.sla_due_at, "SLA due time") : null;
      return {
        id: encodeSupportTicketRecordId(row.id),
        label: row.title,
        summary: overdue ? `Overdue SLA · due ${new Date(dueAt!).toISOString()}` : `${row.status} · due ${row.due_date}`,
        status: row.status,
        href: `/support/${encodeURIComponent(row.id)}`,
      };
    }),
    hasMore,
    nextCursor: hasMore && items.length ? encodeSupportTicketCursor(items.at(-1)!) : null,
    queryHash,
  };
}
async function readSupportTicketEvidence(db: SqlDatabase, actor: WorkContext, metric: SupportTicketMetricId, recordId: string, filters: Filters) {
  await requireSupportTicketEvidence(db, actor, metric);
  const id = decodeSupportTicketRecordId(recordId), overdue = metric === "overdue-support-sla";
  const row = await db.prepare(`SELECT t.id,t.title,t.status,t.due_date,t.updated_at,t.version${overdue ? ",t.sla_clock_json->>'state' sla_state,t.sla_clock_json->>'dueAt' sla_due_at" : ""}
    FROM support_tickets t WHERE t.id=? AND t.company_id=? AND (?::text IS NULL OR t.branch_id=?) AND (?::text IS NULL OR t.department_id=?) AND ${supportTicketStatePredicate(metric)} LIMIT 1`)
    .bind(id, actor.companyId, actor.branchId, actor.branchId, actor.departmentId, actor.departmentId).first<SupportTicketEvidenceRow>();
  if (!row) throw new ApiError(404, "EVIDENCE_NOT_FOUND", "This source record is unavailable in the selected role, scope or filters.");
  const fields = supportTicketFieldsFor(metric), record = {
    id: recordId,
    title: String(row.title),
    status: String(row.status),
    due_date: String(row.due_date),
    updated_at: supportTicketNumber(row.updated_at, "update time"),
    version: supportTicketNumber(row.version, "version"),
    ...(overdue ? { sla_state: String(row.sla_state), sla_due_at: supportTicketNumber(row.sla_due_at, "SLA due time") } : {}),
    href: `/support/${encodeURIComponent(id)}`,
  };
  const queryHash = await sha256(JSON.stringify({ providerId: supportTicketProviderId, metric, filters, scope: [actor.companyId, actor.assignmentId, actor.branchId, actor.departmentId], recordId }));
  await currentAuthority(db, actor);
  await audit(db, actor, supportTicketProviderId, metric, "open", [{ id, version: record.version }], fields, queryHash);
  await currentAuthority(db, actor);
  return { provider: supportTicketProviderId, metric, fields, record, queryHash };
}

type DuplicateReviewEvidenceRow = {
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
function duplicateReviewCursorValue(cursor?: string) {
  if (!cursor) return null;
  try {
    const value = JSON.parse(Buffer.from(cursor, "base64url").toString("utf8")) as { updatedAt?: unknown; id?: unknown };
    if (typeof value.updatedAt !== "number" || !Number.isSafeInteger(value.updatedAt)) throw new Error();
    return { updatedAt: value.updatedAt, id: z.string().min(1).max(200).parse(value.id) };
  } catch {
    throw new ApiError(422, "EVIDENCE_CURSOR_INVALID", "The source record cursor is invalid. Refresh the evidence list.");
  }
}
function encodeDuplicateReviewCursor(row: Pick<DuplicateReviewEvidenceRow, "updated_at" | "id">) {
  return Buffer.from(JSON.stringify({ updatedAt: Number(row.updated_at), id: row.id })).toString("base64url");
}
function encodeDuplicateReviewRecordId(id: string) {
  return Buffer.from(JSON.stringify({ reviewId: id })).toString("base64url");
}
function decodeDuplicateReviewRecordId(id: string) {
  try {
    const value = JSON.parse(Buffer.from(id, "base64url").toString("utf8")) as { reviewId?: unknown };
    return z.string().min(1).max(200).parse(value.reviewId);
  } catch {
    throw new ApiError(404, "EVIDENCE_NOT_FOUND", "This source record is unavailable in the selected role, scope or filters.");
  }
}
function duplicateReviewNumber(value: number | string, field: string) {
  const parsed = Number(value);
  if (!Number.isSafeInteger(parsed)) throw new ApiError(500, "EVIDENCE_VALUE_INVALID", `The source returned an invalid ${field}.`);
  return parsed;
}
async function requireDuplicateReviewEvidence(db: SqlDatabase, actor: WorkContext) {
  await currentAuthority(db, actor);
  const catalog = await metricCatalogFor(db, actor);
  if (!hasPermission(actor, "commerce.read") || !hasPermission(actor, "commerce.sales") || actor.branchId || actor.departmentId || !catalog.some((item) => item.id === duplicateReviewMetric && item.available))
    throw new ApiError(403, "EVIDENCE_FORBIDDEN", "This source record is not available in the current role or scope.");
  await assertProductionModuleEnabled(db, actor.companyId, "customer_authority");
}
async function searchDuplicateReviewEvidence(db: SqlDatabase, actor: WorkContext, input: z.infer<typeof duplicateReviewQuerySchema>) {
  await requireDuplicateReviewEvidence(db, actor);
  const cursor = duplicateReviewCursorValue(input.cursor), conditions = ["r.company_id=?", "r.status='pending'"], values: unknown[] = [actor.companyId];
  if (input.query) {
    conditions.push("position(lower(?) in lower(concat_ws(' ',r.id,r.status,l.name,l.code,q.name,q.code)))>0");
    values.push(input.query);
  }
  if (cursor) {
    conditions.push("(r.updated_at<? OR (r.updated_at=? AND r.id>?))");
    values.push(cursor.updatedAt, cursor.updatedAt, cursor.id);
  }
  const rows = (await db.prepare(`SELECT r.id,l.name left_name,l.code left_code,q.name right_name,q.code right_code,r.status,r.created_at,r.updated_at,r.version
    FROM customer_duplicate_reviews r
    JOIN finance_parties l ON l.id=r.left_customer_id AND l.company_id=r.company_id AND l.kind='customer'
    JOIN finance_parties q ON q.id=r.right_customer_id AND q.company_id=r.company_id AND q.kind='customer'
    WHERE ${conditions.join(" AND ")} ORDER BY r.updated_at DESC,r.id ASC LIMIT ?`)
    .bind(...values, input.limit + 1).all<DuplicateReviewEvidenceRow>()).results;
  const hasMore = rows.length > input.limit, items = rows.slice(0, input.limit);
  const queryHash = await sha256(JSON.stringify({ providerId: duplicateReviewProviderId, metric: input.metric, filters: input.filters, query: input.query, cursor: input.cursor ?? null, scope: [actor.companyId, actor.assignmentId, actor.branchId, actor.departmentId] }));
  await currentAuthority(db, actor);
  await audit(db, actor, duplicateReviewProviderId, input.metric, "search", items.map((row) => ({ id: row.id, version: duplicateReviewNumber(row.version, "version") })), duplicateReviewFields, queryHash);
  await currentAuthority(db, actor);
  return {
    provider: duplicateReviewProviderId,
    metric: input.metric,
    fields: duplicateReviewFields,
    items: items.map((row) => ({
      id: encodeDuplicateReviewRecordId(row.id),
      label: `${row.left_code} · ${row.left_name} ↔ ${row.right_code} · ${row.right_name}`,
      summary: `Pending · updated ${new Date(duplicateReviewNumber(row.updated_at, "update time")).toISOString().slice(0, 10)}`,
      status: row.status,
      href: `/customer-duplicates/${encodeURIComponent(row.id)}`,
    })),
    hasMore,
    nextCursor: hasMore && items.length ? encodeDuplicateReviewCursor(items.at(-1)!) : null,
    queryHash,
  };
}
async function readDuplicateReviewEvidence(db: SqlDatabase, actor: WorkContext, recordId: string, filters: Filters) {
  await requireDuplicateReviewEvidence(db, actor);
  const id = decodeDuplicateReviewRecordId(recordId), row = await db.prepare(`SELECT r.id,l.name left_name,l.code left_code,q.name right_name,q.code right_code,r.status,r.created_at,r.updated_at,r.version
    FROM customer_duplicate_reviews r
    JOIN finance_parties l ON l.id=r.left_customer_id AND l.company_id=r.company_id AND l.kind='customer'
    JOIN finance_parties q ON q.id=r.right_customer_id AND q.company_id=r.company_id AND q.kind='customer'
    WHERE r.company_id=? AND r.status='pending' AND r.id=? LIMIT 1`).bind(actor.companyId, id).first<DuplicateReviewEvidenceRow>();
  if (!row) throw new ApiError(404, "EVIDENCE_NOT_FOUND", "This source record is unavailable in the selected role, scope or filters.");
  const record = {
    id: recordId,
    left_name: row.left_name,
    left_code: row.left_code,
    right_name: row.right_name,
    right_code: row.right_code,
    status: row.status,
    created_at: duplicateReviewNumber(row.created_at, "creation time"),
    updated_at: duplicateReviewNumber(row.updated_at, "update time"),
    version: duplicateReviewNumber(row.version, "version"),
    href: `/customer-duplicates/${encodeURIComponent(id)}`,
  };
  const queryHash = await sha256(JSON.stringify({ providerId: duplicateReviewProviderId, metric: duplicateReviewMetric, filters, scope: [actor.companyId, actor.assignmentId, actor.branchId, actor.departmentId], recordId }));
  await currentAuthority(db, actor);
  await audit(db, actor, duplicateReviewProviderId, duplicateReviewMetric, "open", [{ id, version: record.version }], duplicateReviewFields, queryHash);
  await currentAuthority(db, actor);
  return { provider: duplicateReviewProviderId, metric: duplicateReviewMetric, fields: duplicateReviewFields, record, queryHash };
}

type IdentityChangeEvidenceRow = {
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
function identityChangeCursorValue(cursor?: string) {
  if (!cursor) return null;
  try {
    const value = JSON.parse(Buffer.from(cursor, "base64url").toString("utf8")) as { at?: unknown; id?: unknown };
    if (typeof value.at !== "number" || !Number.isSafeInteger(value.at)) throw new Error();
    return { at: value.at, id: z.string().min(1).max(200).parse(value.id) };
  } catch {
    throw new ApiError(422, "EVIDENCE_CURSOR_INVALID", "The source record cursor is invalid. Refresh the evidence list.");
  }
}
function encodeIdentityChangeCursor(row: Pick<IdentityChangeEvidenceRow, "created_at" | "id">) {
  return Buffer.from(JSON.stringify({ at: Number(row.created_at), id: row.id })).toString("base64url");
}
function encodeIdentityChangeRecordId(id: string) {
  return Buffer.from(JSON.stringify({ eventId: id })).toString("base64url");
}
function decodeIdentityChangeRecordId(id: string) {
  try {
    const value = JSON.parse(Buffer.from(id, "base64url").toString("utf8")) as { eventId?: unknown };
    return z.string().min(1).max(200).parse(value.eventId);
  } catch {
    throw new ApiError(404, "EVIDENCE_NOT_FOUND", "This source record is unavailable in the selected role, scope or filters.");
  }
}
function identityChangeNumber(value: number | string, field: string) {
  const parsed = Number(value);
  if (!Number.isSafeInteger(parsed)) throw new ApiError(500, "EVIDENCE_VALUE_INVALID", `The source returned an invalid ${field}.`);
  return parsed;
}
async function requireIdentityChangeEvidence(db: SqlDatabase, actor: WorkContext) {
  await currentAuthority(db, actor);
  const catalog = await metricCatalogFor(db, actor);
  if (!hasPermission(actor, "commerce.read") || !hasPermission(actor, "commerce.sales") || actor.branchId || actor.departmentId || !catalog.some((item) => item.id === identityChangeMetric && item.available))
    throw new ApiError(403, "EVIDENCE_FORBIDDEN", "This source record is not available in the current role or scope.");
  await assertProductionModuleEnabled(db, actor.companyId, "customer_authority");
}
const identityChangeSource = `SELECT e.id,e.change_id,e.action,e.customer_version,e.created_at,e.version,c.status change_status,c.version change_version,p.name customer_name,p.code customer_code
  FROM customer_identity_change_events e
  JOIN customer_identity_changes c ON c.id=e.change_id
  JOIN finance_parties p ON p.id=c.customer_id AND p.company_id=c.company_id AND p.kind='customer'
  WHERE c.company_id=? AND e.created_at>=? AND e.created_at<?`;
async function searchIdentityChangeEvidence(db: SqlDatabase, actor: WorkContext, input: z.infer<typeof identityChangeQuerySchema>) {
  await requireIdentityChangeEvidence(db, actor);
  const cursor = identityChangeCursorValue(input.cursor), conditions = [] as string[];
  const values: unknown[] = [actor.companyId, Date.parse(input.filters.from), Date.parse(input.filters.to) + 86400000];
  if (input.query) {
    conditions.push("position(lower(?) in lower(concat_ws(' ',e.id,e.action,c.status,p.name,p.code)))>0");
    values.push(input.query);
  }
  if (cursor) {
    conditions.push("(e.created_at<? OR (e.created_at=? AND e.id>?))");
    values.push(cursor.at, cursor.at, cursor.id);
  }
  const rows = (await db.prepare(`${identityChangeSource}${conditions.length ? ` AND ${conditions.join(" AND ")}` : ""}
    ORDER BY e.created_at DESC,e.id ASC LIMIT ?`).bind(...values, input.limit + 1).all<IdentityChangeEvidenceRow>()).results;
  const hasMore = rows.length > input.limit, items = rows.slice(0, input.limit);
  const queryHash = await sha256(JSON.stringify({ providerId: identityChangeProviderId, metric: input.metric, filters: input.filters, query: input.query, cursor: input.cursor ?? null, scope: [actor.companyId, actor.assignmentId, actor.branchId, actor.departmentId] }));
  await currentAuthority(db, actor);
  await audit(db, actor, identityChangeProviderId, input.metric, "search", items.map((row) => ({ id: row.id, version: identityChangeNumber(row.version, "event version") })), identityChangeFields, queryHash);
  await currentAuthority(db, actor);
  return {
    provider: identityChangeProviderId,
    metric: input.metric,
    fields: identityChangeFields,
    items: items.map((row) => ({
      id: encodeIdentityChangeRecordId(row.id),
      label: `${row.customer_code} · ${row.customer_name} · ${row.action}`,
      summary: `Change ${row.change_status} · ${new Date(identityChangeNumber(row.created_at, "event time")).toISOString().slice(0, 10)}`,
      status: row.action,
      href: `/customer-identity/${encodeURIComponent(row.change_id)}`,
    })),
    hasMore,
    nextCursor: hasMore && items.length ? encodeIdentityChangeCursor(items.at(-1)!) : null,
    queryHash,
  };
}
async function readIdentityChangeEvidence(db: SqlDatabase, actor: WorkContext, recordId: string, filters: Filters) {
  await requireIdentityChangeEvidence(db, actor);
  const id = decodeIdentityChangeRecordId(recordId), row = await db.prepare(`${identityChangeSource} AND e.id=? LIMIT 1`)
    .bind(actor.companyId, Date.parse(filters.from), Date.parse(filters.to) + 86400000, id).first<IdentityChangeEvidenceRow>();
  if (!row) throw new ApiError(404, "EVIDENCE_NOT_FOUND", "This source record is unavailable in the selected role, scope or filters.");
  const record = {
    id: recordId,
    change_id: row.change_id,
    action: row.action,
    customer_version: identityChangeNumber(row.customer_version, "customer version"),
    created_at: identityChangeNumber(row.created_at, "event time"),
    version: identityChangeNumber(row.version, "event version"),
    change_status: row.change_status,
    change_version: identityChangeNumber(row.change_version, "change version"),
    customer_name: row.customer_name,
    customer_code: row.customer_code,
    href: `/customer-identity/${encodeURIComponent(row.change_id)}`,
  };
  const queryHash = await sha256(JSON.stringify({ providerId: identityChangeProviderId, metric: identityChangeMetric, filters, scope: [actor.companyId, actor.assignmentId, actor.branchId, actor.departmentId], recordId }));
  await currentAuthority(db, actor);
  await audit(db, actor, identityChangeProviderId, identityChangeMetric, "open", [{ id: row.id, version: record.version }], identityChangeFields, queryHash);
  await currentAuthority(db, actor);
  return { provider: identityChangeProviderId, metric: identityChangeMetric, fields: identityChangeFields, record, queryHash };
}

type OrganizationNodeEvidenceRow = {
  id: string;
  kind: string;
  slug: string;
  display_name: string;
  parent_name: string | null;
  branch_name: string | null;
  status: string;
  hierarchy_revision: number | string;
};
type PendingStructureEvidenceRow = {
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
function organizationEvidenceCursor(cursor?: string) {
  if (!cursor) return null;
  try {
    const value = JSON.parse(Buffer.from(cursor, "base64url").toString("utf8")) as { name?: unknown; at?: unknown; id?: unknown };
    return {
      name: value.name === undefined ? undefined : z.string().max(200).parse(value.name),
      at: value.at === undefined ? undefined : z.number().int().safe().parse(value.at),
      id: z.string().min(1).max(200).parse(value.id),
    };
  } catch {
    throw new ApiError(422, "EVIDENCE_CURSOR_INVALID", "The source record cursor is invalid. Refresh the evidence list.");
  }
}
function encodeOrganizationRecordId(metric: typeof organizationNodeMetric | typeof pendingStructureMetric, id: string) {
  return Buffer.from(JSON.stringify({ metric, id })).toString("base64url");
}
function decodeOrganizationRecordId(recordId: string, metric: typeof organizationNodeMetric | typeof pendingStructureMetric) {
  try {
    const value = JSON.parse(Buffer.from(recordId, "base64url").toString("utf8")) as { metric?: unknown; id?: unknown };
    if (value.metric !== metric) throw new Error();
    return z.string().min(1).max(200).parse(value.id);
  } catch {
    throw new ApiError(404, "EVIDENCE_NOT_FOUND", "This source record is unavailable in the selected role, scope or filters.");
  }
}
function organizationEvidenceNumber(value: number | string, field: string) {
  const parsed = Number(value);
  if (!Number.isSafeInteger(parsed)) throw new ApiError(500, "EVIDENCE_VALUE_INVALID", `The source returned an invalid ${field}.`);
  return parsed;
}
async function requireOrganizationEvidence(db: SqlDatabase, actor: WorkContext, metric: typeof organizationNodeMetric | typeof pendingStructureMetric) {
  await currentAuthority(db, actor);
  const catalog = await metricCatalogFor(db, actor);
  if (!hasPermission(actor, "organization.manage") || !catalog.some((item) => item.id === metric && item.available))
    throw new ApiError(403, "EVIDENCE_FORBIDDEN", "This source record is not available in the current role or scope.");
  await assertProductionModuleEnabled(db, actor.companyId, "organization");
}
const organizationNodeEvidenceSource = `SELECT n.id,n.kind,n.slug,n.display_name,n.status,n.hierarchy_revision,p.display_name parent_name,
  b.display_name branch_name FROM organization_nodes n
  LEFT JOIN organization_nodes p ON p.id=n.parent_id AND p.company_id=n.company_id
  LEFT JOIN organization_nodes b ON b.id=CASE WHEN n.kind='branch' THEN n.id ELSE n.branch_id END AND b.company_id=n.company_id
  WHERE n.company_id=? AND n.kind IN ('branch','department') AND n.status='active'
    AND (?::text IS NULL OR n.id=? OR n.branch_id=?) AND (?::text IS NULL OR n.id=?)`;
async function searchOrganizationNodeEvidence(db: SqlDatabase, actor: WorkContext, input: z.infer<typeof organizationNodeQuerySchema>) {
  await requireOrganizationEvidence(db, actor, organizationNodeMetric);
  const cursor = organizationEvidenceCursor(input.cursor), conditions: string[] = [];
  const values: unknown[] = [actor.companyId, actor.branchId, actor.branchId, actor.branchId, actor.departmentId, actor.departmentId];
  if (input.query) {
    conditions.push("position(lower(?) in lower(concat_ws(' ',n.display_name,n.kind,n.slug,b.display_name)))>0");
    values.push(input.query);
  }
  if (cursor) {
    if (cursor.name === undefined || cursor.at !== undefined) throw new ApiError(422, "EVIDENCE_CURSOR_INVALID", "The source record cursor is invalid. Refresh the evidence list.");
    conditions.push("(n.display_name>? OR (n.display_name=? AND n.id>?))");
    values.push(cursor.name, cursor.name, cursor.id);
  }
  const rows = (await db.prepare(`${organizationNodeEvidenceSource}${conditions.length ? ` AND ${conditions.join(" AND ")}` : ""}
    ORDER BY n.display_name,n.id LIMIT ?`).bind(...values, input.limit + 1).all<OrganizationNodeEvidenceRow>()).results;
  const hasMore = rows.length > input.limit, items = rows.slice(0, input.limit);
  const queryHash = await sha256(JSON.stringify({ providerId: organizationProviderId, metric: input.metric, filters: input.filters, query: input.query, cursor: input.cursor ?? null, scope: [actor.companyId, actor.assignmentId, actor.branchId, actor.departmentId] }));
  await currentAuthority(db, actor);
  await audit(db, actor, organizationProviderId, input.metric, "search", items.map((row) => ({ id: row.id, version: organizationEvidenceNumber(row.hierarchy_revision, "hierarchy revision") })), organizationNodeFields, queryHash);
  await currentAuthority(db, actor);
  return {
    provider: organizationProviderId,
    metric: input.metric,
    fields: organizationNodeFields,
    items: items.map((row) => ({
      id: encodeOrganizationRecordId(organizationNodeMetric, row.id),
      label: `${row.display_name} · ${row.kind}`,
      summary: `${row.kind} · ${row.branch_name ?? "Company-wide"}`,
      status: row.status,
      href: "/people/organization",
    })),
    hasMore,
    nextCursor: hasMore && items.length ? Buffer.from(JSON.stringify({ name: items.at(-1)!.display_name, id: items.at(-1)!.id })).toString("base64url") : null,
    queryHash,
  };
}
async function readOrganizationNodeEvidence(db: SqlDatabase, actor: WorkContext, recordId: string, filters: Filters) {
  await requireOrganizationEvidence(db, actor, organizationNodeMetric);
  const id = decodeOrganizationRecordId(recordId, organizationNodeMetric), values = [actor.companyId, actor.branchId, actor.branchId, actor.branchId, actor.departmentId, actor.departmentId, id];
  const row = await db.prepare(`${organizationNodeEvidenceSource} AND n.id=? LIMIT 1`).bind(...values).first<OrganizationNodeEvidenceRow>();
  if (!row) throw new ApiError(404, "EVIDENCE_NOT_FOUND", "This source record is unavailable in the selected role, scope or filters.");
  const record = {
    id: recordId,
    kind: row.kind,
    slug: row.slug,
    display_name: row.display_name,
    status: row.status,
    branch_name: row.branch_name ?? "Company-wide",
    parent_name: row.parent_name ?? "Company-wide",
    hierarchy_revision: organizationEvidenceNumber(row.hierarchy_revision, "hierarchy revision"),
    href: "/people/organization",
  };
  const queryHash = await sha256(JSON.stringify({ providerId: organizationProviderId, metric: organizationNodeMetric, filters, scope: [actor.companyId, actor.assignmentId, actor.branchId, actor.departmentId], recordId }));
  await currentAuthority(db, actor);
  await audit(db, actor, organizationProviderId, organizationNodeMetric, "open", [{ id: row.id, version: record.hierarchy_revision }], organizationNodeFields, queryHash);
  await currentAuthority(db, actor);
  return { provider: organizationProviderId, metric: organizationNodeMetric, fields: organizationNodeFields, record, queryHash };
}
async function requirePendingStructureEvidence(db: SqlDatabase, actor: WorkContext) {
  await requireOrganizationEvidence(db, actor, pendingStructureMetric);
  if (actor.departmentId || !await canReadOrganizationChanges(db, actor))
    throw new ApiError(403, "EVIDENCE_FORBIDDEN", "This source record is not available in the current role or scope.");
}
const pendingStructureEvidenceSource = `SELECT c.id,c.action,c.node_kind,c.display_name,c.state,c.version,c.created_at,c.updated_at,
  COALESCE(b.display_name,'Company-wide') branch_name FROM organization_node_changes c
  LEFT JOIN organization_nodes b ON b.id=c.branch_id AND b.company_id=c.company_id
  WHERE c.company_id=? AND c.state='submitted' AND (?::text IS NULL OR c.branch_id=?)`;
async function searchPendingStructureEvidence(db: SqlDatabase, actor: WorkContext, input: z.infer<typeof pendingStructureQuerySchema>) {
  await requirePendingStructureEvidence(db, actor);
  const cursor = organizationEvidenceCursor(input.cursor), conditions: string[] = [];
  const values: unknown[] = [actor.companyId, actor.branchId, actor.branchId];
  if (input.query) {
    conditions.push("position(lower(?) in lower(concat_ws(' ',c.action,c.node_kind,c.display_name,c.state,b.display_name)))>0");
    values.push(input.query);
  }
  if (cursor) {
    if (cursor.at === undefined || cursor.name !== undefined) throw new ApiError(422, "EVIDENCE_CURSOR_INVALID", "The source record cursor is invalid. Refresh the evidence list.");
    conditions.push("(c.updated_at<? OR (c.updated_at=? AND c.id>?))");
    values.push(cursor.at, cursor.at, cursor.id);
  }
  const rows = (await db.prepare(`${pendingStructureEvidenceSource}${conditions.length ? ` AND ${conditions.join(" AND ")}` : ""}
    ORDER BY c.updated_at DESC,c.id ASC LIMIT ?`).bind(...values, input.limit + 1).all<PendingStructureEvidenceRow>()).results;
  const hasMore = rows.length > input.limit, items = rows.slice(0, input.limit);
  const queryHash = await sha256(JSON.stringify({ providerId: organizationProviderId, metric: input.metric, filters: input.filters, query: input.query, cursor: input.cursor ?? null, scope: [actor.companyId, actor.assignmentId, actor.branchId, actor.departmentId] }));
  await currentAuthority(db, actor);
  await audit(db, actor, organizationProviderId, input.metric, "search", items.map((row) => ({ id: row.id, version: organizationEvidenceNumber(row.version, "change version") })), pendingStructureFields, queryHash);
  await currentAuthority(db, actor);
  return {
    provider: organizationProviderId,
    metric: input.metric,
    fields: pendingStructureFields,
    items: items.map((row) => ({
      id: encodeOrganizationRecordId(pendingStructureMetric, row.id),
      label: `${row.action} ${row.node_kind} · ${row.display_name}`,
      summary: `Submitted · ${row.branch_name}`,
      status: row.state,
      href: organizationChangeHref(row.id),
    })),
    hasMore,
    nextCursor: hasMore && items.length ? Buffer.from(JSON.stringify({ at: organizationEvidenceNumber(items.at(-1)!.updated_at, "update time"), id: items.at(-1)!.id })).toString("base64url") : null,
    queryHash,
  };
}
async function readPendingStructureEvidence(db: SqlDatabase, actor: WorkContext, recordId: string, filters: Filters) {
  await requirePendingStructureEvidence(db, actor);
  const id = decodeOrganizationRecordId(recordId, pendingStructureMetric), row = await db.prepare(`${pendingStructureEvidenceSource} AND c.id=? LIMIT 1`)
    .bind(actor.companyId, actor.branchId, actor.branchId, id).first<PendingStructureEvidenceRow>();
  if (!row) throw new ApiError(404, "EVIDENCE_NOT_FOUND", "This source record is unavailable in the selected role, scope or filters.");
  const record = {
    id: recordId,
    action: row.action,
    node_kind: row.node_kind,
    display_name: row.display_name,
    state: row.state,
    version: organizationEvidenceNumber(row.version, "change version"),
    created_at: organizationEvidenceNumber(row.created_at, "creation time"),
    updated_at: organizationEvidenceNumber(row.updated_at, "update time"),
    branch_name: row.branch_name,
    href: organizationChangeHref(row.id),
  };
  const queryHash = await sha256(JSON.stringify({ providerId: organizationProviderId, metric: pendingStructureMetric, filters, scope: [actor.companyId, actor.assignmentId, actor.branchId, actor.departmentId], recordId }));
  await currentAuthority(db, actor);
  await audit(db, actor, organizationProviderId, pendingStructureMetric, "open", [{ id: row.id, version: record.version }], pendingStructureFields, queryHash);
  await currentAuthority(db, actor);
  return { provider: organizationProviderId, metric: pendingStructureMetric, fields: pendingStructureFields, record, queryHash };
}

type ActiveAssignmentEvidenceRow = {
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
type PendingAccessReviewEvidenceRow = {
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
function identityAccessCursor(cursor?: string) {
  if (!cursor) return null;
  try {
    const value = JSON.parse(Buffer.from(cursor, "base64url").toString("utf8")) as { userId?: unknown; role?: unknown; createdAt?: unknown; recordId?: unknown };
    if (value.userId !== undefined || value.role !== undefined) {
      return { userId: z.string().min(1).max(200).parse(value.userId), role: z.string().min(1).max(200).parse(value.role), createdAt: undefined, recordId: z.string().min(1).max(200).parse(value.recordId) };
    }
    return { userId: undefined, role: undefined, createdAt: z.number().int().safe().parse(value.createdAt), recordId: z.string().min(1).max(200).parse(value.recordId) };
  } catch {
    throw new ApiError(422, "EVIDENCE_CURSOR_INVALID", "The source record cursor is invalid. Refresh the evidence list.");
  }
}
function encodeIdentityAccessRecordId(metric: typeof activeAssignmentsMetric | typeof pendingAccessReviewsMetric, id: string) {
  return Buffer.from(JSON.stringify({ metric, id })).toString("base64url");
}
function decodeIdentityAccessRecordId(recordId: string, metric: typeof activeAssignmentsMetric | typeof pendingAccessReviewsMetric) {
  try {
    const value = JSON.parse(Buffer.from(recordId, "base64url").toString("utf8")) as { metric?: unknown; id?: unknown };
    if (value.metric !== metric) throw new Error();
    return z.string().min(1).max(200).parse(value.id);
  } catch {
    throw new ApiError(404, "EVIDENCE_NOT_FOUND", "This source record is unavailable in the selected role, scope or filters.");
  }
}
async function identityAccessUserNames(userIds: string[]) {
  const ids = [...new Set(userIds)].slice(0, 50);
  if (!ids.length) return new Map<string, string>();
  const control = (await runtimeEnv()).CONTROL_DB;
  try {
    const rows = (await control.prepare(`SELECT id,name FROM "user" WHERE id IN (${ids.map(() => "?").join(",")})`).bind(...ids).all<{ id: string; name: string }>()).results;
    return new Map(rows.map((row) => [row.id, row.name]));
  } catch (error) {
    // The minimal integration control database does not install Better Auth's
    // user table. Production does; preserve all other database failures.
    if ((error as { code?: string })?.code === "42P01") return new Map<string, string>();
    throw error;
  }
}
function identityAccessScope(alias: "a" | "i", actor: WorkContext) {
  if (actor.departmentId) return {
    sql: `AND (${alias}.scope_kind='holding' OR (${alias}.scope_kind='branch' AND ${alias}.scope_node_id=?) OR (${alias}.scope_kind='department' AND ${alias}.scope_node_id=?))`,
    values: [actor.branchId, actor.departmentId],
  };
  if (actor.branchId) return {
    sql: `AND (${alias}.scope_kind='holding' OR (${alias}.scope_kind='branch' AND ${alias}.scope_node_id=?) OR (${alias}.scope_kind='department' AND ${alias}.scope_node_id IN (SELECT id FROM organization_nodes WHERE company_id=${alias}.company_id AND branch_id=?)))`,
    values: [actor.branchId, actor.branchId],
  };
  return { sql: "", values: [] as string[] };
}
async function requireIdentityAccessEvidence(db: SqlDatabase, actor: WorkContext, metric: typeof activeAssignmentsMetric | typeof pendingAccessReviewsMetric) {
  await currentAuthority(db, actor);
  const catalog = await metricCatalogFor(db, actor), permission = metric === activeAssignmentsMetric ? "iam.read" : "iam.review";
  if (!hasPermission(actor, permission) || !catalog.some((item) => item.id === metric && item.available))
    throw new ApiError(403, "EVIDENCE_FORBIDDEN", "This source record is not available in the current role or scope.");
  await assertProductionModuleEnabled(db, actor.companyId, "identity_access");
}
const activeAssignmentEvidenceSource = `SELECT a.id,a.user_id,a.role,COALESCE(r.name,replace(a.role,'_',' ')) role_name,a.scope_kind,a.scope_node_id,
  CASE WHEN a.scope_kind='holding' THEN 'Company-wide' ELSE COALESCE(n.display_name,a.scope_node_id,'Unavailable scope') END scope_name,
  CASE WHEN a.scope_kind='holding' THEN 'Company-wide' WHEN a.scope_kind='branch' THEN COALESCE(n.display_name,a.scope_node_id,'Unavailable branch') ELSE COALESCE(b.display_name,n.display_name,a.scope_node_id,'Unavailable branch') END branch_name,
  a.source,a.valid_from,a.valid_until,a.version FROM assignments a
  LEFT JOIN LATERAL (SELECT r.name FROM iam_roles r WHERE r.id=a.role AND r.company_id IN ('*',a.company_id) AND r.status='active' ORDER BY (r.company_id=a.company_id) DESC LIMIT 1) r ON TRUE
  LEFT JOIN organization_nodes n ON n.id=a.scope_node_id AND n.company_id=a.company_id AND n.status='active'
  LEFT JOIN organization_nodes b ON b.id=CASE WHEN a.scope_kind='branch' THEN a.scope_node_id ELSE n.branch_id END AND b.company_id=a.company_id AND b.status='active'
  WHERE a.company_id=? AND a.revoked_at IS NULL AND a.valid_from<=? AND a.valid_until>? AND iam_assignment_active(a.id,?)`;
async function searchActiveAssignmentEvidence(db: SqlDatabase, actor: WorkContext, input: z.infer<typeof activeAssignmentsQuerySchema>) {
  await requireIdentityAccessEvidence(db, actor, activeAssignmentsMetric);
  const cursor = identityAccessCursor(input.cursor), scope = identityAccessScope("a", actor), conditions: string[] = [];
  if (cursor && cursor.userId === undefined) throw new ApiError(422, "EVIDENCE_CURSOR_INVALID", "The source record cursor is invalid. Refresh the evidence list.");
  const now = Date.now(), values: unknown[] = [actor.companyId, now, now, now, ...scope.values];
  if (input.query) {
    conditions.push("position(lower(?) in lower(concat_ws(' ',a.user_id,a.role,COALESCE(r.name,''),a.scope_kind,COALESCE(n.display_name,''),COALESCE(b.display_name,''),a.source)))>0");
    values.push(input.query);
  }
  if (cursor) {
    conditions.push("(a.user_id>? OR (a.user_id=? AND (a.role>? OR (a.role=? AND a.id>?))))");
    values.push(cursor.userId, cursor.userId, cursor.role, cursor.role, cursor.recordId);
  }
  const rows = (await db.prepare(`${activeAssignmentEvidenceSource}${scope.sql}${conditions.length ? ` AND ${conditions.join(" AND ")}` : ""}
    ORDER BY a.user_id,a.role,a.id LIMIT ?`).bind(...values, input.limit + 1).all<ActiveAssignmentEvidenceRow>()).results;
  const hasMore = rows.length > input.limit, items = rows.slice(0, input.limit), userNames = await identityAccessUserNames(items.map((row) => row.user_id));
  const queryHash = await sha256(JSON.stringify({ providerId: activeAssignmentsProviderId, metric: input.metric, filters: input.filters, query: input.query, cursor: input.cursor ?? null, scope: [actor.companyId, actor.assignmentId, actor.branchId, actor.departmentId] }));
  await currentAuthority(db, actor);
  await audit(db, actor, activeAssignmentsProviderId, input.metric, "search", items.map((row) => ({ id: row.id, version: organizationEvidenceNumber(row.version, "assignment version") })), activeAssignmentFields, queryHash);
  await currentAuthority(db, actor);
  return {
    provider: activeAssignmentsProviderId,
    metric: input.metric,
    fields: activeAssignmentFields,
    items: items.map((row) => ({
      id: encodeIdentityAccessRecordId(activeAssignmentsMetric, row.id),
      label: `${userNames.get(row.user_id) ?? row.user_id} · ${row.role_name}`,
      summary: `${row.scope_kind} · ${row.scope_name} · expires ${new Date(organizationEvidenceNumber(row.valid_until, "expiry time")).toISOString().slice(0, 10)}`,
      status: "active",
      href: "/people/settings?section=users",
    })),
    hasMore,
    nextCursor: hasMore && items.length ? Buffer.from(JSON.stringify({ userId: items.at(-1)!.user_id, role: items.at(-1)!.role, recordId: items.at(-1)!.id })).toString("base64url") : null,
    queryHash,
  };
}
async function readActiveAssignmentEvidence(db: SqlDatabase, actor: WorkContext, recordId: string, filters: Filters) {
  await requireIdentityAccessEvidence(db, actor, activeAssignmentsMetric);
  const id = decodeIdentityAccessRecordId(recordId, activeAssignmentsMetric), scope = identityAccessScope("a", actor), now = Date.now();
  const row = await db.prepare(`${activeAssignmentEvidenceSource}${scope.sql} AND a.id=? LIMIT 1`).bind(actor.companyId, now, now, now, ...scope.values, id).first<ActiveAssignmentEvidenceRow>();
  if (!row) throw new ApiError(404, "EVIDENCE_NOT_FOUND", "This source record is unavailable in the selected role, scope or filters.");
  const names = await identityAccessUserNames([row.user_id]);
  const record = {
    id: recordId,
    user_id: row.user_id,
    user_name: names.get(row.user_id) ?? null,
    role: row.role,
    role_name: row.role_name,
    scope_kind: row.scope_kind,
    scope_node_id: row.scope_node_id,
    scope_name: row.scope_name,
    branch_name: row.branch_name,
    source: row.source,
    valid_from: organizationEvidenceNumber(row.valid_from, "start time"),
    valid_until: organizationEvidenceNumber(row.valid_until, "expiry time"),
    version: organizationEvidenceNumber(row.version, "assignment version"),
    href: "/people/settings?section=users",
  };
  const queryHash = await sha256(JSON.stringify({ providerId: activeAssignmentsProviderId, metric: activeAssignmentsMetric, filters, scope: [actor.companyId, actor.assignmentId, actor.branchId, actor.departmentId], recordId }));
  await currentAuthority(db, actor);
  await audit(db, actor, activeAssignmentsProviderId, activeAssignmentsMetric, "open", [{ id: row.id, version: record.version }], activeAssignmentFields, queryHash);
  await currentAuthority(db, actor);
  return { provider: activeAssignmentsProviderId, metric: activeAssignmentsMetric, fields: activeAssignmentFields, record, queryHash };
}
const pendingAccessReviewEvidenceSource = `SELECT i.id,i.campaign_id,i.assignment_id,i.assignment_version,i.user_id,i.role,i.scope_kind,i.scope_node_id,
  CASE WHEN i.scope_kind='holding' THEN 'Company-wide' ELSE COALESCE(n.display_name,i.scope_node_id,'Unavailable scope') END scope_name,
  CASE WHEN i.scope_kind='holding' THEN 'Company-wide' WHEN i.scope_kind='branch' THEN COALESCE(n.display_name,i.scope_node_id,'Unavailable branch') ELSE COALESCE(b.display_name,n.display_name,i.scope_node_id,'Unavailable branch') END branch_name,
  i.source,i.valid_until,i.decision,c.name campaign_name,c.status campaign_status,c.due_at campaign_due_at,c.version campaign_version,c.created_at campaign_created_at
  FROM iam_access_review_items i JOIN iam_access_review_campaigns c ON c.id=i.campaign_id AND c.company_id=i.company_id
  LEFT JOIN organization_nodes n ON n.id=i.scope_node_id AND n.company_id=i.company_id AND n.status='active'
  LEFT JOIN organization_nodes b ON b.id=CASE WHEN i.scope_kind='branch' THEN i.scope_node_id ELSE n.branch_id END AND b.company_id=i.company_id AND b.status='active'
  WHERE i.company_id=? AND i.decision='pending' AND c.status='open'`;
async function searchPendingAccessReviewEvidence(db: SqlDatabase, actor: WorkContext, input: z.infer<typeof pendingAccessReviewsQuerySchema>) {
  await requireIdentityAccessEvidence(db, actor, pendingAccessReviewsMetric);
  const cursor = identityAccessCursor(input.cursor), scope = identityAccessScope("i", actor), conditions: string[] = [];
  if (cursor && cursor.createdAt === undefined) throw new ApiError(422, "EVIDENCE_CURSOR_INVALID", "The source record cursor is invalid. Refresh the evidence list.");
  const values: unknown[] = [actor.companyId, ...scope.values];
  if (input.query) {
    conditions.push("position(lower(?) in lower(concat_ws(' ',c.name,i.user_id,i.role,i.source,i.scope_kind,COALESCE(n.display_name,''),COALESCE(b.display_name,''))))>0");
    values.push(input.query);
  }
  if (cursor) {
    conditions.push("(c.created_at<? OR (c.created_at=? AND i.id>?))");
    values.push(cursor.createdAt, cursor.createdAt, cursor.recordId);
  }
  const rows = (await db.prepare(`${pendingAccessReviewEvidenceSource}${scope.sql}${conditions.length ? ` AND ${conditions.join(" AND ")}` : ""}
    ORDER BY c.created_at DESC,i.id ASC LIMIT ?`).bind(...values, input.limit + 1).all<PendingAccessReviewEvidenceRow>()).results;
  const hasMore = rows.length > input.limit, items = rows.slice(0, input.limit), userNames = await identityAccessUserNames(items.map((row) => row.user_id));
  const queryHash = await sha256(JSON.stringify({ providerId: pendingAccessReviewsProviderId, metric: input.metric, filters: input.filters, query: input.query, cursor: input.cursor ?? null, scope: [actor.companyId, actor.assignmentId, actor.branchId, actor.departmentId] }));
  await currentAuthority(db, actor);
  await audit(db, actor, pendingAccessReviewsProviderId, input.metric, "search", items.map((row) => ({ id: row.id, version: organizationEvidenceNumber(row.campaign_version, "campaign version") })), pendingAccessReviewFields, queryHash);
  await currentAuthority(db, actor);
  return {
    provider: pendingAccessReviewsProviderId,
    metric: input.metric,
    fields: pendingAccessReviewFields,
    items: items.map((row) => ({
      id: encodeIdentityAccessRecordId(pendingAccessReviewsMetric, row.id),
      label: `${userNames.get(row.user_id) ?? row.user_id} · ${row.role}`,
      summary: `${row.campaign_name} · ${row.scope_name} · due ${new Date(organizationEvidenceNumber(row.campaign_due_at, "campaign due time")).toISOString().slice(0, 10)}`,
      status: row.decision,
      href: `/people/settings?section=reviews#${encodeURIComponent(row.campaign_id)}`,
    })),
    hasMore,
    nextCursor: hasMore && items.length ? Buffer.from(JSON.stringify({ createdAt: organizationEvidenceNumber(items.at(-1)!.campaign_created_at, "campaign creation time"), recordId: items.at(-1)!.id })).toString("base64url") : null,
    queryHash,
  };
}
async function readPendingAccessReviewEvidence(db: SqlDatabase, actor: WorkContext, recordId: string, filters: Filters) {
  await requireIdentityAccessEvidence(db, actor, pendingAccessReviewsMetric);
  const id = decodeIdentityAccessRecordId(recordId, pendingAccessReviewsMetric), scope = identityAccessScope("i", actor);
  const row = await db.prepare(`${pendingAccessReviewEvidenceSource}${scope.sql} AND i.id=? LIMIT 1`).bind(actor.companyId, ...scope.values, id).first<PendingAccessReviewEvidenceRow>();
  if (!row) throw new ApiError(404, "EVIDENCE_NOT_FOUND", "This source record is unavailable in the selected role, scope or filters.");
  const names = await identityAccessUserNames([row.user_id]);
  const record = {
    id: recordId,
    campaign_id: row.campaign_id,
    campaign_name: row.campaign_name,
    campaign_status: row.campaign_status,
    campaign_due_at: organizationEvidenceNumber(row.campaign_due_at, "campaign due time"),
    campaign_version: organizationEvidenceNumber(row.campaign_version, "campaign version"),
    user_id: row.user_id,
    user_name: names.get(row.user_id) ?? null,
    role: row.role,
    scope_kind: row.scope_kind,
    scope_node_id: row.scope_node_id,
    scope_name: row.scope_name,
    branch_name: row.branch_name,
    source: row.source,
    valid_until: organizationEvidenceNumber(row.valid_until, "assignment expiry"),
    assignment_version: organizationEvidenceNumber(row.assignment_version, "source assignment version"),
    decision: row.decision,
    href: `/people/settings?section=reviews#${encodeURIComponent(row.campaign_id)}`,
  };
  const queryHash = await sha256(JSON.stringify({ providerId: pendingAccessReviewsProviderId, metric: pendingAccessReviewsMetric, filters, scope: [actor.companyId, actor.assignmentId, actor.branchId, actor.departmentId], recordId }));
  await currentAuthority(db, actor);
  await audit(db, actor, pendingAccessReviewsProviderId, pendingAccessReviewsMetric, "open", [{ id: row.id, version: record.campaign_version }], pendingAccessReviewFields, queryHash);
  await currentAuthority(db, actor);
  return { provider: pendingAccessReviewsProviderId, metric: pendingAccessReviewsMetric, fields: pendingAccessReviewFields, record, queryHash };
}

type CommercialSubscriptionRow = {
  source_kind: "enrollment" | "legacy";
  record_id: string;
  plan_name: string;
  status: string;
  started_at: number | string | null;
  effective_at: number | string | null;
  record_version: number | string;
};
type OverdueInvoiceEvidenceRow = {
  id: string;
  kind: "main" | "adjustment";
  status: "open" | "uncollectible";
  total_satang: string | number;
  due_at: string | number;
  updated_at: string | number;
  month: string;
};
function commercialCursor(cursor?: string) {
  if (!cursor) return null;
  try {
    const value = JSON.parse(Buffer.from(cursor, "base64url").toString("utf8")) as { sourceKind?: unknown; recordId?: unknown };
    const sourceKind = z.enum(["enrollment", "legacy"]).parse(value.sourceKind);
    const recordId = z.string().min(1).max(200).parse(value.recordId);
    return { sourceKind, recordId };
  } catch {
    throw new ApiError(422, "EVIDENCE_CURSOR_INVALID", "The source record cursor is invalid. Refresh the evidence list.");
  }
}
function encodeCommercialSubscriptionId(sourceKind: CommercialSubscriptionRow["source_kind"], recordId: string) {
  return Buffer.from(JSON.stringify({ sourceKind, recordId })).toString("base64url");
}
function decodeCommercialSubscriptionId(recordId: string) {
  try {
    const value = JSON.parse(Buffer.from(recordId, "base64url").toString("utf8")) as { sourceKind?: unknown; recordId?: unknown };
    return { sourceKind: z.enum(["enrollment", "legacy"]).parse(value.sourceKind), recordId: z.string().min(1).max(200).parse(value.recordId) };
  } catch {
    throw new ApiError(404, "EVIDENCE_NOT_FOUND", "This source record is unavailable in the selected role, scope or filters.");
  }
}
function encodeOverdueInvoiceId(id: string) {
  return Buffer.from(JSON.stringify({ metric: overdueInvoicesMetric, id })).toString("base64url");
}
function decodeOverdueInvoiceId(recordId: string) {
  try {
    const value = JSON.parse(Buffer.from(recordId, "base64url").toString("utf8")) as { metric?: unknown; id?: unknown };
    if (value.metric !== overdueInvoicesMetric) throw new Error();
    return z.string().min(1).max(200).parse(value.id);
  } catch {
    throw new ApiError(404, "EVIDENCE_NOT_FOUND", "This source record is unavailable in the selected role, scope or filters.");
  }
}
async function requireCommercialEvidence(db: SqlDatabase, actor: WorkContext, metric: typeof activeSubscriptionsMetric | typeof overdueInvoicesMetric) {
  await currentAuthority(db, actor);
  const catalog = await metricCatalogFor(db, actor);
  if (actor.role !== "holding_owner" || actor.assignmentScope !== "holding" || !actor.holdingId || !hasPermission(actor, "company.owner")
    || !catalog.some((item) => item.id === metric && item.available))
    throw new ApiError(403, "EVIDENCE_FORBIDDEN", "This source record is not available in the current role or scope.");
  await assertProductionModuleEnabled(db, actor.companyId, "commercial");
}
function commercialSubscriptionSource() {
  return `WITH subscriptions AS (
      SELECT 'enrollment'::text source_kind,e.id record_id,COALESCE(p.name,e.plan_id) plan_name,
        CASE WHEN e.pending_end THEN 'ending' ELSE 'active' END status,e.started_at,
        CASE WHEN e.pending_end THEN e.effective_at ELSE NULL END effective_at,a.version record_version
      FROM billing_accounts a JOIN billing_enrollments e ON e.holding_id=a.holding_id
      LEFT JOIN LATERAL (SELECT name FROM billing_plans WHERE id=e.plan_id ORDER BY revision DESC LIMIT 1) p ON TRUE
      WHERE a.holding_id=? AND e.started_at<=? AND e.ended_at IS NULL AND (e.pending_end=FALSE OR e.effective_at IS NULL OR e.effective_at>?)
      UNION ALL
      SELECT 'legacy'::text source_kind,b.holding_id record_id,'Legacy subscription'::text plan_name,b.status,
        NULL::bigint started_at,b.current_period_end effective_at,b.updated_at record_version
      FROM beta_billing b WHERE b.holding_id=? AND b.subscription_id IS NOT NULL AND b.status IN ('active','trialing')
        AND (b.current_period_end IS NULL OR b.current_period_end*1000>?)
    )`;
}
async function searchCommercialSubscriptions(db: SqlDatabase, actor: WorkContext, input: z.infer<typeof activeSubscriptionsQuerySchema>) {
  await requireCommercialEvidence(db, actor, activeSubscriptionsMetric);
  const sourceDb = (await runtimeEnv()).CONTROL_DB;
  const cursor = commercialCursor(input.cursor), now = Date.now(), values: unknown[] = [actor.holdingId, now, now, actor.holdingId, now], conditions: string[] = [];
  if (input.query) {
    conditions.push("position(lower(?) in lower(concat_ws(' ',plan_name,status,source_kind,started_at::text,effective_at::text)))>0");
    values.push(input.query);
  }
  if (cursor) {
    conditions.push("(source_kind>? OR (source_kind=? AND record_id>?))");
    values.push(cursor.sourceKind, cursor.sourceKind, cursor.recordId);
  }
  const rows = (await sourceDb.prepare(`${commercialSubscriptionSource()} SELECT source_kind,record_id,plan_name,status,started_at,effective_at,record_version FROM subscriptions
    ${conditions.length ? `WHERE ${conditions.join(" AND ")}` : ""} ORDER BY source_kind,record_id LIMIT ?`)
    .bind(...values, input.limit + 1).all<CommercialSubscriptionRow>()).results;
  const hasMore = rows.length > input.limit, items = rows.slice(0, input.limit);
  const queryHash = await sha256(JSON.stringify({ providerId: activeSubscriptionsProviderId, metric: input.metric, filters: input.filters, query: input.query, cursor: input.cursor ?? null, scope: [actor.holdingId, actor.companyId, actor.assignmentId] }));
  await currentAuthority(db, actor);
  const enrollmentRows = items.filter((row) => row.source_kind === "enrollment"), legacyRows = items.filter((row) => row.source_kind === "legacy");
  await audit(db, actor, activeSubscriptionsProviderId, input.metric, "search", enrollmentRows.map((row) => ({ id: row.record_id, version: organizationEvidenceNumber(row.record_version, "billing account version") })), activeSubscriptionFields, queryHash);
  await audit(db, actor, activeSubscriptionsProviderId, input.metric, "search", legacyRows.map((row) => ({ id: row.record_id, version: organizationEvidenceNumber(row.record_version, "legacy subscription update time") })), legacySubscriptionFields, queryHash);
  await currentAuthority(db, actor);
  return {
    provider: activeSubscriptionsProviderId,
    metric: input.metric,
    fields: [...new Set([...activeSubscriptionFields, ...legacySubscriptionFields])],
    items: items.map((row) => ({
      id: encodeCommercialSubscriptionId(row.source_kind, row.record_id),
      label: `${row.plan_name} · ${row.status}`,
      summary: row.source_kind === "legacy" ? "Legacy subscription snapshot" : `Started ${new Date(organizationEvidenceNumber(row.started_at!, "subscription start time")).toISOString().slice(0, 10)}${row.effective_at ? ` · Ends ${new Date(organizationEvidenceNumber(row.effective_at, "subscription effective time")).toISOString().slice(0, 10)}` : ""}`,
      status: row.status,
      href: `/settings?section=usage&subscription=${row.source_kind}`,
    })),
    hasMore,
    nextCursor: hasMore && items.length ? Buffer.from(JSON.stringify({ sourceKind: items.at(-1)!.source_kind, recordId: items.at(-1)!.record_id })).toString("base64url") : null,
    queryHash,
  };
}
async function readCommercialSubscription(db: SqlDatabase, actor: WorkContext, recordId: string, filters: Filters) {
  await requireCommercialEvidence(db, actor, activeSubscriptionsMetric);
  const sourceDb = (await runtimeEnv()).CONTROL_DB;
  const decoded = decodeCommercialSubscriptionId(recordId), now = Date.now();
  const row = decoded.sourceKind === "enrollment"
    ? await sourceDb.prepare(`${commercialSubscriptionSource()} SELECT source_kind,record_id,plan_name,status,started_at,effective_at,record_version FROM subscriptions
        WHERE source_kind='enrollment' AND record_id=? LIMIT 1`).bind(actor.holdingId, now, now, actor.holdingId, now, decoded.recordId).first<CommercialSubscriptionRow>()
    : await sourceDb.prepare(`${commercialSubscriptionSource()} SELECT source_kind,record_id,plan_name,status,started_at,effective_at,record_version FROM subscriptions
        WHERE source_kind='legacy' AND record_id=? LIMIT 1`).bind(actor.holdingId, now, now, actor.holdingId, now, decoded.recordId).first<CommercialSubscriptionRow>();
  if (!row) throw new ApiError(404, "EVIDENCE_NOT_FOUND", "This source record is unavailable in the selected role, scope or filters.");
  const fields = row.source_kind === "legacy" ? legacySubscriptionFields : activeSubscriptionFields;
  const record = {
    id: recordId,
    source: row.source_kind,
    plan_name: row.plan_name,
    status: row.status,
    started_at: row.started_at === null ? null : organizationEvidenceNumber(row.started_at, "subscription start time"),
    ends_at: row.effective_at === null ? null : organizationEvidenceNumber(row.effective_at, "subscription end time"),
    version: organizationEvidenceNumber(row.record_version, row.source_kind === "legacy" ? "legacy subscription update time" : "billing account version"),
    href: `/settings?section=usage&subscription=${row.source_kind}`,
  };
  const queryHash = await sha256(JSON.stringify({ providerId: activeSubscriptionsProviderId, metric: activeSubscriptionsMetric, filters, scope: [actor.holdingId, actor.companyId, actor.assignmentId], recordId }));
  await currentAuthority(db, actor);
  await audit(db, actor, activeSubscriptionsProviderId, activeSubscriptionsMetric, "open", [{ id: row.record_id, version: record.version }], fields, queryHash);
  await currentAuthority(db, actor);
  return { provider: activeSubscriptionsProviderId, metric: activeSubscriptionsMetric, fields, record, queryHash };
}
function overdueInvoiceSource() {
  return `SELECT i.id,i.kind,i.status,i.total_satang,i.due_at,i.updated_at,p.month
    FROM billing_invoices i JOIN billing_periods p ON p.id=i.period_id AND p.holding_id=i.holding_id
    WHERE i.holding_id=? AND i.status IN ('open','uncollectible') AND i.due_at>=? AND i.due_at<? AND i.due_at<?`;
}
async function searchOverdueInvoiceEvidence(db: SqlDatabase, actor: WorkContext, input: z.infer<typeof overdueInvoicesQuerySchema>) {
  await requireCommercialEvidence(db, actor, overdueInvoicesMetric);
  const sourceDb = (await runtimeEnv()).CONTROL_DB;
  const cursorValue = (() => {
    if (!input.cursor) return null;
    try {
      const value = JSON.parse(Buffer.from(input.cursor, "base64url").toString("utf8")) as { dueAt?: unknown; id?: unknown };
      return { dueAt: z.number().int().safe().parse(value.dueAt), id: z.string().min(1).max(200).parse(value.id) };
    } catch {
      throw new ApiError(422, "EVIDENCE_CURSOR_INVALID", "The source record cursor is invalid. Refresh the evidence list.");
    }
  })();
  const now = Date.now(), end = Date.parse(input.filters.to) + 86400000, values: unknown[] = [actor.holdingId, Date.parse(input.filters.from), end, now], conditions: string[] = [];
  if (input.query) {
    conditions.push("position(lower(?) in lower(concat_ws(' ',i.id,i.kind,i.status,p.month)))>0");
    values.push(input.query);
  }
  if (cursorValue) {
    conditions.push("(i.due_at>? OR (i.due_at=? AND i.id>?))");
    values.push(cursorValue.dueAt, cursorValue.dueAt, cursorValue.id);
  }
  const rows = (await sourceDb.prepare(`${overdueInvoiceSource()}${conditions.length ? ` AND ${conditions.join(" AND ")}` : ""}
    ORDER BY i.due_at,i.id LIMIT ?`).bind(...values, input.limit + 1).all<OverdueInvoiceEvidenceRow>()).results;
  const hasMore = rows.length > input.limit, items = rows.slice(0, input.limit);
  const queryHash = await sha256(JSON.stringify({ providerId: overdueInvoicesProviderId, metric: input.metric, filters: input.filters, query: input.query, cursor: input.cursor ?? null, scope: [actor.holdingId, actor.companyId, actor.assignmentId] }));
  await currentAuthority(db, actor);
  await audit(db, actor, overdueInvoicesProviderId, input.metric, "search", items.map((row) => ({ id: row.id, version: organizationEvidenceNumber(row.updated_at, "invoice update time") })), overdueInvoiceFields, queryHash);
  await currentAuthority(db, actor);
  return {
    provider: overdueInvoicesProviderId,
    metric: input.metric,
    fields: overdueInvoiceFields,
    items: items.map((row) => ({
      id: encodeOverdueInvoiceId(row.id),
      label: `${row.kind === "main" ? "Main invoice" : "Adjustment invoice"} · ${row.status}`,
      summary: `Due ${new Date(organizationEvidenceNumber(row.due_at, "invoice due time")).toISOString().slice(0, 10)} · Service period ${row.month}`,
      amount: String(row.total_satang),
      currency: "THB",
      status: row.status,
      href: `/settings?section=usage&invoice=${encodeURIComponent(row.id)}`,
    })),
    hasMore,
    nextCursor: hasMore && items.length ? Buffer.from(JSON.stringify({ dueAt: organizationEvidenceNumber(items.at(-1)!.due_at, "invoice due time"), id: items.at(-1)!.id })).toString("base64url") : null,
    queryHash,
  };
}
async function readOverdueInvoiceEvidence(db: SqlDatabase, actor: WorkContext, recordId: string, filters: Filters) {
  await requireCommercialEvidence(db, actor, overdueInvoicesMetric);
  const sourceDb = (await runtimeEnv()).CONTROL_DB;
  const id = decodeOverdueInvoiceId(recordId), now = Date.now();
  const row = await sourceDb.prepare(`${overdueInvoiceSource()} AND i.id=? LIMIT 1`)
    .bind(actor.holdingId, Date.parse(filters.from), Date.parse(filters.to) + 86400000, now, id).first<OverdueInvoiceEvidenceRow>();
  if (!row) throw new ApiError(404, "EVIDENCE_NOT_FOUND", "This source record is unavailable in the selected role, scope or filters.");
  const record = {
    id: recordId,
    kind: row.kind,
    status: row.status,
    due_at: organizationEvidenceNumber(row.due_at, "invoice due time"),
    total_satang: String(row.total_satang),
    updated_at: organizationEvidenceNumber(row.updated_at, "invoice update time"),
    month: row.month,
    href: `/settings?section=usage&invoice=${encodeURIComponent(row.id)}`,
  };
  const queryHash = await sha256(JSON.stringify({ providerId: overdueInvoicesProviderId, metric: overdueInvoicesMetric, filters, scope: [actor.holdingId, actor.companyId, actor.assignmentId], recordId }));
  await currentAuthority(db, actor);
  await audit(db, actor, overdueInvoicesProviderId, overdueInvoicesMetric, "open", [{ id: row.id, version: record.updated_at }], overdueInvoiceFields, queryHash);
  await currentAuthority(db, actor);
  return { provider: overdueInvoicesProviderId, metric: overdueInvoicesMetric, fields: overdueInvoiceFields, record, queryHash };
}

type CloseBlockerEvidenceRow = {
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
function closeBlockerCursorValue(cursor?: string) {
  if (!cursor) return null;
  try {
    const value = JSON.parse(Buffer.from(cursor, "base64url").toString("utf8")) as { endDate?: unknown; checkKey?: unknown; id?: unknown };
    return {
      endDate: z.string().date().parse(value.endDate),
      checkKey: z.string().min(1).max(200).parse(value.checkKey),
      id: z.string().min(1).max(200).parse(value.id),
    };
  } catch {
    throw new ApiError(422, "EVIDENCE_CURSOR_INVALID", "The source record cursor is invalid. Refresh the evidence list.");
  }
}
function encodeCloseBlockerCursor(row: Pick<CloseBlockerEvidenceRow, "end_date" | "check_key" | "id">) {
  return Buffer.from(JSON.stringify({ endDate: row.end_date, checkKey: row.check_key, id: row.id })).toString("base64url");
}
function encodeCloseBlockerRecordId(id: string) {
  return Buffer.from(JSON.stringify({ checkId: id })).toString("base64url");
}
function decodeCloseBlockerRecordId(id: string) {
  try {
    const value = JSON.parse(Buffer.from(id, "base64url").toString("utf8")) as { checkId?: unknown };
    return z.string().min(1).max(200).parse(value.checkId);
  } catch {
    throw new ApiError(404, "EVIDENCE_NOT_FOUND", "This source record is unavailable in the selected role, scope or filters.");
  }
}
function closeBlockerNumber(value: number | string, field: string) {
  const parsed = Number(value);
  if (!Number.isSafeInteger(parsed)) throw new ApiError(500, "EVIDENCE_VALUE_INVALID", `The source returned an invalid ${field}.`);
  return parsed;
}
function closeBlockerCount(value: string | number) {
  const parsed = String(value);
  if (!/^\d+$/.test(parsed)) throw new ApiError(500, "EVIDENCE_VALUE_INVALID", "The source returned an invalid blocker count.");
  return parsed;
}
function closeBlockerSource() {
  return `WITH current_checks AS (
      SELECT DISTINCT ON (c.period_id,c.check_key) c.id,c.period_id,c.check_key,c.source_version,c.status,c.count_value,c.checked_at,c.resolved_at,
        p.name period_name,p.end_date,p.state period_state
      FROM finance_close_checks c JOIN finance_periods p ON p.id=c.period_id AND p.company_id=c.company_id
      WHERE c.company_id=? AND p.company_id=? AND p.end_date BETWEEN ? AND ? AND p.state IN ('open','closing')
      ORDER BY c.period_id,c.check_key,c.checked_at DESC,c.id DESC
    ) SELECT latest.id,latest.period_id,latest.period_name,latest.end_date,latest.period_state,latest.check_key,latest.source_version,
      latest.status,latest.count_value,latest.checked_at,latest.resolved_at FROM current_checks latest`;
}
async function requireCloseBlockerEvidence(db: SqlDatabase, actor: WorkContext) {
  await currentAuthority(db, actor);
  const catalog = await metricCatalogFor(db, actor);
  if (!hasPermission(actor, "finance.read") || actor.branchId || actor.departmentId || !catalog.some((item) => item.id === closeBlockersMetric && item.available))
    throw new ApiError(403, "EVIDENCE_FORBIDDEN", "This source record is not available in the current role or scope.");
  await assertProductionModuleEnabled(db, actor.companyId, "finance");
}
async function searchCloseBlockerEvidence(db: SqlDatabase, actor: WorkContext, input: z.infer<typeof closeBlockersQuerySchema>) {
  await requireCloseBlockerEvidence(db, actor);
  const cursor = closeBlockerCursorValue(input.cursor), conditions = ["latest.status='block'", "latest.resolved_at IS NULL"];
  const values: unknown[] = [actor.companyId, actor.companyId, input.filters.from, input.filters.to];
  if (input.query) {
    conditions.push("position(lower(?) in lower(concat_ws(' ',latest.id,latest.period_name,latest.end_date,latest.check_key,latest.status)))>0");
    values.push(input.query);
  }
  if (cursor) {
    conditions.push("(latest.end_date<? OR (latest.end_date=? AND (latest.check_key>? OR (latest.check_key=? AND latest.id>?))))");
    values.push(cursor.endDate, cursor.endDate, cursor.checkKey, cursor.checkKey, cursor.id);
  }
  const rows = (await db.prepare(`${closeBlockerSource()} WHERE ${conditions.join(" AND ")}
    ORDER BY latest.end_date DESC,latest.check_key ASC,latest.id ASC LIMIT ?`)
    .bind(...values, input.limit + 1).all<CloseBlockerEvidenceRow>()).results;
  const hasMore = rows.length > input.limit, items = rows.slice(0, input.limit);
  const queryHash = await sha256(JSON.stringify({ providerId: closeBlockersProviderId, metric: input.metric, filters: input.filters, query: input.query, cursor: input.cursor ?? null, scope: [actor.companyId, actor.assignmentId, actor.branchId, actor.departmentId] }));
  await currentAuthority(db, actor);
  // Close-check rows have no numeric version column; use a row-local version marker while source_version distinguishes check runs.
  await audit(db, actor, closeBlockersProviderId, input.metric, "search", items.map((row) => ({ id: row.id, version: 1 })), closeBlockersFields, queryHash);
  await currentAuthority(db, actor);
  return {
    provider: closeBlockersProviderId,
    metric: input.metric,
    fields: closeBlockersFields,
    items: items.map((row) => ({
      id: encodeCloseBlockerRecordId(row.id),
      label: `${row.period_name} · ${row.check_key}`,
      summary: `Block · ${closeBlockerCount(row.count_value)} current items · checked ${new Date(closeBlockerNumber(row.checked_at, "check time")).toISOString().slice(0, 10)}`,
      status: row.status,
      href: "/finance/close",
    })),
    hasMore,
    nextCursor: hasMore && items.length ? encodeCloseBlockerCursor(items.at(-1)!) : null,
    queryHash,
  };
}
async function readCloseBlockerEvidence(db: SqlDatabase, actor: WorkContext, recordId: string, filters: Filters) {
  await requireCloseBlockerEvidence(db, actor);
  const id = decodeCloseBlockerRecordId(recordId), row = await db.prepare(`${closeBlockerSource()}
    WHERE latest.id=? AND latest.status='block' AND latest.resolved_at IS NULL LIMIT 1`)
    .bind(actor.companyId, actor.companyId, filters.from, filters.to, id).first<CloseBlockerEvidenceRow>();
  if (!row) throw new ApiError(404, "EVIDENCE_NOT_FOUND", "This source record is unavailable in the selected role, scope or filters.");
  const record = {
    id: recordId,
    period_id: row.period_id,
    period_name: row.period_name,
    period_end_date: row.end_date,
    period_state: row.period_state,
    check_key: row.check_key,
    source_version: row.source_version,
    status: row.status,
    count_value: closeBlockerCount(row.count_value),
    checked_at: closeBlockerNumber(row.checked_at, "check time"),
    href: "/finance/close",
  };
  const queryHash = await sha256(JSON.stringify({ providerId: closeBlockersProviderId, metric: closeBlockersMetric, filters, scope: [actor.companyId, actor.assignmentId, actor.branchId, actor.departmentId], recordId }));
  await currentAuthority(db, actor);
  await audit(db, actor, closeBlockersProviderId, closeBlockersMetric, "open", [{ id: row.id, version: 1 }], closeBlockersFields, queryHash);
  await currentAuthority(db, actor);
  return { provider: closeBlockersProviderId, metric: closeBlockersMetric, fields: closeBlockersFields, record, queryHash };
}

type MaintenanceEvidenceRow = Awaited<ReturnType<typeof maintenanceDowntimeEvidenceRows>>[number];
function maintenanceCursorValue(cursor?: string) {
  if (!cursor) return null;
  try {
    const value = JSON.parse(Buffer.from(cursor, "base64url").toString("utf8")) as { endedAt?: unknown; id?: unknown };
    if (typeof value.endedAt !== "number" || !Number.isSafeInteger(value.endedAt) || typeof value.id !== "string") throw new Error();
    return { endedAt: value.endedAt, id: z.string().min(1).max(200).parse(value.id) };
  } catch {
    throw new ApiError(422, "EVIDENCE_CURSOR_INVALID", "The source record cursor is invalid. Refresh the evidence list.");
  }
}
function encodeMaintenanceCursor(row: MaintenanceEvidenceRow) {
  return Buffer.from(JSON.stringify({ endedAt: Number(row.downtime_ended_at), id: row.id })).toString("base64url");
}
function maintenanceEvidenceNumber(value: string, field: string) {
  const parsed = Number(value);
  if (!Number.isSafeInteger(parsed)) throw new ApiError(500, "EVIDENCE_VALUE_INVALID", `The source returned an invalid ${field}.`);
  return parsed;
}
type AvailableStockEvidenceRow = {
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
type InventoryShortageEvidenceRow = {
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
type InventoryMovementEvidenceRow = {
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
function inventoryMovementCursorValue(cursor?: string) {
  if (!cursor) return null;
  try {
    const value = JSON.parse(Buffer.from(cursor, "base64url").toString("utf8")) as { movementDate?: unknown; id?: unknown };
    return { movementDate: z.string().date().parse(value.movementDate), id: z.string().min(1).max(200).parse(value.id) };
  } catch {
    throw new ApiError(422, "EVIDENCE_CURSOR_INVALID", "The source record cursor is invalid. Refresh the evidence list.");
  }
}
function encodeInventoryMovementCursor(row: Pick<InventoryMovementEvidenceRow, "movement_date" | "id">) {
  return Buffer.from(JSON.stringify({ movementDate: row.movement_date, id: row.id })).toString("base64url");
}
function encodeInventoryMovementRecordId(row: Pick<InventoryMovementEvidenceRow, "id">) {
  return Buffer.from(JSON.stringify({ movementId: row.id })).toString("base64url");
}
function decodeInventoryMovementRecordId(id: string) {
  try {
    const value = JSON.parse(Buffer.from(id, "base64url").toString("utf8")) as { movementId?: unknown };
    return { movementId: z.string().min(1).max(200).parse(value.movementId) };
  } catch {
    throw new ApiError(404, "EVIDENCE_NOT_FOUND", "This source record is unavailable in the selected role, scope or filters.");
  }
}
function inventoryMovementHref(row: Pick<InventoryMovementEvidenceRow, "id" | "source_type" | "source_id">) {
  const sourceId = encodeURIComponent(row.source_id);
  if (row.source_type === "stock_count") return `/stock-counts/${sourceId}`;
  if (row.source_type === "transfer") return `/stock-transfers/${sourceId}`;
  if (row.source_type === "transit") return `/stock-transit/${sourceId}`;
  if (row.source_type.startsWith("pos_")) return `/pos?movement=${encodeURIComponent(row.id)}`;
  if (row.source_type === "pms") return `/pms/inventory?property=${sourceId}`;
  return `/commerce/orders/${sourceId}`;
}
function availableStockCursorValue(cursor?: string) {
  if (!cursor) return null;
  try {
    const value = JSON.parse(Buffer.from(cursor, "base64url").toString("utf8")) as { warehouseId?: unknown; productId?: unknown };
    return {
      warehouseId: z.string().min(1).max(200).parse(value.warehouseId),
      productId: z.string().min(1).max(200).parse(value.productId),
    };
  } catch {
    throw new ApiError(422, "EVIDENCE_CURSOR_INVALID", "The source record cursor is invalid. Refresh the evidence list.");
  }
}
function encodeAvailableStockCursor(row: AvailableStockEvidenceRow) {
  return Buffer.from(JSON.stringify({ warehouseId: row.warehouse_id, productId: row.product_id })).toString("base64url");
}
function encodeAvailableStockRecordId(row: Pick<AvailableStockEvidenceRow, "warehouse_id" | "product_id">) {
  return Buffer.from(JSON.stringify({ warehouseId: row.warehouse_id, productId: row.product_id })).toString("base64url");
}
function decodeAvailableStockRecordId(id: string) {
  try {
    const value = JSON.parse(Buffer.from(id, "base64url").toString("utf8")) as { warehouseId?: unknown; productId?: unknown };
    return {
      warehouseId: z.string().min(1).max(200).parse(value.warehouseId),
      productId: z.string().min(1).max(200).parse(value.productId),
    };
  } catch {
    throw new ApiError(404, "EVIDENCE_NOT_FOUND", "This source record is unavailable in the selected role, scope or filters.");
  }
}
function milliQuantity(value: string) {
  if (!/^-?\d+$/.test(value)) throw new ApiError(500, "EVIDENCE_VALUE_INVALID", "The source returned an invalid stock quantity.");
  const number = BigInt(value), negative = number < BigInt(0), absolute = negative ? -number : number;
  return `${negative ? "-" : ""}${absolute / BigInt(1000)}.${String(absolute % BigInt(1000)).padStart(3, "0")}`;
}
const availableStockPositions = `WITH reservations AS (
    SELECT warehouse_id,product_id,sum(quantity_milli::numeric) quantity_milli
    FROM inventory_active_reservations GROUP BY warehouse_id,product_id
  )
  SELECT w.id warehouse_id,p.id product_id,p.sku,p.name product_name,p.unit,w.name warehouse_name,
    s.quantity_milli::text on_hand_milli,COALESCE(r.quantity_milli,0)::text reserved_milli,
    inventory_quarantined_quantity(w.id,p.id)::text quarantined_milli,
    GREATEST(s.quantity_milli::numeric-GREATEST(COALESCE(r.quantity_milli,0),inventory_quarantined_quantity(w.id,p.id)::numeric),0)::text available_milli,
    s.version stock_version
  FROM commerce_stock s JOIN commerce_warehouses w ON w.id=s.warehouse_id AND w.company_id=? AND w.status='active'
  JOIN commerce_products p ON p.id=s.product_id AND p.company_id=w.company_id AND p.status='active'
  LEFT JOIN reservations r ON r.warehouse_id=s.warehouse_id AND r.product_id=s.product_id
  WHERE (?::text IS NULL OR w.branch_id=?)`;
const stockedLocationPositions = `WITH reservations AS (
    SELECT warehouse_id,product_id,sum(quantity_milli::numeric) quantity_milli
    FROM inventory_active_reservations GROUP BY warehouse_id,product_id
  )
  SELECT w.id warehouse_id,p.id product_id,p.sku,p.name product_name,p.unit,w.name warehouse_name,
    s.quantity_milli::text on_hand_milli,COALESCE(r.quantity_milli,0)::text reserved_milli,
    inventory_quarantined_quantity(w.id,p.id)::text quarantined_milli,
    GREATEST(s.quantity_milli::numeric-GREATEST(COALESCE(r.quantity_milli,0),inventory_quarantined_quantity(w.id,p.id)::numeric),0)::text available_milli,
    s.version stock_version
  FROM commerce_stock s JOIN commerce_warehouses w ON w.id=s.warehouse_id AND w.company_id=?
  JOIN commerce_products p ON p.id=s.product_id AND p.company_id=w.company_id
  LEFT JOIN reservations r ON r.warehouse_id=s.warehouse_id AND r.product_id=s.product_id
  WHERE s.quantity_milli>0`;
const inventoryShortagePositions = `WITH reservations AS (
    SELECT warehouse_id,product_id,sum(quantity_milli::numeric) quantity_milli
    FROM inventory_active_reservations GROUP BY warehouse_id,product_id
  )
  SELECT w.id warehouse_id,p.id product_id,p.sku,p.name product_name,p.unit,w.name warehouse_name,
    COALESCE(s.quantity_milli,0)::text on_hand_milli,COALESCE(r.quantity_milli,0)::text reserved_milli,
    inventory_quarantined_quantity(w.id,p.id)::text quarantined_milli,
    GREATEST(COALESCE(s.quantity_milli,0)::numeric-GREATEST(COALESCE(r.quantity_milli,0),inventory_quarantined_quantity(w.id,p.id)::numeric),0)::text available_milli,
    policy.current_reorder_point_milli::text approved_reorder_point_milli,policy.version policy_version,COALESCE(s.version,0) stock_version
  FROM inventory_reorder_policies policy
  JOIN commerce_warehouses w ON w.id=policy.warehouse_id AND w.company_id=policy.company_id AND w.status='active'
  JOIN commerce_products p ON p.id=policy.product_id AND p.company_id=policy.company_id AND p.status='active'
  LEFT JOIN commerce_stock s ON s.warehouse_id=w.id AND s.product_id=p.id
  LEFT JOIN reservations r ON r.warehouse_id=w.id AND r.product_id=p.id
  WHERE policy.company_id=? AND policy.current_reorder_point_milli IS NOT NULL AND policy.retired_at IS NULL
    AND (?::text IS NULL OR w.branch_id=?)`;
async function requireAvailableStockEvidence(db: SqlDatabase, actor: WorkContext) {
  await currentAuthority(db, actor);
  const catalog = await metricCatalogFor(db, actor);
  if (!hasPermission(actor, "commerce.read") || actor.departmentId || !catalog.some((item) => item.id === availableStockMetric && item.available))
    throw new ApiError(403, "EVIDENCE_FORBIDDEN", "This source record is not available in the current role or scope.");
  await assertProductionModuleEnabled(db, actor.companyId, "inventory");
}
async function searchAvailableStockEvidence(db: SqlDatabase, actor: WorkContext, input: z.infer<typeof availableStockQuerySchema>) {
  await requireAvailableStockEvidence(db, actor);
  const cursor = availableStockCursorValue(input.cursor), conditions = ["available_milli::numeric>0"], values: unknown[] = [actor.companyId, actor.branchId, actor.branchId];
  if (input.query) {
    conditions.push("(position(lower(?) in lower(sku))>0 OR position(lower(?) in lower(product_name))>0 OR position(lower(?) in lower(warehouse_name))>0)");
    values.push(input.query, input.query, input.query);
  }
  if (cursor) {
    conditions.push("(warehouse_id,product_id)>(?,?)");
    values.push(cursor.warehouseId, cursor.productId);
  }
  const rows = (await db.prepare(`SELECT * FROM (${availableStockPositions}) stock WHERE ${conditions.join(" AND ")} ORDER BY warehouse_id,product_id LIMIT ?`)
    .bind(...values, input.limit + 1).all<AvailableStockEvidenceRow>()).results;
  const hasMore = rows.length > input.limit, items = rows.slice(0, input.limit);
  const queryHash = await sha256(JSON.stringify({ providerId: availableStockProviderId, metric: input.metric, scope: [actor.companyId, actor.assignmentId, actor.branchId], query: input.query, cursor: input.cursor ?? null }));
  await currentAuthority(db, actor);
  await audit(db, actor, availableStockProviderId, input.metric, "search", items.map((row) => ({ id: encodeAvailableStockRecordId(row), version: Number(row.stock_version) })), availableStockFields, queryHash);
  await currentAuthority(db, actor);
  return {
    provider: availableStockProviderId,
    metric: input.metric,
    fields: availableStockFields,
    items: items.map((row) => ({
      id: encodeAvailableStockRecordId(row),
      label: `${row.sku} · ${row.product_name}`,
      summary: `${row.warehouse_name} · Available ${milliQuantity(row.available_milli)} ${row.unit}`,
      status: "available",
      version: Number(row.stock_version),
      href: "/commerce?view=stock",
    })),
    hasMore,
    nextCursor: hasMore && items.length ? encodeAvailableStockCursor(items.at(-1)!) : null,
    queryHash,
  };
}
async function readAvailableStockEvidence(db: SqlDatabase, actor: WorkContext, recordId: string) {
  await requireAvailableStockEvidence(db, actor);
  const ids = decodeAvailableStockRecordId(recordId);
  const row = await db.prepare(`SELECT * FROM (${availableStockPositions}) stock WHERE available_milli::numeric>0 AND warehouse_id=? AND product_id=?`)
    .bind(actor.companyId, actor.branchId, actor.branchId, ids.warehouseId, ids.productId).first<AvailableStockEvidenceRow>();
  if (!row) throw new ApiError(404, "EVIDENCE_NOT_FOUND", "This source record is unavailable in the selected role, scope or current state.");
  const record = {
    id: recordId, sku: row.sku, product_name: row.product_name, unit: row.unit, warehouse_name: row.warehouse_name,
    on_hand_milli: row.on_hand_milli, reserved_milli: row.reserved_milli, quarantined_milli: row.quarantined_milli,
    available_milli: row.available_milli, stock_version: Number(row.stock_version), href: "/commerce?view=stock",
  };
  const queryHash = await sha256(JSON.stringify({ providerId: availableStockProviderId, metric: availableStockMetric, scope: [actor.companyId, actor.assignmentId, actor.branchId], recordId }));
  await currentAuthority(db, actor);
  await audit(db, actor, availableStockProviderId, availableStockMetric, "open", [{ id: recordId, version: record.stock_version }], availableStockFields, queryHash);
  await currentAuthority(db, actor);
  return { provider: availableStockProviderId, metric: availableStockMetric, fields: availableStockFields, record, queryHash };
}
async function requireInventoryShortageEvidence(db: SqlDatabase, actor: WorkContext) {
  await currentAuthority(db, actor);
  const catalog = await metricCatalogFor(db, actor);
  if (!hasPermission(actor, "commerce.read") || actor.departmentId || !catalog.some((item) => item.id === inventoryShortagesMetric && item.available))
    throw new ApiError(403, "EVIDENCE_FORBIDDEN", "This source record is not available in the current role or scope.");
  await assertProductionModuleEnabled(db, actor.companyId, "inventory");
}
async function searchInventoryShortageEvidence(db: SqlDatabase, actor: WorkContext, input: z.infer<typeof inventoryShortagesQuerySchema>) {
  await requireInventoryShortageEvidence(db, actor);
  const cursor = availableStockCursorValue(input.cursor), conditions = ["available_milli::numeric<approved_reorder_point_milli::numeric"], values: unknown[] = [actor.companyId, actor.branchId, actor.branchId];
  if (input.query) {
    conditions.push("(position(lower(?) in lower(sku))>0 OR position(lower(?) in lower(product_name))>0 OR position(lower(?) in lower(warehouse_name))>0)");
    values.push(input.query, input.query, input.query);
  }
  if (cursor) { conditions.push("(warehouse_id,product_id)>(?,?)"); values.push(cursor.warehouseId, cursor.productId); }
  const rows = (await db.prepare(`SELECT * FROM (${inventoryShortagePositions}) shortages WHERE ${conditions.join(" AND ")} ORDER BY warehouse_id,product_id LIMIT ?`)
    .bind(...values, input.limit + 1).all<InventoryShortageEvidenceRow>()).results;
  const hasMore = rows.length > input.limit, items = rows.slice(0, input.limit);
  const queryHash = await sha256(JSON.stringify({ providerId: inventoryShortagesProviderId, metric: input.metric, filters: input.filters, scope: [actor.companyId, actor.assignmentId, actor.branchId], query: input.query, cursor: input.cursor ?? null }));
  await currentAuthority(db, actor);
  await audit(db, actor, inventoryShortagesProviderId, input.metric, "search", items.map((row) => ({ id: encodeAvailableStockRecordId(row), version: Number(row.policy_version) })), inventoryShortagesFields, queryHash);
  await currentAuthority(db, actor);
  return {
    provider: inventoryShortagesProviderId,
    metric: input.metric,
    fields: inventoryShortagesFields,
    items: items.map((row) => ({
      id: encodeAvailableStockRecordId(row),
      label: `${row.sku} · ${row.product_name}`,
      summary: `${row.warehouse_name} · Available ${milliQuantity(row.available_milli)} ${row.unit}; approved point ${milliQuantity(row.approved_reorder_point_milli)} ${row.unit}`,
      status: "shortage",
      version: Number(row.policy_version),
      href: "/commerce?view=stock",
    })),
    hasMore,
    nextCursor: hasMore && items.length ? encodeAvailableStockCursor(items.at(-1)!) : null,
    queryHash,
  };
}
async function readInventoryShortageEvidence(db: SqlDatabase, actor: WorkContext, recordId: string) {
  await requireInventoryShortageEvidence(db, actor);
  const ids = decodeAvailableStockRecordId(recordId);
  const row = await db.prepare(`SELECT * FROM (${inventoryShortagePositions}) shortages
    WHERE available_milli::numeric<approved_reorder_point_milli::numeric AND warehouse_id=? AND product_id=?`)
    .bind(actor.companyId, actor.branchId, actor.branchId, ids.warehouseId, ids.productId).first<InventoryShortageEvidenceRow>();
  if (!row) throw new ApiError(404, "EVIDENCE_NOT_FOUND", "This source record is unavailable in the selected role, scope or current state.");
  const record = {
    id: recordId, sku: row.sku, product_name: row.product_name, unit: row.unit, warehouse_name: row.warehouse_name,
    on_hand_milli: row.on_hand_milli, reserved_milli: row.reserved_milli, quarantined_milli: row.quarantined_milli,
    available_milli: row.available_milli, approved_reorder_point_milli: row.approved_reorder_point_milli,
    policy_version: Number(row.policy_version), stock_version: Number(row.stock_version), href: "/commerce?view=stock",
  };
  const queryHash = await sha256(JSON.stringify({ providerId: inventoryShortagesProviderId, metric: inventoryShortagesMetric, scope: [actor.companyId, actor.assignmentId, actor.branchId], recordId }));
  await currentAuthority(db, actor);
  await audit(db, actor, inventoryShortagesProviderId, inventoryShortagesMetric, "open", [{ id: recordId, version: record.policy_version }], inventoryShortagesFields, queryHash);
  await currentAuthority(db, actor);
  return { provider: inventoryShortagesProviderId, metric: inventoryShortagesMetric, fields: inventoryShortagesFields, record, queryHash };
}
async function requireStockedLocationEvidence(db: SqlDatabase, actor: WorkContext) {
  await currentAuthority(db, actor);
  const catalog = await metricCatalogFor(db, actor);
  if (!hasPermission(actor, "commerce.read") || actor.branchId || actor.departmentId || !catalog.some((item) => item.id === stockedLocationsMetric && item.available))
    throw new ApiError(403, "EVIDENCE_FORBIDDEN", "This source record is not available in the current role or scope.");
  await assertProductionModuleEnabled(db, actor.companyId, "inventory");
}
async function searchStockedLocationEvidence(db: SqlDatabase, actor: WorkContext, input: z.infer<typeof stockedLocationsQuerySchema>) {
  await requireStockedLocationEvidence(db, actor);
  const cursor = availableStockCursorValue(input.cursor), conditions = ["on_hand_milli::numeric>0"], values: unknown[] = [actor.companyId];
  if (input.query) {
    conditions.push("(position(lower(?) in lower(sku))>0 OR position(lower(?) in lower(product_name))>0 OR position(lower(?) in lower(warehouse_name))>0)");
    values.push(input.query, input.query, input.query);
  }
  if (cursor) {
    conditions.push("(warehouse_id,product_id)>(?,?)");
    values.push(cursor.warehouseId, cursor.productId);
  }
  const rows = (await db.prepare(`SELECT * FROM (${stockedLocationPositions}) stock WHERE ${conditions.join(" AND ")} ORDER BY warehouse_id,product_id LIMIT ?`)
    .bind(...values, input.limit + 1).all<AvailableStockEvidenceRow>()).results;
  const hasMore = rows.length > input.limit, items = rows.slice(0, input.limit);
  const queryHash = await sha256(JSON.stringify({ providerId: availableStockProviderId, metric: input.metric, scope: [actor.companyId, actor.assignmentId], query: input.query, cursor: input.cursor ?? null }));
  await currentAuthority(db, actor);
  await audit(db, actor, availableStockProviderId, input.metric, "search", items.map((row) => ({ id: encodeAvailableStockRecordId(row), version: Number(row.stock_version) })), availableStockFields, queryHash);
  await currentAuthority(db, actor);
  return {
    provider: availableStockProviderId,
    metric: input.metric,
    fields: availableStockFields,
    items: items.map((row) => ({
      id: encodeAvailableStockRecordId(row),
      label: `${row.sku} · ${row.product_name}`,
      summary: `${row.warehouse_name} · On hand ${milliQuantity(row.on_hand_milli)} ${row.unit}`,
      status: "stocked",
      version: Number(row.stock_version),
      href: "/commerce?view=stock",
    })),
    hasMore,
    nextCursor: hasMore && items.length ? encodeAvailableStockCursor(items.at(-1)!) : null,
    queryHash,
  };
}
async function readStockedLocationEvidence(db: SqlDatabase, actor: WorkContext, recordId: string) {
  await requireStockedLocationEvidence(db, actor);
  const ids = decodeAvailableStockRecordId(recordId);
  const row = await db.prepare(`SELECT * FROM (${stockedLocationPositions}) stock WHERE on_hand_milli::numeric>0 AND warehouse_id=? AND product_id=?`)
    .bind(actor.companyId, ids.warehouseId, ids.productId).first<AvailableStockEvidenceRow>();
  if (!row) throw new ApiError(404, "EVIDENCE_NOT_FOUND", "This source record is unavailable in the selected role, scope or current state.");
  const record = {
    id: recordId, sku: row.sku, product_name: row.product_name, unit: row.unit, warehouse_name: row.warehouse_name,
    on_hand_milli: row.on_hand_milli, reserved_milli: row.reserved_milli, quarantined_milli: row.quarantined_milli,
    available_milli: row.available_milli, stock_version: Number(row.stock_version), href: "/commerce?view=stock",
  };
  const queryHash = await sha256(JSON.stringify({ providerId: availableStockProviderId, metric: stockedLocationsMetric, scope: [actor.companyId, actor.assignmentId], recordId }));
  await currentAuthority(db, actor);
  await audit(db, actor, availableStockProviderId, stockedLocationsMetric, "open", [{ id: recordId, version: record.stock_version }], availableStockFields, queryHash);
  await currentAuthority(db, actor);
  return { provider: availableStockProviderId, metric: stockedLocationsMetric, fields: availableStockFields, record, queryHash };
}
async function requireInventoryMovementEvidence(db: SqlDatabase, actor: WorkContext) {
  await currentAuthority(db, actor);
  const catalog = await metricCatalogFor(db, actor);
  if (!hasPermission(actor, "commerce.read") || actor.departmentId || !catalog.some((item) => item.id === inventoryMovementsMetric && item.available))
    throw new ApiError(403, "EVIDENCE_FORBIDDEN", "This source record is not available in the current role or scope.");
  await assertProductionModuleEnabled(db, actor.companyId, "inventory");
}
async function searchInventoryMovementEvidence(db: SqlDatabase, actor: WorkContext, input: z.infer<typeof inventoryMovementsQuerySchema>) {
  await requireInventoryMovementEvidence(db, actor);
  const cursor = inventoryMovementCursorValue(input.cursor), conditions = ["w.company_id=?", "(?::text IS NULL OR w.branch_id=?)", "l.movement_date BETWEEN ? AND ?", "l.quantity_milli<>0"];
  const values: unknown[] = [actor.companyId, actor.branchId, actor.branchId, input.filters.from, input.filters.to];
  if (input.query) {
    conditions.push("(position(lower(?) in lower(p.sku))>0 OR position(lower(?) in lower(p.name))>0 OR position(lower(?) in lower(w.name))>0 OR position(lower(?) in lower(l.source_number))>0 OR position(lower(?) in lower(l.source_type))>0)");
    values.push(input.query, input.query, input.query, input.query, input.query);
  }
  if (cursor) {
    conditions.push("(l.movement_date,l.id)<(?,?)");
    values.push(cursor.movementDate, cursor.id);
  }
  const rows = (await db.prepare(`SELECT l.id,l.source_type,l.source_id,l.source_number,l.movement_date,l.quantity_milli::text quantity_milli,p.unit,p.sku,p.name product_name,w.id warehouse_id,w.name warehouse_name
    FROM inventory_ledger l JOIN commerce_warehouses w ON w.id=l.warehouse_id JOIN commerce_products p ON p.id=l.product_id AND p.company_id=w.company_id
    WHERE ${conditions.join(" AND ")} ORDER BY l.movement_date DESC,l.id DESC LIMIT ?`).bind(...values, input.limit + 1).all<InventoryMovementEvidenceRow>()).results;
  const hasMore = rows.length > input.limit, items = rows.slice(0, input.limit);
  const queryHash = await sha256(JSON.stringify({ providerId: inventoryMovementsProviderId, metric: input.metric, filters: input.filters, scope: [actor.companyId, actor.assignmentId, actor.branchId], query: input.query, cursor: input.cursor ?? null }));
  await currentAuthority(db, actor);
  // Ledger movement rows are append-only events and have no mutable version column; revision 1 identifies their sole event version.
  await audit(db, actor, inventoryMovementsProviderId, input.metric, "search", items.map((row) => ({ id: encodeInventoryMovementRecordId(row), version: 1 })), inventoryMovementFields, queryHash);
  await currentAuthority(db, actor);
  return {
    provider: inventoryMovementsProviderId,
    metric: input.metric,
    fields: inventoryMovementFields,
    items: items.map((row) => ({
      id: encodeInventoryMovementRecordId(row),
      label: `${row.sku} · ${row.product_name}`,
      summary: `${row.movement_date} · ${row.source_type.replaceAll("_", " ")} · ${milliQuantity(row.quantity_milli)} ${row.unit} · ${row.warehouse_name}`,
      status: row.source_type,
      href: inventoryMovementHref(row),
    })),
    hasMore,
    nextCursor: hasMore && items.length ? encodeInventoryMovementCursor(items.at(-1)!) : null,
    queryHash,
  };
}
async function readInventoryMovementEvidence(db: SqlDatabase, actor: WorkContext, recordId: string, filters: Filters) {
  await requireInventoryMovementEvidence(db, actor);
  const ids = decodeInventoryMovementRecordId(recordId);
  const row = await db.prepare(`SELECT l.id,l.source_type,l.source_id,l.source_number,l.movement_date,l.quantity_milli::text quantity_milli,p.unit,p.sku,p.name product_name,w.id warehouse_id,w.name warehouse_name
    FROM inventory_ledger l JOIN commerce_warehouses w ON w.id=l.warehouse_id JOIN commerce_products p ON p.id=l.product_id AND p.company_id=w.company_id
    WHERE w.company_id=? AND (?::text IS NULL OR w.branch_id=?) AND l.movement_date BETWEEN ? AND ? AND l.quantity_milli<>0 AND l.id=?`)
    .bind(actor.companyId, actor.branchId, actor.branchId, filters.from, filters.to, ids.movementId).first<InventoryMovementEvidenceRow>();
  if (!row) throw new ApiError(404, "EVIDENCE_NOT_FOUND", "This source record is unavailable in the selected role, scope or filters.");
  const record = {
    id: recordId, movement_date: row.movement_date, source_type: row.source_type, source_id: row.source_id, source_number: row.source_number,
    sku: row.sku, product_name: row.product_name, unit: row.unit, warehouse_name: row.warehouse_name, quantity_milli: row.quantity_milli,
    href: inventoryMovementHref(row),
  };
  const queryHash = await sha256(JSON.stringify({ providerId: inventoryMovementsProviderId, metric: inventoryMovementsMetric, filters, scope: [actor.companyId, actor.assignmentId, actor.branchId], recordId }));
  await currentAuthority(db, actor);
  await audit(db, actor, inventoryMovementsProviderId, inventoryMovementsMetric, "open", [{ id: recordId, version: 1 }], inventoryMovementFields, queryHash);
  await currentAuthority(db, actor);
  return { provider: inventoryMovementsProviderId, metric: inventoryMovementsMetric, fields: inventoryMovementFields, record, queryHash };
}
async function requireMaintenanceEvidence(db: SqlDatabase, actor: WorkContext) {
  await currentAuthority(db, actor);
  const catalog = await metricCatalogFor(db, actor);
  if (!catalog.some((item) => item.id === maintenanceDowntimeMetric && item.available))
    throw new ApiError(403, "EVIDENCE_FORBIDDEN", "This source record is not available in the current role or scope.");
  await assertProductionModuleEnabled(db, actor.companyId, "maintenance");
}
async function searchMaintenanceDowntimeEvidence(db: SqlDatabase, actor: WorkContext, input: z.infer<typeof maintenanceQuerySchema>) {
  await requireMaintenanceEvidence(db, actor);
  const cursor = maintenanceCursorValue(input.cursor), rows = await maintenanceDowntimeEvidenceRows(actor, {
    from: Date.parse(`${input.filters.from}T00:00:00.000Z`), to: Date.parse(input.filters.to) + 86400000,
    query: input.query, limit: input.limit, cursor: cursor ?? undefined,
  });
  const hasMore = rows.length > input.limit, items = rows.slice(0, input.limit);
  const queryHash = await sha256(JSON.stringify({ providerId: maintenanceProviderId, metric: input.metric, filters: input.filters, query: input.query, cursor: input.cursor ?? null }));
  await currentAuthority(db, actor);
  await audit(db, actor, maintenanceProviderId, input.metric, "search", items.map((row) => ({ id: row.id, version: maintenanceEvidenceNumber(row.version, "version") })), maintenanceSearchFields, queryHash);
  await currentAuthority(db, actor);
  return {
    provider: maintenanceProviderId,
    metric: input.metric,
    fields: maintenanceSearchFields,
    items: items.map((row) => ({
      id: row.id, title: row.title, status: row.status, asset_code: row.asset_code, asset_name: row.asset_name,
      downtime_started_at: maintenanceEvidenceNumber(row.downtime_started_at, "downtime start"),
      downtime_ended_at: maintenanceEvidenceNumber(row.downtime_ended_at, "downtime end"),
      version: maintenanceEvidenceNumber(row.version, "version"), href: `/maintenance/${encodeURIComponent(row.id)}`,
      label: row.title, summary: `${row.asset_code} · ${row.asset_name} · ${row.status}`,
    })),
    hasMore, nextCursor: hasMore && items.length ? encodeMaintenanceCursor(items.at(-1)!) : null, queryHash,
  };
}
async function readMaintenanceDowntimeEvidence(db: SqlDatabase, actor: WorkContext, recordId: string, filters: Filters) {
  await requireMaintenanceEvidence(db, actor);
  const [row] = await maintenanceDowntimeEvidenceRows(actor, {
    from: Date.parse(`${filters.from}T00:00:00.000Z`), to: Date.parse(filters.to) + 86400000,
    query: "", limit: 1, recordId,
  });
  if (!row) throw new ApiError(404, "EVIDENCE_NOT_FOUND", "This source record is unavailable in the selected role, scope or filters.");
  const startedAt = maintenanceEvidenceNumber(row.downtime_started_at, "downtime start"), endedAt = maintenanceEvidenceNumber(row.downtime_ended_at, "downtime end");
  const record = {
    id: row.id, title: row.title, status: row.status, asset_code: row.asset_code, asset_name: row.asset_name,
    downtime_started_at: startedAt, downtime_ended_at: endedAt, duration_ms: String(BigInt(endedAt) - BigInt(startedAt)),
    version: maintenanceEvidenceNumber(row.version, "version"), href: `/maintenance/${encodeURIComponent(row.id)}`,
  };
  const queryHash = await sha256(JSON.stringify({ providerId: maintenanceProviderId, metric: maintenanceDowntimeMetric, filters, recordId }));
  await currentAuthority(db, actor);
  await audit(db, actor, maintenanceProviderId, maintenanceDowntimeMetric, "open", [{ id: record.id, version: record.version }], maintenanceDetailFields, queryHash);
  await currentAuthority(db, actor);
  return { provider: maintenanceProviderId, metric: maintenanceDowntimeMetric, fields: maintenanceDetailFields, record, queryHash };
}

type ProjectMilestoneEvidenceRow = {
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

function projectMilestoneCursor(value?: string) {
  if (!value) return null;
  try {
    const decoded = JSON.parse(Buffer.from(value, "base64url").toString("utf8")) as { dueAt?: unknown; id?: unknown };
    if (typeof decoded.dueAt !== "number" || !Number.isSafeInteger(decoded.dueAt)) throw new Error();
    return { dueAt: decoded.dueAt, id: z.string().min(1).max(200).parse(decoded.id) };
  } catch {
    throw new ApiError(422, "EVIDENCE_CURSOR_INVALID", "The source record cursor is invalid. Refresh the evidence list.");
  }
}

async function requireProjectMilestoneEvidence(db: SqlDatabase, actor: WorkContext) {
  await currentAuthority(db, actor);
  const catalog = await metricCatalogFor(db, actor);
  if (!hasPermission(actor, "records.read") || !hasModuleAccess(actor.modules, "projects")
      || !catalog.some((item) => item.id === projectMilestoneMetric && item.available))
    throw new ApiError(403, "EVIDENCE_FORBIDDEN", "This source record is not available in the current role or scope.");
  await assertProductionModuleEnabled(db, actor.companyId, "projects");
}

async function searchProjectMilestoneEvidence(db: SqlDatabase, actor: WorkContext, raw: unknown) {
  const input = projectMilestoneQuerySchema.parse(raw);
  await requireProjectMilestoneEvidence(db, actor);
  const referenceAt = Date.now(), cohort = projectMilestoneOverdueCohort(actor, input.filters, referenceAt);
  const cursor = projectMilestoneCursor(input.cursor), conditions = [cohort.sql], values: unknown[] = [...cohort.values];
  if (input.query) {
    conditions.push("(position(lower(?) in lower(m.title))>0 OR position(lower(?) in lower(p.title))>0 OR position(lower(?) in lower(h.legal_name))>0 OR position(lower(?) in lower(m.state))>0)");
    values.push(input.query, input.query, input.query, input.query);
  }
  if (cursor) {
    conditions.push("(m.due_at,m.id)>(?,?)");
    values.push(cursor.dueAt, cursor.id);
  }
  const rows = (await db.prepare(`SELECT m.id,m.project_id,p.title project_title,m.title,m.due_at,m.owner_user_id,
      COALESCE(h.legal_name,m.owner_user_id) owner_name,m.state,m.version
    FROM staff_project_milestones m JOIN staff_projects p ON p.id=m.project_id AND p.company_id=m.company_id
    LEFT JOIN hr_people h ON h.company_id=m.company_id AND h.user_id=m.owner_user_id AND h.merged_into_id IS NULL
    WHERE ${conditions.join(" AND ")} ORDER BY m.due_at,m.id LIMIT ?`)
    .bind(...values,input.limit+1).all<ProjectMilestoneEvidenceRow>()).results;
  const hasMore=rows.length>input.limit,items=rows.slice(0,input.limit),fields=projectMilestoneFields;
  const queryHash=await sha256(JSON.stringify({providerId:projectMilestoneProviderId,metric:input.metric,filters:input.filters,query:input.query,cursor:input.cursor??null,scope:[actor.companyId,actor.assignmentId,actor.branchId,actor.departmentId,actor.userId],referenceAt}));
  await currentAuthority(db,actor);
  await audit(db,actor,projectMilestoneProviderId,projectMilestoneMetric,"search",items.map((row)=>({id:row.id,version:row.version})),fields,queryHash);
  await currentAuthority(db,actor);
  return {provider:projectMilestoneProviderId,metric:projectMilestoneMetric,fields,items:items.map((row)=>({
    id:row.id,label:row.title,summary:`${row.project_title} · Due ${new Date(row.due_at).toISOString().slice(0,10)} · ${row.owner_name}`,
    status:row.state,version:row.version,href:projectMilestoneHref(row.project_id),
  })),hasMore,nextCursor:hasMore&&items.length?Buffer.from(JSON.stringify({dueAt:items.at(-1)!.due_at,id:items.at(-1)!.id})).toString("base64url"):null,queryHash};
}

async function readProjectMilestoneEvidence(db: SqlDatabase, actor: WorkContext, recordId: string, filters: Filters) {
  await requireProjectMilestoneEvidence(db,actor);
  const id=z.string().min(1).max(200).parse(recordId),referenceAt=Date.now(),cohort=projectMilestoneOverdueCohort(actor,filters,referenceAt);
  const row=await db.prepare(`SELECT m.id,m.project_id,p.title project_title,m.title,m.due_at,m.owner_user_id,
      COALESCE(h.legal_name,m.owner_user_id) owner_name,m.state,m.version
    FROM staff_project_milestones m JOIN staff_projects p ON p.id=m.project_id AND p.company_id=m.company_id
    LEFT JOIN hr_people h ON h.company_id=m.company_id AND h.user_id=m.owner_user_id AND h.merged_into_id IS NULL
    WHERE ${cohort.sql} AND m.id=? LIMIT 1`).bind(...cohort.values,id).first<ProjectMilestoneEvidenceRow>();
  if(!row)throw new ApiError(404,"EVIDENCE_NOT_FOUND","This source record is unavailable in the selected role, scope or current state.");
  const record={id:row.id,project_id:row.project_id,project_title:row.project_title,title:row.title,due_at:row.due_at,owner_user_id:row.owner_user_id,owner_name:row.owner_name,state:row.state,version:row.version,href:projectMilestoneHref(row.project_id)};
  const queryHash=await sha256(JSON.stringify({providerId:projectMilestoneProviderId,metric:projectMilestoneMetric,filters,scope:[actor.companyId,actor.assignmentId,actor.branchId,actor.departmentId,actor.userId],recordId,referenceAt}));
  await currentAuthority(db,actor);
  await audit(db,actor,projectMilestoneProviderId,projectMilestoneMetric,"open",[{id:row.id,version:row.version}],projectMilestoneFields,queryHash);
  await currentAuthority(db,actor);
  return {provider:projectMilestoneProviderId,metric:projectMilestoneMetric,fields:projectMilestoneFields,record,queryHash};
}

type ProjectDependencyEvidenceRow = {
  id: string; project_id: string; project_title: string; title: string; due_at: number;
  state: "planned" | "in_progress"; version: number; blockers_json: string;
};
type ProjectDependencyEvidenceBlocker = { id: string; title: string; state: string; version: number };

function projectDependencyBlockers(value: string): ProjectDependencyEvidenceBlocker[] {
  try {
    const parsed = JSON.parse(value) as unknown;
    if (!Array.isArray(parsed)) return [];
    return parsed.filter((item): item is ProjectDependencyEvidenceBlocker => !!item && typeof item === "object"
      && typeof (item as ProjectDependencyEvidenceBlocker).id === "string"
      && typeof (item as ProjectDependencyEvidenceBlocker).title === "string"
      && typeof (item as ProjectDependencyEvidenceBlocker).state === "string"
      && Number.isSafeInteger((item as ProjectDependencyEvidenceBlocker).version));
  } catch { return []; }
}

async function requireProjectDependencyEvidence(db: SqlDatabase, actor: WorkContext) {
  await currentAuthority(db, actor);
  const catalog = await metricCatalogFor(db, actor);
  if (!hasPermission(actor, "records.read") || !hasModuleAccess(actor.modules, "projects")
      || !catalog.some((item) => item.id === projectDependencyMetric && item.available))
    throw new ApiError(403, "EVIDENCE_FORBIDDEN", "This source record is not available in the current role or scope.");
  await assertProductionModuleEnabled(db, actor.companyId, "projects");
}

async function searchProjectDependencyEvidence(db: SqlDatabase, actor: WorkContext, raw: unknown) {
  const input = projectDependencyQuerySchema.parse(raw);
  await requireProjectDependencyEvidence(db, actor);
  const cohort = projectDependencyBlockerCohort(actor, input.filters), cursor = projectMilestoneCursor(input.cursor);
  const conditions = [cohort.sql], values: unknown[] = [...cohort.values];
  if (input.query) {
    conditions.push(`(position(lower(?) in lower(m.title))>0 OR position(lower(?) in lower(p.title))>0 OR EXISTS(
      SELECT 1 FROM staff_project_milestone_dependencies search_dependency
      JOIN staff_project_milestones prerequisite ON prerequisite.id=search_dependency.predecessor_milestone_id
      WHERE search_dependency.company_id=p.company_id AND search_dependency.project_id=p.id
        AND search_dependency.successor_milestone_id=m.id AND prerequisite.state<>'completed'
        AND position(lower(?) in lower(prerequisite.title))>0))`);
    values.push(input.query, input.query, input.query);
  }
  if (cursor) { conditions.push("(m.due_at,m.id)>(?,?)"); values.push(cursor.dueAt, cursor.id); }
  const rows = (await db.prepare(`SELECT m.id,m.project_id,p.title project_title,m.title,m.due_at,m.state,m.version,
      COALESCE(blockers.items,'[]'::jsonb)::text blockers_json
    FROM staff_project_milestones m JOIN staff_projects p ON p.id=m.project_id AND p.company_id=m.company_id
    LEFT JOIN LATERAL (
      SELECT jsonb_agg(jsonb_build_object('id',pr.id,'title',pr.title,'state',pr.state,'version',pr.version) ORDER BY pr.due_at,pr.title) items
      FROM staff_project_milestone_dependencies dependency
      JOIN staff_project_milestones pr ON pr.id=dependency.predecessor_milestone_id AND pr.company_id=dependency.company_id AND pr.project_id=dependency.project_id
      WHERE dependency.company_id=p.company_id AND dependency.project_id=p.id AND dependency.successor_milestone_id=m.id AND pr.state<>'completed'
    ) blockers ON true
    WHERE ${conditions.join(" AND ")} ORDER BY m.due_at,m.id LIMIT ?`)
    .bind(...values, input.limit + 1).all<ProjectDependencyEvidenceRow>()).results;
  const hasMore = rows.length > input.limit, items = rows.slice(0, input.limit), fields = projectDependencyFields;
  const queryHash = await sha256(JSON.stringify({ providerId: projectDependencyProviderId, metric: projectDependencyMetric,
    filters: input.filters, query: input.query, cursor: input.cursor ?? null,
    scope: [actor.companyId, actor.assignmentId, actor.branchId, actor.departmentId, actor.userId],
    rows: items.map((row) => ({ id: row.id, version: row.version, blockers: projectDependencyBlockers(row.blockers_json) })) }));
  await currentAuthority(db, actor);
  await audit(db, actor, projectDependencyProviderId, projectDependencyMetric, "search", items.flatMap((row) => [
    { id: row.id, version: row.version }, ...projectDependencyBlockers(row.blockers_json).map((blocker) => ({ id: blocker.id, version: blocker.version })),
  ]), fields, queryHash);
  await currentAuthority(db, actor);
  return {
    provider: projectDependencyProviderId, metric: projectDependencyMetric, fields,
    items: items.map((row) => {
      const blockers = projectDependencyBlockers(row.blockers_json);
      return { id: row.id, label: row.title, summary: `${row.project_title} · ${blockers.length} incomplete prerequisite${blockers.length === 1 ? "" : "s"}`,
        status: row.state, version: row.version, href: `/holdings/${encodeURIComponent(actor.holdingSlug)}/projects/${encodeURIComponent(row.project_id)}` };
    }),
    hasMore,
    nextCursor: hasMore && items.length ? Buffer.from(JSON.stringify({ dueAt: items.at(-1)!.due_at, id: items.at(-1)!.id })).toString("base64url") : null,
    queryHash,
  };
}

async function readProjectDependencyEvidence(db: SqlDatabase, actor: WorkContext, recordId: string, filters: Filters) {
  await requireProjectDependencyEvidence(db, actor);
  const id = z.string().min(1).max(200).parse(recordId), cohort = projectDependencyBlockerCohort(actor, filters);
  const row = await db.prepare(`SELECT m.id,m.project_id,p.title project_title,m.title,m.due_at,m.state,m.version,
      COALESCE(blockers.items,'[]'::jsonb)::text blockers_json
    FROM staff_project_milestones m JOIN staff_projects p ON p.id=m.project_id AND p.company_id=m.company_id
    LEFT JOIN LATERAL (
      SELECT jsonb_agg(jsonb_build_object('id',pr.id,'title',pr.title,'state',pr.state,'version',pr.version) ORDER BY pr.due_at,pr.title) items
      FROM staff_project_milestone_dependencies dependency
      JOIN staff_project_milestones pr ON pr.id=dependency.predecessor_milestone_id AND pr.company_id=dependency.company_id AND pr.project_id=dependency.project_id
      WHERE dependency.company_id=p.company_id AND dependency.project_id=p.id AND dependency.successor_milestone_id=m.id AND pr.state<>'completed'
    ) blockers ON true
    WHERE ${cohort.sql} AND m.id=? LIMIT 1`)
    .bind(...cohort.values, id).first<ProjectDependencyEvidenceRow>();
  if (!row) throw new ApiError(404, "EVIDENCE_NOT_FOUND", "This source record is unavailable in the selected role, scope or current state.");
  const blockers = projectDependencyBlockers(row.blockers_json);
  const record = { id: row.id, project_id: row.project_id, project_title: row.project_title, title: row.title,
    due_at: row.due_at, state: row.state, version: row.version, prerequisites: blockers,
    href: `/holdings/${encodeURIComponent(actor.holdingSlug)}/projects/${encodeURIComponent(row.project_id)}` };
  const queryHash = await sha256(JSON.stringify({ providerId: projectDependencyProviderId, metric: projectDependencyMetric,
    filters, scope: [actor.companyId, actor.assignmentId, actor.branchId, actor.departmentId, actor.userId],
    record: { id: row.id, version: row.version, blockers } }));
  await currentAuthority(db, actor);
  await audit(db, actor, projectDependencyProviderId, projectDependencyMetric, "open", [
    { id: row.id, version: row.version }, ...blockers.map((blocker) => ({ id: blocker.id, version: blocker.version })),
  ], projectDependencyFields, queryHash);
  await currentAuthority(db, actor);
  return { provider: projectDependencyProviderId, metric: projectDependencyMetric, fields: projectDependencyFields, record, queryHash };
}

type ProjectActualEvidenceRow = {
  id: string; project_id: string; project_title: string; sort_date: string; state: string; version: number;
  work_date?: string; minutes?: number; user_id?: string;
  journal_id?: string; journal_number?: string; entry_date?: string; currency?: string;
  amount_minor?: string; account_code?: string; account_name?: string;
};

function projectActualCursor(value?: string) {
  if (!value) return null;
  try {
    const decoded = JSON.parse(Buffer.from(value, "base64url").toString("utf8")) as { date?: unknown; id?: unknown };
    return { date: z.string().date().parse(decoded.date), id: z.string().min(1).max(200).parse(decoded.id) };
  } catch {
    throw new ApiError(422, "EVIDENCE_CURSOR_INVALID", "The source record cursor is invalid. Refresh the evidence list.");
  }
}

function projectActualScope() {
  return `p.company_id=? AND (?::text IS NULL OR p.branch_id=?) AND (?::text IS NULL OR p.department_id=?)
    AND (iam_same_person(p.owner_user_id,?,p.company_id) OR EXISTS(
      SELECT 1 FROM staff_project_members member WHERE member.project_id=p.id AND iam_same_person(member.user_id,?,p.company_id)))`;
}

async function requireProjectActualEvidence(db: SqlDatabase, actor: WorkContext, metric: ProjectActualMetric) {
  await currentAuthority(db, actor);
  const permission = metric === "project-posted-cost" ? "finance.read" : "records.read";
  const catalog = await metricCatalogFor(db, actor);
  if (!hasPermission(actor, permission) || !hasModuleAccess(actor.modules, "projects")
      || !catalog.some((item) => item.id === metric && item.available))
    throw new ApiError(403, "EVIDENCE_FORBIDDEN", "This source record is not available in the current role or scope.");
  await assertProductionModuleEnabled(db, actor.companyId, "projects");
}

async function searchProjectActualEvidence(db: SqlDatabase, actor: WorkContext, raw: unknown) {
  const input = projectActualQuerySchema.parse(raw), metric = input.metric;
  await requireProjectActualEvidence(db, actor, metric);
  const cursor = projectActualCursor(input.cursor), scope = projectActualScope();
  const fields = metric === "project-actual-hours" ? projectActualHoursFields : projectPostedCostFields;
  const conditions = [scope], values: unknown[] = [actor.companyId, actor.branchId, actor.branchId,
    actor.departmentId, actor.departmentId, actor.userId, actor.userId];
  let sql: string;
  let idColumn: string;
  let dateColumn: string;
  if (metric === "project-actual-hours") {
    conditions.push("l.state='approved'", "l.work_date BETWEEN ? AND ?",
      "(iam_same_person(p.owner_user_id,?,p.company_id) OR iam_same_person(l.user_id,?,p.company_id))");
    values.push(input.filters.from, input.filters.to, actor.userId, actor.userId);
    if (input.query) {
      conditions.push("(position(lower(?) in lower(p.title))>0 OR position(lower(?) in lower(l.work_date::text))>0 OR position(lower(?) in lower(l.user_id))>0)");
      values.push(input.query, input.query, input.query);
    }
    dateColumn = "l.work_date::text";
    idColumn = "l.id";
    sql = `SELECT l.id,l.project_id,p.title project_title,l.work_date::text work_date,l.work_date::text sort_date,
        l.minutes,l.user_id,l.state,l.version
      FROM staff_project_work_logs l JOIN staff_projects p ON p.id=l.project_id AND p.company_id=l.company_id`;
  } else {
    conditions.push("a.state='approved'", "j.status='posted'", "ac.category IN ('expense','other_expense')",
      "j.currency=?", "j.entry_date BETWEEN ? AND ?");
    values.push(input.filters.currency, input.filters.from, input.filters.to);
    if (input.query) {
      conditions.push("(position(lower(?) in lower(p.title))>0 OR position(lower(?) in lower(j.journal_number))>0 OR position(lower(?) in lower(ac.code||' '||ac.name))>0)");
      values.push(input.query, input.query, input.query);
    }
    dateColumn = "j.entry_date::text";
    idColumn = "a.id";
    sql = `SELECT a.id,a.project_id,p.title project_title,j.entry_date::text entry_date,j.entry_date::text sort_date,
        j.id journal_id,j.journal_number,j.currency,ac.code account_code,ac.name account_name,
        a.amount_minor::text amount_minor,a.state,a.version
      FROM staff_project_cost_allocations a JOIN staff_projects p ON p.id=a.project_id AND p.company_id=a.company_id
      JOIN finance_journal_lines l ON l.id=a.journal_line_id JOIN finance_journals j ON j.id=l.journal_id
      JOIN finance_accounts ac ON ac.id=l.account_id`;
  }
  if (cursor) {
    conditions.push(`(${dateColumn},${idColumn})>(?,?)`);
    values.push(cursor.date, cursor.id);
  }
  const rows = (await db.prepare(`${sql} WHERE ${conditions.join(" AND ")} ORDER BY ${dateColumn},${idColumn} LIMIT ?`)
    .bind(...values, input.limit + 1).all<ProjectActualEvidenceRow>()).results;
  const hasMore = rows.length > input.limit, items = rows.slice(0, input.limit);
  const queryHash = await sha256(JSON.stringify({ providerId: projectActualProviderId, metric, filters: input.filters,
    query: input.query, cursor: input.cursor ?? null, scope: [actor.companyId, actor.assignmentId, actor.branchId, actor.departmentId, actor.userId] }));
  await currentAuthority(db, actor);
  await audit(db, actor, projectActualProviderId, metric, "search", items.map((row) => ({ id: row.id, version: row.version })), fields, queryHash);
  await currentAuthority(db, actor);
  return {
    provider: projectActualProviderId, metric, fields,
    items: items.map((row) => metric === "project-actual-hours" ? {
      id: row.id, label: `${row.work_date} · ${row.minutes} minutes`, summary: `${row.project_title} · Approved tracked time`,
      status: row.state, version: row.version, href: projectMilestoneHref(row.project_id),
    } : {
      id: row.id, label: String(row.journal_number), amount: row.amount_minor, currency: row.currency,
      summary: `${row.project_title} · ${row.entry_date} · ${row.account_code} ${row.account_name}`,
      status: row.state, version: row.version,
      href: `/holdings/${encodeURIComponent(actor.holdingSlug)}/finance/journals/${encodeURIComponent(row.journal_id!)}`,
    }),
    hasMore,
    nextCursor: hasMore && items.length ? Buffer.from(JSON.stringify({ date: items.at(-1)!.sort_date, id: items.at(-1)!.id })).toString("base64url") : null,
    queryHash,
  };
}

async function readProjectActualEvidence(db: SqlDatabase, actor: WorkContext, metric: ProjectActualMetric, recordId: string, filters: Filters) {
  await requireProjectActualEvidence(db, actor, metric);
  const id = z.string().min(1).max(200).parse(recordId), scope = projectActualScope();
  const fields = metric === "project-actual-hours" ? projectActualHoursFields : projectPostedCostFields;
  let row: ProjectActualEvidenceRow | null;
  if (metric === "project-actual-hours") {
    row = await db.prepare(`SELECT l.id,l.project_id,p.title project_title,l.work_date::text work_date,l.work_date::text sort_date,
        l.minutes,l.user_id,l.state,l.version
      FROM staff_project_work_logs l JOIN staff_projects p ON p.id=l.project_id AND p.company_id=l.company_id
      WHERE ${scope} AND l.id=? AND l.state='approved' AND l.work_date BETWEEN ? AND ?
        AND (iam_same_person(p.owner_user_id,?,p.company_id) OR iam_same_person(l.user_id,?,p.company_id)) LIMIT 1`)
      .bind(actor.companyId, actor.branchId, actor.branchId, actor.departmentId, actor.departmentId, actor.userId, actor.userId,
        id, filters.from, filters.to, actor.userId, actor.userId).first<ProjectActualEvidenceRow>();
  } else {
    row = await db.prepare(`SELECT a.id,a.project_id,p.title project_title,j.entry_date::text entry_date,j.entry_date::text sort_date,
        j.id journal_id,j.journal_number,j.currency,l.id journal_line_id,ac.code account_code,ac.name account_name,
        a.amount_minor::text amount_minor,a.state,a.version
      FROM staff_project_cost_allocations a JOIN staff_projects p ON p.id=a.project_id AND p.company_id=a.company_id
      JOIN finance_journal_lines l ON l.id=a.journal_line_id JOIN finance_journals j ON j.id=l.journal_id
      JOIN finance_accounts ac ON ac.id=l.account_id
      WHERE ${scope} AND a.id=? AND a.state='approved' AND j.status='posted' AND ac.category IN ('expense','other_expense')
        AND j.currency=? AND j.entry_date BETWEEN ? AND ? LIMIT 1`)
      .bind(actor.companyId, actor.branchId, actor.branchId, actor.departmentId, actor.departmentId, actor.userId, actor.userId,
        id, filters.currency, filters.from, filters.to).first<ProjectActualEvidenceRow>();
  }
  if (!row) throw new ApiError(404, "EVIDENCE_NOT_FOUND", "This source record is unavailable in the selected role, scope or filters.");
  const record = metric === "project-actual-hours" ? {
    id: row.id, project_id: row.project_id, project_title: row.project_title, work_date: row.work_date,
    minutes: row.minutes, user_id: row.user_id, state: row.state, version: row.version,
    href: projectMilestoneHref(row.project_id),
  } : {
    id: row.id, project_id: row.project_id, project_title: row.project_title, journal_id: row.journal_id,
    journal_number: row.journal_number, entry_date: row.entry_date, currency: row.currency,
    account_code: row.account_code, account_name: row.account_name, amount_minor: row.amount_minor,
    state: row.state, version: row.version,
    href: `/holdings/${encodeURIComponent(actor.holdingSlug)}/finance/journals/${encodeURIComponent(row.journal_id!)}`,
  };
  const queryHash = await sha256(JSON.stringify({ providerId: projectActualProviderId, metric, filters,
    scope: [actor.companyId, actor.assignmentId, actor.branchId, actor.departmentId, actor.userId], recordId: id }));
  await currentAuthority(db, actor);
  await audit(db, actor, projectActualProviderId, metric, "open", [{ id: row.id, version: row.version }], fields, queryHash);
  await currentAuthority(db, actor);
  return { provider: projectActualProviderId, metric, fields, record, queryHash };
}

type OperationsEvidenceRow = {
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

function operationsEvidenceFields(metric: OperationsMetricId) {
  return metric === "operations-escalations" ? operationsEscalationFields
    : metric === "exception-resolution-time" ? operationsResolutionFields : operationsFields;
}

function operationsRecordId(metric: OperationsMetricId, id: string) {
  return Buffer.from(JSON.stringify({ metric, id })).toString("base64url");
}

function decodeOperationsRecordId(value: string, metric: OperationsMetricId) {
  try {
    const decoded = JSON.parse(Buffer.from(value, "base64url").toString("utf8")) as { metric?: unknown; id?: unknown };
    if (decoded.metric !== metric || typeof decoded.id !== "string") throw new Error();
    return z.string().min(1).max(200).parse(decoded.id);
  } catch {
    throw new ApiError(422, "EVIDENCE_RECORD_INVALID", "The source record reference is invalid. Refresh the evidence list.");
  }
}

function operationsEvidenceCursor(value?: string, metric?: OperationsMetricId) {
  if (!value) return null;
  try {
    const decoded = JSON.parse(Buffer.from(value, "base64url").toString("utf8")) as { metric?: unknown; sortAt?: unknown; id?: unknown };
    if (decoded.metric !== metric || typeof decoded.sortAt !== "number" || !Number.isSafeInteger(decoded.sortAt)) throw new Error();
    return { sortAt: decoded.sortAt, id: z.string().min(1).max(200).parse(decoded.id) };
  } catch {
    throw new ApiError(422, "EVIDENCE_CURSOR_INVALID", "The source record cursor is invalid. Refresh the evidence list.");
  }
}

async function requireOperationsEvidence(db: SqlDatabase, actor: WorkContext, metric: OperationsMetricId) {
  await currentAuthority(db, actor);
  const catalog = await metricCatalogFor(db, actor);
  if (!hasPermission(actor, "records.read") || !catalog.some((item) => item.id === metric && item.available))
    throw new ApiError(403, "EVIDENCE_FORBIDDEN", "This source record is not available in the current role or scope.");
  await assertProductionModuleEnabled(db, actor.companyId, "operations");
}

async function searchOperationsEvidence(db: SqlDatabase, actor: WorkContext, raw: unknown) {
  const input = operationsQuerySchema.parse(raw), metric = input.metric;
  await requireOperationsEvidence(db, actor, metric);
  const referenceAt = Date.now(), visibility = operationsTaskVisibility(actor, referenceAt);
  const cohort = operationsTaskCohort(metric, "t", actor, input.filters, referenceAt);
  const sortColumn = metric === "exception-resolution-time" ? "updated_at" : "due_at";
  const cursor = operationsEvidenceCursor(input.cursor, metric);
  const conditions = [visibility.sql, cohort.sql];
  const values: unknown[] = [...visibility.values, ...cohort.values];
  if (input.query) {
    conditions.push("position(lower(?) in lower(t.title))>0");
    values.push(input.query);
  }
  if (cursor) {
    conditions.push(`(t.${sortColumn},t.id)<(?,?)`);
    values.push(cursor.sortAt, cursor.id);
  }
  const alertSelect = metric === "operations-escalations"
    ? `(SELECT min(n.created_at) FROM staff_notifications n WHERE n.company_id=t.company_id AND n.user_id=? AND n.record_type='staff_task_alert' AND n.record_id=t.id AND n.record_version=t.version AND n.id LIKE 'staff-task-alert-overdue-%')`
    : "NULL::bigint";
  // The recipient placeholder is in SELECT, before the WHERE placeholders.
  const queryValues = metric === "operations-escalations" ? [actor.userId, ...values] : values;
  const rows = (await db.prepare(`SELECT t.id,t.title,t.state,t.due_at,t.created_at,t.updated_at,t.version,t.branch_id,t.department_id,
      ${metric === "exception-resolution-time" ? "(t.updated_at-t.due_at)::text" : "NULL::text"} exception_duration_ms,
      ${alertSelect} alert_created_at,t.${sortColumn} sort_at
    FROM staff_tasks t WHERE ${conditions.join(" AND ")} ORDER BY t.${sortColumn} DESC,t.id DESC LIMIT ?`)
    .bind(...queryValues, input.limit + 1).all<OperationsEvidenceRow>()).results;
  const hasMore = rows.length > input.limit, items = rows.slice(0, input.limit), fields = operationsEvidenceFields(metric);
  const queryHash = await sha256(JSON.stringify({ providerId: operationsProviderId, metric, filters: input.filters, query: input.query, cursor: input.cursor ?? null, scope: [actor.companyId, actor.assignmentId, actor.branchId, actor.departmentId, actor.userId], referenceAt }));
  await currentAuthority(db, actor);
  await audit(db, actor, operationsProviderId, metric, "search", items.map((row) => ({ id: row.id, version: row.version })), fields, queryHash);
  await currentAuthority(db, actor);
  return {
    provider: operationsProviderId,
    metric,
    fields,
    items: items.map((row) => ({
      id: operationsRecordId(metric, row.id),
      label: row.title || "Operational task",
      summary: metric === "exception-resolution-time"
        ? `Completed ${new Date(row.updated_at).toISOString().slice(0, 10)} · ${row.exception_duration_ms} ms past due`
        : `${row.state} · ${row.due_at === null ? "No due date" : `due ${new Date(row.due_at).toISOString().slice(0, 10)}`}`,
      status: row.state,
      href: operationsTaskHref(row.id),
    })),
    hasMore,
    nextCursor: hasMore && items.length ? Buffer.from(JSON.stringify({ metric, sortAt: items.at(-1)!.sort_at, id: items.at(-1)!.id })).toString("base64url") : null,
    queryHash,
  };
}

async function readOperationsEvidence(db: SqlDatabase, actor: WorkContext, metric: OperationsMetricId, recordId: string, filters: Filters) {
  await requireOperationsEvidence(db, actor, metric);
  const taskId = decodeOperationsRecordId(recordId, metric), referenceAt = Date.now();
  const visibility = operationsTaskVisibility(actor, referenceAt), cohort = operationsTaskCohort(metric, "t", actor, filters, referenceAt);
  const alertSelect = metric === "operations-escalations"
    ? `(SELECT min(n.created_at) FROM staff_notifications n WHERE n.company_id=t.company_id AND n.user_id=? AND n.record_type='staff_task_alert' AND n.record_id=t.id AND n.record_version=t.version AND n.id LIKE 'staff-task-alert-overdue-%')`
    : "NULL::bigint";
  const queryValues = metric === "operations-escalations" ? [actor.userId, ...visibility.values, ...cohort.values, taskId] : [...visibility.values, ...cohort.values, taskId];
  const row = await db.prepare(`SELECT t.id,t.title,t.state,t.due_at,t.created_at,t.updated_at,t.version,t.branch_id,t.department_id,
      ${metric === "exception-resolution-time" ? "(t.updated_at-t.due_at)::text" : "NULL::text"} exception_duration_ms,
      ${alertSelect} alert_created_at
    FROM staff_tasks t WHERE ${visibility.sql} AND ${cohort.sql} AND t.id=? LIMIT 1`)
    .bind(...queryValues).first<OperationsEvidenceRow>();
  if (!row) throw new ApiError(404, "EVIDENCE_NOT_FOUND", "This source record is unavailable in the selected role, scope or filters.");
  const fields = operationsEvidenceFields(metric), record = {
    id: row.id, title: row.title, state: row.state, due_at: row.due_at, created_at: row.created_at,
    updated_at: row.updated_at, version: row.version, branch_id: row.branch_id, department_id: row.department_id,
    ...(metric === "exception-resolution-time" ? { exception_duration_ms: row.exception_duration_ms } : {}),
    ...(metric === "operations-escalations" ? { alert_created_at: row.alert_created_at } : {}),
    href: operationsTaskHref(row.id),
  };
  const queryHash = await sha256(JSON.stringify({ providerId: operationsProviderId, metric, filters, scope: [actor.companyId, actor.assignmentId, actor.branchId, actor.departmentId, actor.userId], recordId, referenceAt }));
  await currentAuthority(db, actor);
  await audit(db, actor, operationsProviderId, metric, "open", [{ id: row.id, version: row.version }], fields, queryHash);
  await currentAuthority(db, actor);
  return { provider: operationsProviderId, metric, fields, record, queryHash };
}

export async function searchExecutiveEvidence(db: SqlDatabase, actor: WorkContext, raw: unknown) {
  const common = z.object({ metric: z.enum([...financeMetrics, payrollSummaryMetric, approvedLeaveMetric, authorizedOvertimeMetric, hotelOccupancyMetric, ...hotelRoomRevenueMetrics, hotelFolioBalanceMetric, hotelNightAuditBlockersMetric, "invoice-match-exceptions", ...salesMetrics, salesFulfillmentMetric, fieldVisitMetric, manufacturingCostMetric, customerActivityMetric, socialActivityMetric, ...qualityMetrics, ...legalMetrics, closeBlockersMetric, identityChangeMetric, organizationNodeMetric, pendingStructureMetric, activeAssignmentsMetric, pendingAccessReviewsMetric, activeSubscriptionsMetric, overdueInvoicesMetric, maintenanceDowntimeMetric, duplicateReviewMetric, supportTicketMetrics[0], supportTicketMetrics[1], supportIncidentMetric, stockedLocationsMetric, availableStockMetric, inventoryShortagesMetric, inventoryMovementsMetric, projectMilestoneMetric, projectDependencyMetric, ...projectActualMetrics, ...operationsMetrics, ...agentEvidenceMetrics, ...crmEvidenceMetrics, ...brandEvidenceMetrics, ...learningEvidenceMetrics, ...talentEvidenceMetrics, ...posEvidenceMetrics, ...marketingEvidenceMetrics, ...planningEvidenceMetrics]), ...commonFilters }).strict().parse(raw);
  if (crmEvidenceMetrics.includes(common.metric as typeof crmEvidenceMetrics[number])) return searchCrmEvidence(db, actor, common);
  if (agentEvidenceMetrics.includes(common.metric as typeof agentEvidenceMetrics[number])) return searchAgentEvidence(db, actor, common);
  if (brandEvidenceMetrics.includes(common.metric as typeof brandEvidenceMetrics[number])) return searchBrandEvidence(db, actor, common);
  if (learningEvidenceMetrics.includes(common.metric as typeof learningEvidenceMetrics[number])) return searchLearningEvidence(db, actor, common);
  if (talentEvidenceMetrics.includes(common.metric as typeof talentEvidenceMetrics[number])) return searchTalentEvidence(db, actor, common);
  if (posEvidenceMetrics.includes(common.metric as typeof posEvidenceMetrics[number])) return searchPosEvidence(db, actor, common);
  if (marketingEvidenceMetrics.includes(common.metric as typeof marketingEvidenceMetrics[number])) return searchMarketingEvidence(db, actor, common);
  if (planningEvidenceMetrics.includes(common.metric as typeof planningEvidenceMetrics[number])) return searchPlanningEvidence(db, actor, common);
  if (common.metric === projectMilestoneMetric) return searchProjectMilestoneEvidence(db, actor, projectMilestoneQuerySchema.parse(common));
  if (common.metric === projectDependencyMetric) return searchProjectDependencyEvidence(db, actor, projectDependencyQuerySchema.parse(common));
  if (projectActualMetrics.includes(common.metric as ProjectActualMetric)) return searchProjectActualEvidence(db, actor, projectActualQuerySchema.parse(common));
  if (common.metric === payrollSummaryMetric) return searchPayrollSummaryEvidence(db, actor, payrollSummaryQuerySchema.parse(common));
  if (common.metric === approvedLeaveMetric || common.metric === authorizedOvertimeMetric) return searchPeopleLeaveOvertimeEvidence(db, actor, peopleLeaveOvertimeQuerySchema.parse(common));
  if (common.metric === hotelOccupancyMetric || hotelRoomRevenueMetrics.includes(common.metric as HotelRoomRevenueMetric)) return searchHotelOccupancyEvidence(db, actor, hotelOccupancyQuerySchema.parse(common));
  if (common.metric === hotelFolioBalanceMetric) return searchHotelFolioBalanceEvidence(db, actor, hotelFolioBalanceQuerySchema.parse(common));
  if (common.metric === hotelNightAuditBlockersMetric) return searchHotelNightAuditBlockerEvidence(db, actor, hotelNightAuditBlockersQuerySchema.parse(common));
  if (operationsMetrics.includes(common.metric as OperationsMetricId)) return searchOperationsEvidence(db, actor, operationsQuerySchema.parse(common));
  if (legalMetrics.includes(common.metric as LegalMetricId)) return searchLegalEvidence(db, actor, legalQuerySchema.parse(common));
  if (common.metric === closeBlockersMetric) return searchCloseBlockerEvidence(db, actor, closeBlockersQuerySchema.parse(common));
  if (common.metric === identityChangeMetric) return searchIdentityChangeEvidence(db, actor, identityChangeQuerySchema.parse(common));
  if (common.metric === organizationNodeMetric) return searchOrganizationNodeEvidence(db, actor, organizationNodeQuerySchema.parse(common));
  if (common.metric === pendingStructureMetric) return searchPendingStructureEvidence(db, actor, pendingStructureQuerySchema.parse(common));
  if (common.metric === activeAssignmentsMetric) return searchActiveAssignmentEvidence(db, actor, activeAssignmentsQuerySchema.parse(common));
  if (common.metric === pendingAccessReviewsMetric) return searchPendingAccessReviewEvidence(db, actor, pendingAccessReviewsQuerySchema.parse(common));
  if (common.metric === activeSubscriptionsMetric) return searchCommercialSubscriptions(db, actor, activeSubscriptionsQuerySchema.parse(common));
  if (common.metric === overdueInvoicesMetric) return searchOverdueInvoiceEvidence(db, actor, overdueInvoicesQuerySchema.parse(common));
  if (common.metric === duplicateReviewMetric) return searchDuplicateReviewEvidence(db, actor, duplicateReviewQuerySchema.parse(common));
  if (common.metric === maintenanceDowntimeMetric) return searchMaintenanceDowntimeEvidence(db, actor, maintenanceQuerySchema.parse(common));
  if (supportTicketMetrics.includes(common.metric as SupportTicketMetricId)) return searchSupportTicketEvidence(db, actor, supportTicketQuerySchema.parse(common));
  if (common.metric === supportIncidentMetric) return searchSupportIncidentEvidence(db, actor, supportIncidentQuerySchema.parse(common));
  if (common.metric === stockedLocationsMetric) return searchStockedLocationEvidence(db, actor, stockedLocationsQuerySchema.parse(common));
  if (common.metric === availableStockMetric) return searchAvailableStockEvidence(db, actor, availableStockQuerySchema.parse(common));
  if (common.metric === inventoryShortagesMetric) return searchInventoryShortageEvidence(db, actor, inventoryShortagesQuerySchema.parse(common));
  if (common.metric === inventoryMovementsMetric) return searchInventoryMovementEvidence(db, actor, inventoryMovementsQuerySchema.parse(common));
  if (common.metric === "invoice-match-exceptions") return searchSupplierBillEvidence(db, actor, procurementQuerySchema.parse(common));
  if (common.metric === salesFulfillmentMetric) return searchSalesFulfillmentEvidence(db, actor, salesFulfillmentQuerySchema.parse(common));
  if (common.metric === fieldVisitMetric) return searchFieldVisitEvidence(db, actor, fieldVisitQuerySchema.parse(common));
  if (common.metric === manufacturingCostMetric) return searchManufacturingCostEvidence(db, actor, manufacturingCostQuerySchema.parse(common));
  if (common.metric === customerActivityMetric) return searchCustomerActivityEvidence(db, actor, customerActivityQuerySchema.parse(common));
  if (common.metric === socialActivityMetric) return searchSocialActivityEvidence(db, actor, socialActivityQuerySchema.parse(common));
  if (salesMetrics.includes(common.metric as SalesMetricId)) return searchSalesOrderEvidence(db, actor, salesQuerySchema.parse(common));
  if (qualityMetrics.includes(common.metric as typeof qualityMetrics[number])) return searchQualityInspectionEvidence(db, actor, qualityQuerySchema.parse(common));
  return searchFinanceEvidence(db, actor, querySchema.parse(common));
}

export async function readExecutiveEvidence(db: SqlDatabase, actor: WorkContext, metric: MetricId, recordId: string, filters: Filters) {
  if (crmEvidenceMetrics.includes(metric as typeof crmEvidenceMetrics[number])) return readCrmEvidence(db, actor, metric, recordId, filters);
  if (agentEvidenceMetrics.includes(metric as typeof agentEvidenceMetrics[number])) return readAgentEvidence(db, actor, metric, recordId, filters);
  if (brandEvidenceMetrics.includes(metric as typeof brandEvidenceMetrics[number])) return readBrandEvidence(db, actor, metric, recordId, filters);
  if (learningEvidenceMetrics.includes(metric as typeof learningEvidenceMetrics[number])) return readLearningEvidence(db, actor, metric, recordId, filters);
  if (talentEvidenceMetrics.includes(metric as typeof talentEvidenceMetrics[number])) return readTalentEvidence(db, actor, metric, recordId, filters);
  if (posEvidenceMetrics.includes(metric as typeof posEvidenceMetrics[number])) return readPosEvidence(db, actor, metric, recordId, filters);
  if (marketingEvidenceMetrics.includes(metric as typeof marketingEvidenceMetrics[number])) return readMarketingEvidence(db, actor, metric, recordId, filters);
  if (planningEvidenceMetrics.includes(metric as typeof planningEvidenceMetrics[number])) return readPlanningEvidence(db, actor, metric, recordId, filters);
  if (metric === projectMilestoneMetric) return readProjectMilestoneEvidence(db, actor, recordId, filters);
  if (metric === projectDependencyMetric) return readProjectDependencyEvidence(db, actor, recordId, filters);
  if (projectActualMetrics.includes(metric as ProjectActualMetric)) return readProjectActualEvidence(db, actor, metric as ProjectActualMetric, recordId, filters);
  if (metric === payrollSummaryMetric) return readPayrollSummaryEvidence(db, actor, recordId, filters);
  if (metric === approvedLeaveMetric || metric === authorizedOvertimeMetric) return readPeopleLeaveOvertimeEvidence(db, actor, metric, recordId, filters);
  if (metric === hotelOccupancyMetric || hotelRoomRevenueMetrics.includes(metric as HotelRoomRevenueMetric)) return readHotelOccupancyEvidence(db, actor, metric as typeof hotelOccupancyMetric | HotelRoomRevenueMetric, recordId, filters);
  if (metric === hotelFolioBalanceMetric) return readHotelFolioBalanceEvidence(db, actor, recordId, filters);
  if (metric === hotelNightAuditBlockersMetric) return readHotelNightAuditBlockerEvidence(db, actor, recordId, filters);
  if (operationsMetrics.includes(metric as OperationsMetricId)) return readOperationsEvidence(db, actor, metric as OperationsMetricId, recordId, filters);
  if (legalMetrics.includes(metric as LegalMetricId)) return readLegalEvidence(db, actor, metric as LegalMetricId, recordId, filters);
  if (metric === closeBlockersMetric) return readCloseBlockerEvidence(db, actor, recordId, filters);
  if (metric === identityChangeMetric) return readIdentityChangeEvidence(db, actor, recordId, filters);
  if (metric === organizationNodeMetric) return readOrganizationNodeEvidence(db, actor, recordId, filters);
  if (metric === pendingStructureMetric) return readPendingStructureEvidence(db, actor, recordId, filters);
  if (metric === activeAssignmentsMetric) return readActiveAssignmentEvidence(db, actor, recordId, filters);
  if (metric === pendingAccessReviewsMetric) return readPendingAccessReviewEvidence(db, actor, recordId, filters);
  if (metric === activeSubscriptionsMetric) return readCommercialSubscription(db, actor, recordId, filters);
  if (metric === overdueInvoicesMetric) return readOverdueInvoiceEvidence(db, actor, recordId, filters);
  if (metric === duplicateReviewMetric) return readDuplicateReviewEvidence(db, actor, recordId, filters);
  if (metric === maintenanceDowntimeMetric) return readMaintenanceDowntimeEvidence(db, actor, recordId, filters);
  if (supportTicketMetrics.includes(metric as SupportTicketMetricId)) return readSupportTicketEvidence(db, actor, metric as SupportTicketMetricId, recordId, filters);
  if (metric === supportIncidentMetric) return readSupportIncidentEvidence(db, actor, recordId, filters);
  if (metric === stockedLocationsMetric) return readStockedLocationEvidence(db, actor, recordId);
  if (metric === availableStockMetric) return readAvailableStockEvidence(db, actor, recordId);
  if (metric === inventoryShortagesMetric) return readInventoryShortageEvidence(db, actor, recordId);
  if (metric === inventoryMovementsMetric) return readInventoryMovementEvidence(db, actor, recordId, filters);
  if (metric === "invoice-match-exceptions") return readSupplierBillEvidence(db, actor, recordId, filters);
  if (metric === salesFulfillmentMetric) return readSalesFulfillmentEvidence(db, actor, recordId, filters);
  if (metric === fieldVisitMetric) return readFieldVisitEvidence(db, actor, recordId, filters);
  if (metric === manufacturingCostMetric) return readManufacturingCostEvidence(db, actor, recordId, filters);
  if (metric === customerActivityMetric) return readCustomerActivityEvidence(db, actor, recordId, filters);
  if (metric === socialActivityMetric) return readSocialActivityEvidence(db, actor, recordId, filters);
  if (salesMetrics.includes(metric as SalesMetricId)) return readSalesOrderEvidence(db, actor, metric as SalesMetricId, recordId, filters);
  if (metric === "inspection-pass-rate") return readQualityInspectionEvidence(db, actor, recordId, filters);
  if (metric === "quality-holds" || metric === "quality-releases") return readQualityHoldEvidence(db, actor, metric, recordId, filters);
  if (metric === "receivables" || metric === "payables" || metric === "invoiced-sales") return readFinanceEvidence(db, actor, metric, recordId, filters);
  throw new ApiError(403, "EVIDENCE_FORBIDDEN", "This source record is not available in the current role or scope.");
}
