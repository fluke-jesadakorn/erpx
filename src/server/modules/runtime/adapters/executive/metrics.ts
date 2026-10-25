import { type MetricSourceGroup, type MetricSourcePage } from '../../public/executive/metric-sources';
import "server-only";
import { type Breakdown, type Filters, type MetricData, type MetricId } from "@/lib/executive/canvas";
import type { WorkContext } from "@/server/modules/identity_access/public/authorization";
import { sha256 } from "../../public/crypto";
import { ApiError } from "../../public/http";
import { assertProductionModuleEnabled } from "@/server/modules/pms/public/hospitality-rollout";
import type { SqlDatabase } from "../../public/runtime/types";
import { runtimeEnv } from "../../public/runtime";
import { companyAuthority } from "@/server/modules/operations/public/staff-policy";
import { specs, metricCatalog as catalog, metricUnavailable as unavailable } from '../../domain/executive/metrics';
import { createExecutiveMetricApplication, type MetricSnapshot } from '../../application/executive/metrics';
import { BusinessRuleError } from '../../../shared';
import { readExecutiveMetricProjection as owner0 } from '../../../billing/public/executive-metric-projections';
import { readExecutiveMetricProjection as owner1 } from '../../../finance/public/executive-metric-projections';
import { readExecutiveMetricProjection as owner2 } from '../../../commerce/public/executive-metric-projections';
import { readExecutiveMetricProjection as owner3 } from '../../../procurement/public/executive-metric-projections';
import { readExecutiveMetricProjection as owner4 } from '../../../people_payroll/public/executive-metric-projections';
import { readExecutiveMetricProjection as owner5 } from '../../../operations/public/executive-metric-projections';
import { readExecutiveMetricProjection as owner6 } from '../../../inventory/public/executive-metric-projections';
import { readExecutiveMetricProjection as owner7 } from '../../../pos/public/executive-metric-projections';
import { readExecutiveMetricProjection as owner8 } from '../../../pms/public/executive-metric-projections';
import { readExecutiveMetricProjection as owner9 } from '../../../projects/public/executive-metric-projections';
import { readExecutiveMetricProjection as owner10 } from '../../../collaboration/public/executive-metric-projections';
import { readExecutiveMetricProjection as owner11 } from '../../../customer_authority/public/executive-metric-projections';
import { readExecutiveMetricProjection as owner12 } from '../../../support/public/executive-metric-projections';
import { readExecutiveMetricProjection as owner13 } from '../../../assets/public/executive-metric-projections';
import { readExecutiveMetricProjection as owner14 } from '../../../maintenance/public/executive-metric-projections';
import { readExecutiveMetricProjection as owner15 } from '../../../quality/public/executive-metric-projections';
import { readExecutiveMetricProjection as owner16 } from '../../../crm/public/executive-metric-projections';
import { readExecutiveMetricProjection as owner17 } from '../../../brand/public/executive-metric-projections';
import { readExecutiveMetricProjection as owner18 } from '../../../identity_access/public/executive-metric-projections';
import { readExecutiveMetricProjection as owner19 } from '../../../organization/public/executive-metric-projections';
import { readExecutiveMetricProjection as owner20 } from '../../../marketing/public/executive-metric-projections';
import { readExecutiveMetricProjection as owner21 } from '../../../manufacturing/public/executive-metric-projections';
import { readExecutiveMetricProjection as owner22 } from '../../../planning/public/executive-metric-projections';
import { readExecutiveMetricProjection as owner23 } from '../../../ecommerce/public/executive-metric-projections';
import { readExecutiveMetricProjection as owner24 } from '../../../benefits/public/executive-metric-projections';
import { readExecutiveMetricProjection as owner25 } from '../../../learning/public/executive-metric-projections';
import { readExecutiveMetricProjection as owner26 } from '../../../talent/public/executive-metric-projections';
import { readExecutiveMetricProjection as owner27 } from '../../../field_service/public/executive-metric-projections';
import { readExecutiveMetricProjection as owner28 } from '../../../agent/public/executive-metric-projections';
import { readExecutiveMetricProjection as owner29 } from '../../../legal/public/executive-metric-projections';
import { executiveBillingWatermarks } from '../../../billing/public/executive-metric-watermarks';
import { executiveMetricWatermarks } from '../../../work_projection/public/executive-metric-watermarks';
const readers = { 'billing': owner0, 'finance': owner1, 'commerce': owner2, 'procurement': owner3, 'people_payroll': owner4, 'operations': owner5, 'inventory': owner6, 'pos': owner7, 'pms': owner8, 'projects': owner9, 'collaboration': owner10, 'customer_authority': owner11, 'support': owner12, 'assets': owner13, 'maintenance': owner14, 'quality': owner15, 'crm': owner16, 'brand': owner17, 'identity_access': owner18, 'organization': owner19, 'marketing': owner20, 'manufacturing': owner21, 'planning': owner22, 'ecommerce': owner23, 'benefits': owner24, 'learning': owner25, 'talent': owner26, 'field_service': owner27, 'agent': owner28, 'legal': owner29 };
const metricOwner: Record<MetricId, keyof typeof readers> = { "legal-records": "legal", "legal-contract-value": "legal", "legal-exposure": "legal", "active-legal-obligations": "legal", "active-legal-holds": "legal", "active-subscriptions": "billing", "overdue-invoices": "billing", "receivables": "finance", "payables": "finance", "invoiced-sales": "finance", "cash-receipts": "finance", "close-blockers": "finance", "sales-orders": "commerce", "sales-fulfillment-rate": "commerce", "sales-cancellations": "commerce", "sales-cancellation-value": "commerce", "purchase-orders": "commerce", "open-po-receipts": "commerce", "invoice-match-exceptions": "procurement", "headcount": "people_payroll", "approved-leave-duration": "people_payroll", "authorized-overtime-duration": "people_payroll", "payroll-summary": "people_payroll", "my-tasks": "operations", "open-operations-exceptions": "operations", "exception-resolution-time": "operations", "operations-escalations": "operations", "collaboration-thread-activity": "collaboration", "inventory": "inventory", "available-stock": "inventory", "inventory-shortages": "inventory", "inventory-movements": "inventory", "pos-sales": "pos", "net-pos-sales": "pos", "pos-payment-mix": "pos", "hotel-rooms": "pms", "hotel-occupancy": "pms", "hotel-adr": "pms", "hotel-revpar": "pms", "hotel-guest-account-balance": "pms", "hotel-night-audit-blockers": "pms", "projects": "projects", "overdue-project-milestones": "projects", "project-dependency-blockers": "projects", "project-actual-hours": "projects", "project-posted-cost": "projects", "routines": "operations", "active-customers": "finance", "recent-customer-activity": "commerce", "duplicate-reviews-pending": "customer_authority", "identity-change-events": "customer_authority", "open-support-tickets": "support", "overdue-support-sla": "support", "active-support-incidents": "support", "active-assets": "assets", "asset-condition-location": "assets", "asset-retirements": "assets", "open-work-orders": "maintenance", "blocked-maintenance": "maintenance", "asset-downtime": "maintenance", "quality-holds": "quality", "quality-releases": "quality", "inspection-pass-rate": "quality", "opportunity-pipeline": "crm", "overdue-followups": "crm", "brand-profiles": "brand", "brand-profile-updates": "brand", "active-assignments": "identity_access", "pending-access-reviews": "identity_access", "active-organization-nodes": "organization", "pending-structure-changes": "organization", "marketing-lead-candidates": "marketing", "marketing-engagement-events": "marketing", "completed-manufacturing-orders": "manufacturing", "manufacturing-orders-with-scrap": "manufacturing", "production-cost": "manufacturing", "planning-shortage-scenarios": "planning", "proposed-planning-recommendations": "planning", "capacity-feasible-recommendations": "planning", "open-ecommerce-carts": "ecommerce", "completed-ecommerce-checkouts": "ecommerce", "ecommerce-return-links": "ecommerce", "ecommerce-fulfillment-links": "ecommerce", "active-benefit-elections": "benefits", "published-benefit-plans": "benefits", "learning-completions": "learning", "overdue-required-learning": "learning", "open-requisitions": "talent", "application-stage-counts": "talent", "open-service-orders": "field_service", "service-billing-releases": "field_service", "field-visit-completion": "field_service", "approved-agent-versions": "agent", "agent-invocation-outcomes": "agent" };
async function read(db: SqlDatabase, a: WorkContext, id: MetricId, f: Filters, breakdown: Breakdown): Promise<MetricSnapshot> { const spec = specs[id], sourceDb = spec.holdingOwnerOnly ? (await runtimeEnv()).CONTROL_DB : db, snapshot = await readers[metricOwner[id]](sourceDb, a, id, f, breakdown); return { ...snapshot, watermarks: () => spec.holdingOwnerOnly ? executiveBillingWatermarks(sourceDb, a.holdingId) : executiveMetricWatermarks(sourceDb, spec.sourceTables) }; }
function application(database: SqlDatabase) { return createExecutiveMetricApplication({ clock: { now: Date.now }, digest: sha256, production: () => process.env.NODE_ENV === 'production', companyAuthority: actor => fromNative(() => companyAuthority(database, actor)), enabledModules: async (actor) => (await import('../../../pms/public/hospitality-rollout')).enabledProductionModules(database, actor.companyId), assertModule: (actor, module) => fromNative(() => assertProductionModuleEnabled(database, actor.companyId, module)), read: (actor, id, filters, breakdown) => fromNative(() => read(database, actor, id, filters, breakdown)) }); }
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
export { metricSupportsComparison, metricBreakdowns } from '../../domain/executive/metrics';
export function metricCatalog(actor: WorkContext) { return catalog(actor, process.env.NODE_ENV === 'production'); }
export function metricCatalogFor(db: SqlDatabase, a: WorkContext) { return invoke(() => application(db).metricCatalogFor(a)); }
export function queryMetric(db: SqlDatabase, a: WorkContext, id: MetricId, f: Filters, breakdown?: Breakdown): Promise<MetricData>;
export function queryMetric(db: SqlDatabase, a: WorkContext, id: MetricId, f: Filters, breakdown: Breakdown, sourceGroup: MetricSourceGroup): Promise<MetricSourcePage>;
export function queryMetric(db: SqlDatabase, a: WorkContext, id: MetricId, f: Filters, breakdown: Breakdown = 'default', sourceGroup?: MetricSourceGroup) { return invoke(() => application(db).queryMetric(a, id, f, breakdown, sourceGroup)); }
export function metricUnavailable(id: MetricId, reason: string, status: 'unavailable' | 'failed' = 'unavailable', currency = 'THB'): MetricData { return unavailable(id, reason, status, currency, Date.now()); }
