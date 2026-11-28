import { cachedWorkSnapshot } from "@erpx/work-projection-server/public/work-cache";
import { measureReads } from "@erpx/platform-runtime-server/public/runtime/read-metrics";
import { withPolicyReadSnapshot } from "@erpx/identity-access-server/public/iam-read-snapshot";
import { enrichFlowWork } from "@erpx/workflow-server/public/flow-work";
import "server-only";
import { headers } from "next/headers";
import type { WorkContext } from "@erpx/identity-access-server/public/authorization";
import { getDomainWork } from "@erpx/work-projection-server/adapters/domain-work";
import { getWork } from "@erpx/work-projection-server/public/work";
import { type WorkItem } from "@erpx/work-projection-ui/lib/work-item";
import { holdingResources } from "../../../../../platform/runtime/server/src/public/runtime";
import { ApiError } from "@erpx/platform-runtime-server/public/http";
import { createWorkSnapshot, type WorkSnapshotPorts } from "@erpx/work-projection-server/application/work-items";
import { mergeWorkItems, nextWorkSteps as decideNextWorkSteps, type WorkSnapshot } from "@erpx/work-projection-server/domain/work-items";
import type { SqlDatabase } from "@erpx/platform-runtime-server/public/runtime/types";
import {
    nativeWorkHomeUnread,
    nativeWorkHomeUnreadConfigured,
    nativeWorkSnapshotConfigured,
    readNativeWorkSnapshot,
    type NativeWorkHomeUnreadAvailability,
    type NativeWorkSnapshotAvailability,
    type NativeWorkSnapshotItem,
} from "@erpx/work-projection-server/adapters/native-work-snapshot";
import { unreadHomeCount } from "@erpx/work-projection-server/public/navigation-counts";
export { mergeWorkItems };
export type { WorkSnapshot };
export function nextWorkSteps(items: WorkItem[], now = Date.now()) { return decideNextWorkSteps(items, now); }
export function workSnapshotPorts(write: SqlDatabase): WorkSnapshotPorts {
    return {
        source: async (name, actor) => {
            switch (name) {
                case "financeExpansionWork": return (await import("@erpx/finance-server/public/finance-expansion-work")).financeExpansionWork(actor);
                case "legalWork": return (await import("@erpx/legal-server/public/legal/integrations")).legalWork(write, actor);
                case "organizationChangeWork": return (await import("@erpx/organization-server/public/organization-work")).organizationChangeWork(write, actor);
                case "companyBusinessAcceptanceWork": return (await import("@erpx/finance-server/public/company-business-acceptance")).companyBusinessAcceptanceWork(write, actor);
                case "posWork": return (await import("@erpx/pos-server/public/pos")).posWork(actor);
                case "crmWork": return (await import("@erpx/crm-server/public/crm")).crmWork(actor);
                case "supportAlertWork": return (await import("@erpx/support-server/public/support-sla-alerts")).supportAlertWork(actor);
                case "incidentWork": return (await import("@erpx/support-server/public/support-incidents")).incidentWork(actor);
                case "supportPolicyWork": return (await import("@erpx/support-server/public/support-sla")).supportPolicyWork(actor);
                case "supportWork": return (await import("@erpx/support-server/public/support")).supportWork(actor);
                case "maintenanceWork": return (await import("@erpx/maintenance-server/public/asset-maintenance")).maintenanceWork(actor);
                case "preventivePlanWork": return (await import("../../../../maintenance/server/src/public/native-preventive-read")).preventivePlanWork(actor);
                case "projectHandoffWork": return (await import("@erpx/projects-server/public/project-structure")).projectHandoffWork(actor);
                case "fieldServiceWork": {
                    const native=await (await import("@erpx/field-service-server/public/native-work")).nativeServiceWork(actor);
                    if(!native){
                        // The native Field source is optional in development. An
                        // absent config means the source is disabled, not that a
                        // purchased source is degraded; production still fails closed.
                        if(process.env.NODE_ENV==="production")throw new ApiError(503,"NATIVE_SERVICE_UNAVAILABLE","The native Service work queue is unavailable.");
                        return [];
                    }
                    return native.data;
                }
                case "collaborationActionWork": return (await import("@erpx/collaboration-server/public/collaboration-work")).collaborationActionWork(actor,async()=>(await import("@erpx/field-service-server/public/native-work")).nativeFieldCollaborationWork(actor));
                case "operationsExceptionWork": return (await import("@erpx/operations-server/public/operations-exceptions")).operationsExceptionWork(actor,async()=>(await import("@erpx/field-service-server/public/native-work")).nativeFieldOperationsWork(actor));
                case "learningWork": return (await import("@erpx/learning-server/public/learning")).nativeLearningWork(actor);
                case "benefitWork": return (await import("@erpx/benefits-server/public/benefits-native-read")).benefitWork(actor);
                case "talentWork": return (await import("@erpx/talent-server/public/talent")).talentWork(actor);
                case "marketingWork": return (await import("@erpx/marketing-server/public/marketing")).marketingWork(actor);
                case "manufacturingWork": return (await import("@erpx/manufacturing-server/public/production")).manufacturingWork(actor);
                case "customerAccessWork": return (await import("@erpx/customer-authority-server/public/customer-access")).customerAccessWork(actor);
                case "customerDuplicateWork": return (await import("@erpx/customer-authority-server/public/customer-duplicates")).customerDuplicateWork(actor);
                case "customerIdentityWork": return (await import("@erpx/customer-authority-server/public/customer-identity-changes")).customerIdentityWork(actor);
                case "customerFollowupWork": return (await import("@erpx/sales-server/public/customer-followups")).customerFollowupWork(actor);
                case "supplierBillWork": return (await import("@erpx/procurement-server/public/supplier-bill-matching")).supplierBillWork(actor);
                case "landedCostWork": return (await import("@erpx/inventory-server/public/inventory-landed-costs")).landedCostWork(actor);
                case "rfqWork": return (await import("@erpx/procurement-server/public/procurement-rfqs")).rfqWork(actor);
                case "supplierPromiseWork": return (await import("@erpx/procurement-server/public/supplier-promises")).supplierPromiseWork(actor);
                case "transitWork": return (await import("@erpx/inventory-server/public/inventory-transit")).transitWork(actor);
                case "qualityWork": return (await import("../../../../quality/ui/src/public/server")).nativeQualityWork(actor).then(result=>result.items);
                case "qualityAlertWork": return (await import("@erpx/quality-server/public/quality-alerts")).qualityAlertWork(actor);
                case "supplierBatchWork": return (await import("@erpx/procurement-server/public/supplier-payments")).supplierBatchWork(actor);
                case "refundWork": return (await import("@erpx/finance-server/public/refunds")).refundWork(actor);
                case "overtimeWork": return (await import("@erpx/people-payroll-server/public/overtime")).overtimeWork(actor);
                case "pmsWork": return (await import("../../../../pms/server/src/public/pms")).pmsWork(actor);
                case "employeeRequestWork": return (await import("@erpx/records-server/public/employee-requests")).employeeRequestWork(write, actor);
                case "departmentOverrideWork": return (await import("@erpx/organization-server/public/department-decisions")).departmentOverrideWork(actor);
                case "retirementWork": return (await import("@erpx/people-payroll-server/public/retirement-clearance")).retirementWork(write, actor);
                case "peopleLifecycleWork": return (await import("@erpx/people-payroll-server/public/people-lifecycle")).peopleLifecycleWork(write, actor);
                case "fieldFinancialWork": {
                    try {
                        const native=await (await import("@erpx/finance-server/public/native-field-work")).nativeFieldFinancialWork(actor);
                        if(!native)throw new ApiError(503,"NATIVE_FINANCIAL_WORK_UNAVAILABLE","The native Field financial Work source is unavailable.");
                        return native;
                    }catch(error){if(error instanceof ApiError&&error.status===403)return [];throw error;}
                }
                case "otherFlowWork": return (await import("@erpx/workflow-server/public/flow-work")).otherFlowWork(actor);
            }
            // Retired owner reads have no TypeScript source. The native
            // snapshot reports them as unavailable rather than falling back
            // to a retained reader.
            return [];
        }, domain: (actor, source) => getDomainWork(actor, source), leave: actor => getWork(actor.holdingId, actor), enrich: (actor, items) => enrichFlowWork(actor, items), enabledModules: async (actor) => (await import("@erpx/pms-server/public/feature-availability")).enabledProductionModules(write, actor.companyId), production: () => process.env.NODE_ENV === 'production', monotonic: () => performance.now(), performanceLog: () => process.env.ERPX_PERF_LOG === '1', errorStatus: error => error instanceof ApiError ? error.status : undefined, info: value => console.info(value), warn: (...values) => console.warn(...values), cached: (actor, load) => process.env.ERPX_NATIVE_FIELD_SERVICE_WORK_CONFIG !== undefined || process.env.ERPX_NATIVE_FINANCE_SERVICE_INVOICE_CONFIG !== undefined || actor.modules.includes('field_service') ? load() : cachedWorkSnapshot(actor, write, load)
    };
}
export async function readWorkSnapshot(actor: WorkContext): Promise<WorkSnapshot> { const { write } = await holdingResources(actor.holdingId); return createWorkSnapshot(workSnapshotPorts(write)).readWorkSnapshot(actor); }
export async function workSnapshot(actor: WorkContext): Promise<WorkSnapshot> { const { write } = await holdingResources(actor.holdingId); const load=async () => { const measured = await measureReads(() => withPolicyReadSnapshot(() => createWorkSnapshot(workSnapshotPorts(write)).readWorkSnapshot(actor))); if (process.env.ERPX_PERF_LOG === '1') console.info(JSON.stringify({ event: "work_snapshot", ...measured.metrics, items: measured.result.items.length })); return measured.result; }; return process.env.ERPX_NATIVE_FIELD_SERVICE_WORK_CONFIG === undefined && process.env.ERPX_NATIVE_FINANCE_SERVICE_INVOICE_CONFIG === undefined && !actor.modules.includes('field_service') ? cachedWorkSnapshot(actor,write,load) : load(); }
export async function workItems(actor: WorkContext) { return (await workSnapshot(actor)).items; }

