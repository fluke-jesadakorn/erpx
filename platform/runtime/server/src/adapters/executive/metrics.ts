import {workMetricHistoryAccess} from "@erpx/operations-server/public/record-purchases";
import {serviceMetricHistoryAccess} from "@erpx/field-service-server/public/record-purchases";
import {benefitMetricHistoryAccess} from "@erpx/benefits-server/public/record-purchases";
import {crmMetricHistoryAccess} from "@erpx/crm-server/public/metric-purchases";
import {marketingMetricHistoryAccess} from "@erpx/marketing-server/public/metric-purchases";
import {legalMetricHistoryAccess} from "@erpx/legal-server/public/metric-purchases";
import {talentMetricHistoryAccess} from "@erpx/talent-server/public/metric-purchases";
import {manufacturingMetricHistoryAccess} from "@erpx/manufacturing-server/public/metric-purchases";
import {assetMetricHistoryAccess} from "@erpx/assets-server/public/metric-purchases";
import { metricSourceSelection, type MetricSourceGroup, type MetricSourcePage } from '@erpx/platform-runtime-server/public/executive/metric-sources';
import "server-only";
import { type Breakdown, type Filters, type MetricData, type MetricId } from "@erpx/platform-shared-ui/lib/executive/canvas";
import type { WorkContext } from "@erpx/identity-access-server/public/authorization";
import { sha256 } from "@erpx/platform-runtime-server/public/crypto";
import { ApiError } from "@erpx/platform-runtime-server/public/http";
import { assertProductionModuleEnabled } from "@erpx/pms-server/public/feature-availability";
import type { SqlDatabase } from "@erpx/platform-runtime-server/public/runtime/types";
import { runtimeEnv } from "../../public/runtime";
import { companyAuthority } from "@erpx/operations-server/public/staff-policy";
import { specs, metricCatalog as catalog, metricUnavailable as unavailable } from '@erpx/platform-runtime-server/domain/executive/metrics';
import { createExecutiveMetricApplication, type MetricSnapshot } from '@erpx/platform-runtime-server/application/executive/metrics';
import { BusinessRuleError } from '@erpx/platform-shared-server/index';
import { readExecutiveMetricProjection as owner0 } from '@erpx/billing-server/public/executive-metric-projections';
import { readExecutiveMetricProjection as owner1, financeMetricHistoryAccess } from '@erpx/finance-server/public/executive-metric-projections';
import { readExecutiveMetricProjection as owner2 } from '@erpx/commerce-server/public/executive-metric-projections';
import { readExecutiveMetricProjection as owner3 } from '@erpx/procurement-server/public/executive-metric-projections';
import { readExecutiveMetricProjection as owner4 } from '@erpx/people-payroll-server/public/executive-metric-projections';
import { readExecutiveMetricProjection as owner5 } from '@erpx/operations-server/public/executive-metric-projections';
import { readExecutiveMetricProjection as owner6 } from '@erpx/inventory-server/public/executive-metric-projections';
import { readExecutiveMetricProjection as owner7, posMetricHistoryAccess } from '@erpx/pos-server/public/executive-metric-projections';
import { readExecutiveMetricProjection as owner8, pmsMetricHistoryAccess } from '@erpx/pms-server/public/executive-metric-projections';
import { readExecutiveMetricProjection as owner9, projectMetricHistoryAccess } from '@erpx/projects-server/public/executive-metric-projections';
import { readExecutiveMetricProjection as owner10 } from '@erpx/collaboration-server/public/executive-metric-projections';
import { readExecutiveMetricProjection as owner11 } from '@erpx/customer-authority-server/public/executive-metric-projections';
import { readExecutiveMetricProjection as owner12, supportMetricHistoryAccess } from '@erpx/support-server/public/executive-metric-projections';
import { readExecutiveMetricProjection as owner13 } from '@erpx/assets-server/public/executive-metric-projections';
import { readExecutiveMetricProjection as owner14 } from '@erpx/maintenance-server/public/executive-metric-projections';
import { readExecutiveMetricProjection as owner15 } from '@erpx/quality-server/public/executive-metric-projections';
import { readExecutiveMetricProjection as owner16 } from '@erpx/crm-server/public/executive-metric-projections';
import { readExecutiveMetricProjection as owner17 } from '@erpx/brand-server/public/executive-metric-projections';
import { readExecutiveMetricProjection as owner18 } from '@erpx/identity-access-server/public/executive-metric-projections';
import { readExecutiveMetricProjection as owner19 } from '@erpx/organization-server/public/executive-metric-projections';
import { readExecutiveMetricProjection as owner20 } from '@erpx/marketing-server/public/executive-metric-projections';
import { readExecutiveMetricProjection as owner21 } from '@erpx/manufacturing-server/public/executive-metric-projections';
import { readExecutiveMetricProjection as owner22 } from '@erpx/planning-server/public/executive-metric-projections';
import { readExecutiveMetricProjection as owner23 } from '@erpx/ecommerce-server/public/executive-metric-projections';
import { readExecutiveMetricProjection as owner24 } from '@erpx/benefits-server/public/executive-metric-projections';
import { readExecutiveMetricProjection as owner25, learningMetricHistoryAccess } from '@erpx/learning-server/public/executive-metric-projections';
import { readExecutiveMetricProjection as owner26 } from '@erpx/talent-server/public/executive-metric-projections';
import { readNativeFieldReport } from '@erpx/field-service-server/public/native-executive';
import { readExecutiveMetricProjection as owner28 } from '@erpx/agent-server/public/executive-metric-projections';
import { readExecutiveMetricProjection as owner29 } from '@erpx/legal-server/public/executive-metric-projections';
import { executiveBillingWatermarks } from '@erpx/billing-server/public/executive-metric-watermarks';
import { executiveMetricWatermarks } from '@erpx/work-projection-server/public/executive-metric-watermarks';
const readers = { 'billing': owner0, 'finance': owner1, 'commerce': owner2, 'procurement': owner3, 'people_payroll': owner4, 'operations': owner5, 'inventory': owner6, 'pos': owner7, 'pms': owner8, 'projects': owner9, 'collaboration': owner10, 'customer_authority': owner11, 'support': owner12, 'assets': owner13, 'maintenance': owner14, 'quality': owner15, 'crm': owner16, 'brand': owner17, 'identity_access': owner18, 'organization': owner19, 'marketing': owner20, 'manufacturing': owner21, 'planning': owner22, 'ecommerce': owner23, 'benefits': owner24, 'learning': owner25, 'talent': owner26, 'field_service': readFieldExecutiveMetric, 'agent': owner28, 'legal': owner29 };
const metricOwner: Record<MetricId, keyof typeof readers> = { "legal-records": "legal", "legal-contract-value": "legal", "legal-exposure": "legal", "active-legal-obligations": "legal", "active-legal-holds": "legal", "active-subscriptions": "billing", "overdue-invoices": "billing", "receivables": "finance", "payables": "finance", "invoiced-sales": "finance", "cash-receipts": "finance", "close-blockers": "finance", "sales-orders": "commerce", "sales-fulfillment-rate": "commerce", "sales-cancellations": "commerce", "sales-cancellation-value": "commerce", "purchase-orders": "commerce", "open-po-receipts": "commerce", "invoice-match-exceptions": "procurement", "headcount": "people_payroll", "approved-leave-duration": "people_payroll", "authorized-overtime-duration": "people_payroll", "payroll-summary": "people_payroll", "my-tasks": "operations", "open-operations-exceptions": "operations", "exception-resolution-time": "operations", "operations-escalations": "operations", "collaboration-thread-activity": "collaboration", "inventory": "inventory", "available-stock": "inventory", "inventory-shortages": "inventory", "inventory-movements": "inventory", "pos-sales": "pos", "net-pos-sales": "pos", "pos-payment-mix": "pos", "hotel-rooms": "pms", "hotel-occupancy": "pms", "hotel-adr": "pms", "hotel-revpar": "pms", "hotel-guest-account-balance": "pms", "hotel-night-audit-blockers": "pms", "projects": "projects", "overdue-project-milestones": "projects", "project-dependency-blockers": "projects", "project-actual-hours": "projects", "project-posted-cost": "projects", "routines": "operations", "active-customers": "finance", "recent-customer-activity": "commerce", "duplicate-reviews-pending": "customer_authority", "identity-change-events": "customer_authority", "open-support-tickets": "support", "overdue-support-sla": "support", "active-support-incidents": "support", "active-assets": "assets", "asset-condition-location": "assets", "asset-retirements": "assets", "open-work-orders": "maintenance", "blocked-maintenance": "maintenance", "asset-downtime": "maintenance", "quality-holds": "quality", "quality-releases": "quality", "inspection-pass-rate": "quality", "opportunity-pipeline": "crm", "overdue-followups": "crm", "brand-profiles": "brand", "brand-profile-updates": "brand", "active-assignments": "identity_access", "pending-access-reviews": "identity_access", "active-organization-nodes": "organization", "pending-structure-changes": "organization", "marketing-lead-candidates": "marketing", "marketing-engagement-events": "marketing", "completed-manufacturing-orders": "manufacturing", "manufacturing-orders-with-scrap": "manufacturing", "production-cost": "manufacturing", "planning-shortage-scenarios": "planning", "proposed-planning-recommendations": "planning", "capacity-feasible-recommendations": "planning", "open-ecommerce-carts": "ecommerce", "completed-ecommerce-checkouts": "ecommerce", "ecommerce-return-links": "ecommerce", "ecommerce-fulfillment-links": "ecommerce", "active-benefit-elections": "benefits", "published-benefit-plans": "benefits", "learning-completions": "learning", "overdue-required-learning": "learning", "open-requisitions": "talent", "application-stage-counts": "talent", "open-service-orders": "field_service", "service-billing-releases": "field_service", "field-visit-completion": "field_service", "approved-agent-versions": "agent", "agent-invocation-outcomes": "agent" };
/** Field reports are read from the mounted native owner. The generic engine only
 * maps the native DTO to its snapshot/source contract; no Field SQL, source
 * policy or access decision lives here. */
