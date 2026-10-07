import { cachedWorkSnapshot } from "../public/work-cache";
import { measureReads } from "@/server/modules/runtime/public/runtime/read-metrics";
import { withPolicyReadSnapshot } from "@/server/modules/identity_access/public/iam-read-snapshot";
import { enrichFlowWork } from "@/server/modules/workflow/public/flow-work";
import "server-only";
import type { WorkContext } from "@/server/modules/identity_access/public/authorization";
import { getDomainWork } from "./domain-work";
import { getWork } from "../public/work";
import { type WorkItem } from "@/lib/work-item";
import { holdingResources } from "@/server/modules/runtime/public/runtime";
import { ApiError } from "@/server/modules/runtime/public/http";
import { createWorkSnapshot, type WorkSnapshotPorts } from "../application/work-items";
import { mergeWorkItems, nextWorkSteps as decideNextWorkSteps, type WorkSnapshot } from "../domain/work-items";
import type { SqlDatabase } from "../../runtime/public/runtime/types";
export { mergeWorkItems };
export type { WorkSnapshot };
export function nextWorkSteps(items: WorkItem[], now = Date.now()) { return decideNextWorkSteps(items, now); }
export function workSnapshotPorts(write: SqlDatabase): WorkSnapshotPorts {
    return {
        source: async (name, actor) => {
            switch (name) {
                case "financeExpansionWork": return (await import("@/server/modules/finance/public/finance-expansion-work")).financeExpansionWork(actor);
                case "legalWork": return (await import("@/server/modules/legal/public/legal/integrations")).legalWork(write, actor);
                case "organizationChangeWork": return (await import("@/server/modules/organization/public/organization-work")).organizationChangeWork(write, actor);
                case "companyBusinessAcceptanceWork": return (await import("@/server/modules/finance/public/company-business-acceptance")).companyBusinessAcceptanceWork(write, actor);
                case "posWork": return (await import("@/server/modules/pos/public/pos")).posWork(actor);
                case "crmWork": return (await import("@/server/modules/crm/public/crm")).crmWork(actor);
                case "supportAlertWork": return (await import("@/server/modules/support/public/support-sla-alerts")).supportAlertWork(actor);
                case "incidentWork": return (await import("@/server/modules/support/public/support-incidents")).incidentWork(actor);
                case "supportPolicyWork": return (await import("@/server/modules/support/public/support-sla")).supportPolicyWork(actor);
                case "supportWork": return (await import("@/server/modules/support/public/support")).supportWork(actor);
                case "supportSessionWork": return (await import("@/server/modules/support/public/support-sessions")).supportSessionWork(actor);
                case "maintenanceWork": return (await import("@/server/modules/maintenance/public/asset-maintenance")).maintenanceWork(actor);
                case "preventivePlanWork": return (await import("@/server/modules/maintenance/public/maintenance-preventive")).preventivePlanWork(actor);
                case "projectHandoffWork": return (await import("@/server/modules/projects/public/project-structure")).projectHandoffWork(actor);
                case "fieldServiceWork": return (await import("@/server/modules/field_service/public/field-service")).fieldServiceWork(actor);
                case "collaborationActionWork": return (await import("@/server/modules/collaboration/public/collaboration-work")).collaborationActionWork(actor);
                case "operationsExceptionWork": return (await import("@/server/modules/operations/public/operations-exceptions")).operationsExceptionWork(actor);
                case "learningWork": return (await import("@/server/modules/learning/public/learning")).learningWork(actor);
                case "benefitWork": return (await import("@/server/modules/benefits/public/benefits")).benefitWork(actor);
                case "talentWork": return (await import("@/server/modules/talent/public/talent")).talentWork(actor);
                case "marketingWork": return (await import("@/server/modules/marketing/public/marketing")).marketingWork(actor);
                case "manufacturingWork": return (await import("@/server/modules/manufacturing/public/production")).manufacturingWork(actor);
                case "customerAccessWork": return (await import("@/server/modules/customer_authority/public/customer-access")).customerAccessWork(actor);
                case "customerDuplicateWork": return (await import("@/server/modules/customer_authority/public/customer-duplicates")).customerDuplicateWork(actor);
                case "customerIdentityWork": return (await import("@/server/modules/customer_authority/public/customer-identity-changes")).customerIdentityWork(actor);
                case "customerFollowupWork": return (await import("@/server/modules/sales/public/customer-followups")).customerFollowupWork(actor);
                case "supplierBillWork": return (await import("@/server/modules/procurement/public/supplier-bill-matching")).supplierBillWork(actor);
                case "landedCostWork": return (await import("@/server/modules/inventory/public/inventory-landed-costs")).landedCostWork(actor);
                case "rfqWork": return (await import("@/server/modules/procurement/public/procurement-rfqs")).rfqWork(actor);
                case "supplierPromiseWork": return (await import("@/server/modules/procurement/public/supplier-promises")).supplierPromiseWork(actor);
                case "transitWork": return (await import("@/server/modules/inventory/public/inventory-transit")).transitWork(actor);
                case "qualityWork": return (await import("@/server/modules/quality/public/quality")).qualityWork(actor);
                case "qualityAlertWork": return (await import("@/server/modules/quality/public/quality-alerts")).qualityAlertWork(actor);
                case "supplierBatchWork": return (await import("@/server/modules/procurement/public/supplier-payments")).supplierBatchWork(actor);
                case "refundWork": return (await import("@/server/modules/finance/public/refunds")).refundWork(actor);
                case "overtimeWork": return (await import("@/server/modules/people_payroll/public/overtime")).overtimeWork(actor);
                case "pmsWork": return (await import("@/server/modules/pms/public/pms")).pmsWork(actor);
                case "employeeRequestWork": return (await import("@/server/modules/records/public/employee-requests")).employeeRequestWork(write, actor);
                case "departmentOverrideWork": return (await import("@/server/modules/organization/public/department-decisions")).departmentOverrideWork(actor);
                case "retirementWork": return (await import("@/server/modules/people_payroll/public/retirement-clearance")).retirementWork(write, actor);
                case "peopleLifecycleWork": return (await import("@/server/modules/people_payroll/public/people-lifecycle")).peopleLifecycleWork(write, actor);
                case "otherFlowWork": return (await import("@/server/modules/workflow/public/flow-work")).otherFlowWork(actor);
            }
        }, domain: (actor, source) => getDomainWork(actor, source), leave: actor => getWork(actor.holdingId, actor), enrich: (actor, items) => enrichFlowWork(actor, items), enabledModules: async (actor) => (await import("../../pms/public/hospitality-rollout")).enabledProductionModules(write, actor.companyId), production: () => process.env.NODE_ENV === 'production', monotonic: () => performance.now(), performanceLog: () => process.env.ERPX_PERF_LOG === '1', errorStatus: error => error instanceof ApiError ? error.status : undefined, info: value => console.info(value), warn: (...values) => console.warn(...values), cached: (actor, load) => cachedWorkSnapshot(actor, write, load)
    };
}
export async function readWorkSnapshot(actor: WorkContext): Promise<WorkSnapshot> { const { write } = await holdingResources(actor.holdingId); return createWorkSnapshot(workSnapshotPorts(write)).readWorkSnapshot(actor); }
export async function workSnapshot(actor: WorkContext): Promise<WorkSnapshot> { const { write } = await holdingResources(actor.holdingId); return cachedWorkSnapshot(actor, write, async () => { const measured = await measureReads(() => withPolicyReadSnapshot(() => createWorkSnapshot(workSnapshotPorts(write)).readWorkSnapshot(actor))); if (process.env.ERPX_PERF_LOG === '1')
    console.info(JSON.stringify({ event: 'work_snapshot', ...measured.metrics, items: measured.result.items.length })); return measured.result; }); }
export async function workItems(actor: WorkContext) { return (await workSnapshot(actor)).items; }