function availabilityFromSnapshot(snapshot: WorkSnapshot): NativeWorkSnapshotAvailability {
    return {
        items: snapshot.items.map((item): NativeWorkSnapshotItem => ({
            id: item.id,
            type: item.type,
            queue: item.queue,
            canAct: item.canAct ?? null,
            actionKind: item.actionKind ?? null,
            relationships: item.relationships ? [...item.relationships] : [],
            updatedAt: item.updatedAt,
            sourceId: null,
            href: item.href,
        })),
        unavailableSources: [...snapshot.unavailableSources],
    };
}

/**
 * Request-scoped availability consumer for the native Work snapshot.
 *
 * The native route returns only owner availability facts, so callers that need
 * `title`/`detail`/`status`/`flow` keep `workSnapshot`. When
 * `ERPX_NATIVE_WORK_PROJECTION_CONFIG` is set this consumer never falls back to
 * the retained TS SQL read: configuration, transport, context and contract
 * failures surface as named `ApiError`s.
 */
export async function workSnapshotAvailability(actor: WorkContext): Promise<NativeWorkSnapshotAvailability> {
    if (nativeWorkSnapshotConfigured()) return readNativeWorkSnapshot(await headers(), actor);
    return availabilityFromSnapshot(await workSnapshot(actor));
}

/**
 * Request-scoped Home unread count for the Work navigation.
 *
 * When `ERPX_NATIVE_WORK_PROJECTION_CONFIG` is set this reads the native owner
 * route; otherwise it keeps the retained `unreadHomeCount` Collaboration read.
 * Presentation and flow facts are not part of this consumer, so navigation and
 * summary continue to use their retained readers.
 */
export async function workHomeUnreadAvailability(actor: WorkContext): Promise<number> {
    if (nativeWorkHomeUnreadConfigured()) return (await nativeWorkHomeUnread(await headers(), actor)).count;
    const { write } = await holdingResources(actor.holdingId);
    return unreadHomeCount(write, actor);
}