async function readFieldExecutiveMetric(_sourceDb: SqlDatabase, actor: WorkContext, id: MetricId, filters: Filters, breakdown: Breakdown): Promise<MetricSnapshot> {
    void _sourceDb;
    if (id !== 'open-service-orders' && id !== 'service-billing-releases' && id !== 'field-visit-completion')
        throw new ApiError(404, 'METRIC_NOT_FOUND', 'This Field metric is unavailable.');
    const query = { metric: id, breakdown, fromAt: Date.parse(filters.from), toAt: Date.parse(filters.to) };
    return {
        raw: (await readNativeFieldReport(actor, query)).raw,
        referenceAtOverride: null,
        watermarks: async () => [],
        source: async (selection, reported) => {
            const current = await readNativeFieldReport(actor, query);
            let selected: ReturnType<typeof metricSourceSelection>;
            try { selected = metricSourceSelection(selection, current.raw); }
            catch (error) { if (error instanceof BusinessRuleError) throw new ApiError(error.status, error.code, error.message); throw error; }
            const labels = new Set(selected.labels);
            // Every row and group was already authorized and ordered in Rust.
            const rows = current.sources.filter(row => selected.other ? !labels.has(row.label) : labels.has(row.label)).filter(row => !selection.recordId || row.id === selection.recordId);
            const start = selection.recordId ? 0 : (selection.page - 1) * 30;
            const selectedRows = rows.slice(start, start + (selection.recordId ? 1 : 31)).map(({label, ...row}) => { void label; return row; });
            return { row: { name: selection.label, reported_total: reported, description: `${specs[id].dateBasis} · ${filters.from} to ${filters.to} · ${filters.currency}` }, rows: selectedRows, page: selection.page, hasNext: selectedRows.length > 30 };
        }
    };
}
async function read(db: SqlDatabase, a: WorkContext, id: MetricId, f: Filters, breakdown: Breakdown): Promise<MetricSnapshot> { const spec = specs[id], sourceDb = spec.holdingOwnerOnly ? (await runtimeEnv()).CONTROL_DB : db, snapshot = await readers[metricOwner[id]](sourceDb, a, id, f, breakdown); return { ...snapshot, watermarks: () => spec.holdingOwnerOnly ? executiveBillingWatermarks(sourceDb, a) : executiveMetricWatermarks(sourceDb, spec.sourceTables) }; }
function application(database: SqlDatabase) { return createExecutiveMetricApplication({ clock: { now: Date.now }, digest: sha256, production: () => process.env.NODE_ENV === 'production', companyAuthority: actor => fromNative(() => companyAuthority(database, actor)), enabledModules: async (actor) => (await import('@erpx/pms-server/public/feature-availability')).enabledProductionModules(database, actor.companyId), assertModule: (actor, module) => fromNative(() => assertProductionModuleEnabled(database, actor.companyId, module)), read: (actor, id, filters, breakdown) => fromNative(() => read(database, actor, id, filters, breakdown)) }); }
async function fromNative<T>(run: () => Promise<T> | T): Promise<T> { try {
    return await run();
}
catch (error) {
    if (error instanceof ApiError)
        throw new BusinessRuleError(error.status, error.code, error.message);
    throw error;
} }
async function invoke<T>(run: () => Promise<T> | T): Promise<T> { try {
    return await run();
}
catch (error) {
    if (error instanceof BusinessRuleError)
        throw new ApiError(error.status, error.code, error.message);
    throw error;
} }
export { metricSupportsComparison, metricBreakdowns } from '@erpx/platform-runtime-server/domain/executive/metrics';
export function metricCatalog(actor: WorkContext) { return catalog(actor, process.env.NODE_ENV === 'production'); }
export function metricCatalogFor(db: SqlDatabase, a: WorkContext) { return invoke(async () => {
    const metrics = await application(db).metricCatalogFor(a);
    const owners = ["support", "pos", "pms", "finance", "projects", "learning", "talent", "assets", "maintenance", "manufacturing", "planning", "legal", "crm", "marketing", "benefits", "field_service", "operations"] as const, access: Record<string, Record<string, boolean>> = {};
    for (const owner of owners) if (metrics.some(metric => metricOwner[metric.id] === owner && metric.available))
      access[owner] = await (owner === "operations" ? workMetricHistoryAccess(db,a) : owner === "field_service" ? serviceMetricHistoryAccess(db,a) : owner === "benefits" ? benefitMetricHistoryAccess(db,a) : owner === "crm" ? crmMetricHistoryAccess(db,a) : owner === "marketing" ? marketingMetricHistoryAccess(db,a) : owner === "legal" ? legalMetricHistoryAccess(db,a) : (owner === "manufacturing"||owner === "planning") ? access.manufacturing??access.planning??manufacturingMetricHistoryAccess(db,a) : (owner === "assets"||owner === "maintenance") ? assetMetricHistoryAccess(db,a) : owner === "talent" ? talentMetricHistoryAccess(db,a) : owner === "learning" ? learningMetricHistoryAccess(db,a) : owner === "projects" ? projectMetricHistoryAccess(db,a) : owner === "finance" ? financeMetricHistoryAccess(db,a) : owner === "support" ? supportMetricHistoryAccess(db, a) : owner === "pos" ? posMetricHistoryAccess(db, a) : pmsMetricHistoryAccess(db, a));
    return metrics.map(metric => metric.available && access[metricOwner[metric.id]] && !access[metricOwner[metric.id]][metric.id]
        ? { ...metric, available: false, reason: "A started purchase and current source authority are required to read this metric." } : metric);
}); }
export function queryMetric(db: SqlDatabase, a: WorkContext, id: MetricId, f: Filters, breakdown?: Breakdown): Promise<MetricData>;
export function queryMetric(db: SqlDatabase, a: WorkContext, id: MetricId, f: Filters, breakdown: Breakdown, sourceGroup: MetricSourceGroup): Promise<MetricSourcePage>;
export function queryMetric(db: SqlDatabase, a: WorkContext, id: MetricId, f: Filters, breakdown: Breakdown = 'default', sourceGroup?: MetricSourceGroup) { return invoke(() => application(db).queryMetric(a, id, f, breakdown, sourceGroup)); }
export function metricUnavailable(id: MetricId, reason: string, status: 'unavailable' | 'failed' = 'unavailable', currency = 'THB'): MetricData { return unavailable(id, reason, status, currency, Date.now()); }
