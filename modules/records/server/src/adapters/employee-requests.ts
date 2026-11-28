import{nativeEmployeeRequestReadRepository,getEmployeeRequest,requestAttachments,employeeRequestDetail,employeeRequestUpload}from '@erpx/records-server/adapters/employee-requests-read';
export{getEmployeeRequest,requestAttachments,employeeRequestDetail,employeeRequestUpload};
import { EmployeeRequestsCommerceOwnedFacts } from '@erpx/commerce-server/public/records-employee-requests-owned-facts';
import { EmployeeRequestsProcurementOwnedFacts } from '@erpx/procurement-server/public/records-employee-requests-owned-facts';

import { namedRoute } from "@erpx/records-server/public/request-preview";
import { stageRecordRouteCheck } from "@erpx/workflow-server/public/records-route-checks";
import { requirePermission } from "@erpx/identity-access-server/public/iam-permissions";
import type { SqlDatabase, SqlStatement } from "@erpx/platform-runtime-server/public/runtime/types";
import { z } from "zod";
import type { WorkContext } from "@erpx/identity-access-server/public/authorization";
import { activeStaff, notification } from "@erpx/operations-server/public/staff-policy";
import {assertion} from "@erpx/workflow-server/public/command-assertions";
import { ownerVersionExistsSql } from './read-tail-sql';
import { prepareAttachmentBinding } from "@erpx/files-server/public/attachments/service";
import { resourceRequestRoute, expenseRoute, spendingStepCandidates, expenseAuthoritySnapshot } from "@erpx/people-payroll-server/public/expense-routing";
import {departmentRoute,departmentStepEligible} from '@erpx/organization-server/public/department-authority-reads';
import {canOverrideStaffStep,staffOverrideGuards} from '@erpx/workflow-server/public/approval-planning';
import { companyMember, inServiceWorkspace, serviceAuthoritySnapshot, serviceMember, serviceOwner, validateServiceScope } from "@erpx/support-server/public/service-access";
import {resolveServiceTeam,teamHandlers} from '@erpx/support-server/public/service-team-reads';
import type { WorkItem } from "@erpx/work-projection-ui/lib/work-item";
import { holdingResources as nativeHoldingResources } from '../../../../../platform/runtime/server/src/public/runtime';
import { policyRead as nativePolicyRead, policyNow as nativePolicyNow } from '@erpx/identity-access-server/public/iam-read-snapshot';
import { replayCommand as nativeReplayCommand, executeCommand as nativeExecuteCommand } from '@erpx/workflow-server/public/commands';
import { ApiError as NativeApiError } from '@erpx/platform-runtime-server/public/http';
import { employeeRequestSchema, requestActionSchema, type EmployeeRequest, requestOwn as purerequestOwn } from '@erpx/records-server/domain/employee-requests';
import { createEmployeeRequestsApplication, type EmployeeRequestsRepository } from '@erpx/records-server/application/employee-requests';
import { BusinessRuleError } from '@erpx/platform-shared-server/index';

import { EmployeeRequestsIdentityAccessFacts } from '@erpx/identity-access-server/public/records-employee-requests-facts';
import { nativeRecordsStatement } from '@erpx/records-ui/adapters/server/native-records-command';
export { employeeRequestSchema } from '@erpx/records-server/domain/employee-requests';
export { requestActionSchema } from '@erpx/records-server/domain/employee-requests';
export type { EmployeeRequest } from '@erpx/records-server/domain/employee-requests';
export type { SavedApproval } from '@erpx/records-server/domain/employee-requests';
function application(provided?: SqlDatabase, actor?: WorkContext) {
    let database = provided!;
    const repository: EmployeeRequestsRepository<SqlStatement> = { guard1(key: string, value: "request-version", id: string, company_id: string, version: number, state: string, evidence_version: number) {
            return assertion(database, key, value, ownerVersionExistsSql('employee_requests', ' AND state=? AND evidence_version=?'), [id, company_id, version, state, evidence_version]);
        },
        readReadRequestForOverrideRow(...args){return nativeEmployeeRequestReadRepository(database).readReadRequestForOverrideRow(...args);},
        readReadEmployeeRequestRow(...args){return nativeEmployeeRequestReadRepository(database).readReadEmployeeRequestRow(...args);},
        readRequestAttachmentsEffect(...args){return nativeEmployeeRequestReadRepository(database).readRequestAttachmentsEffect(...args);},
        readEmployeeRequestDetailEvents(...args){return nativeEmployeeRequestReadRepository(database).readEmployeeRequestDetailEvents(...args);},
        readEmployeeRequestDetailTeam(...args){return nativeEmployeeRequestReadRepository(database).readEmployeeRequestDetailTeam(...args);},
        readEmployeeRequestDetailChildren(...args){return nativeEmployeeRequestReadRepository(database).readEmployeeRequestDetailChildren(...args);},
        stageEventEffect(value: string, company_id: string, id: string, version: number, action: string, userId: string, note: string, value2: string, now: number) {
            return nativeRecordsStatement('records_stmt_12', { companyId: actor?.companyId, actor, now: Date.now() })
                .bind(value, company_id, id, version, action, userId, note, value2, now);
        },
        stageSaveEmployeeRequestStatements(values: (string | null)[], version: number, now: number, recordId: string, companyId: string) {
            return nativeRecordsStatement('records_stmt_13', { companyId: actor?.companyId, actor, now: Date.now() })
                .bind(...values, version, now, recordId, companyId);
        },
        stageSaveEmployeeRequestStatements2(recordId: string, companyId: string, value: string | null, values: (string | null)[], kind: "service" | "equipment" | "purchase", requester: string, userId: string, parentRequestId: string | null, version: number, now: number, now2: number) {
            return nativeRecordsStatement('records_stmt_14', { companyId: actor?.companyId, actor, now: Date.now() })
                .bind(recordId, companyId, value, ...values, kind, requester, userId, parentRequestId, version, now, now2);
        },
        stageActOnEmployeeRequestEffect(engagement_id: string, id: string, company_id: string) { return nativeRecordsStatement('records_stmt_15', { companyId: actor?.companyId, actor, now: Date.now() }).bind(engagement_id, id, company_id); },
        readActOnEmployeeRequestAllocations(...args) { return EmployeeRequestsProcurementOwnedFacts(database).readActOnEmployeeRequestAllocations(...args); },
        readActOnEmployeeRequestOrders(...args) { return EmployeeRequestsCommerceOwnedFacts(database).readActOnEmployeeRequestOrders(...args); },
        guard15(...args) { return EmployeeRequestsCommerceOwnedFacts(database).guard15(...args); },
        readActOnEmployeeRequestQuotationCount(...args) { return EmployeeRequestsProcurementOwnedFacts(database).readActOnEmployeeRequestQuotationCount(...args); },
        readActOnEmployeeRequestQuotation(...args) { return EmployeeRequestsProcurementOwnedFacts(database).readActOnEmployeeRequestQuotation(...args); },
        readActOnEmployeeRequestOrder(...args) { return EmployeeRequestsCommerceOwnedFacts(database).readActOnEmployeeRequestOrder(...args); },
        guard20(...args) { return EmployeeRequestsCommerceOwnedFacts(database).guard20(...args); },
        readActOnEmployeeRequestLinked(...args) { return EmployeeRequestsProcurementOwnedFacts(database).readActOnEmployeeRequestLinked(...args); },
        stageActOnEmployeeRequestEffect2(...args) { return EmployeeRequestsProcurementOwnedFacts(database).stageActOnEmployeeRequestEffect2(...args); },
        readActOnEmployeeRequestEffect(...args) { return EmployeeRequestsIdentityAccessFacts(database).readActOnEmployeeRequestEffect(...args); },
        stageActOnEmployeeRequestEffect3(state: string, team_id: string | null, assignee_user_id: string | null, routing_json: string, approval_json: string, blocked_reason: string | null, resolution: string, purchase_order_id: string | null, handover_json: string, version: number, updated_at: number, id: string, company_id: string) {
            return nativeRecordsStatement('records_stmt_16', { companyId: actor?.companyId, actor, now: Date.now() })
                .bind(state, team_id, assignee_user_id, routing_json, approval_json, blocked_reason, resolution, purchase_order_id, handover_json, version, updated_at, id, company_id);
        },
        readListEmployeeRequestsRows(...args) { return EmployeeRequestsIdentityAccessFacts(database).readListEmployeeRequestsRows(...args); },
        readPrepareRequestOrderHandoffQuotation(...args) { return EmployeeRequestsProcurementOwnedFacts(database).readPrepareRequestOrderHandoffQuotation(...args); },
        readPrepareRequestOrderHandoffActiveQuotationCount(...args) { return EmployeeRequestsProcurementOwnedFacts(database).readPrepareRequestOrderHandoffActiveQuotationCount(...args); },
        readPrepareRequestOrderHandoffExistingLink(...args) { return EmployeeRequestsProcurementOwnedFacts(database).readPrepareRequestOrderHandoffExistingLink(...args); },
        stagePrepareRequestOrderHandoffEffect(orderId: string, now: number, id: string, company_id: string) { return nativeRecordsStatement('records_stmt_17', { companyId: actor?.companyId, actor, now: Date.now() }).bind(orderId, now, id, company_id); },
        stagePrepareRequestOrderHandoffEffect2(...args) { return EmployeeRequestsProcurementOwnedFacts(database).stagePrepareRequestOrderHandoffEffect2(...args); },
        readRequestQuotationSourceAllocated(...args) { return EmployeeRequestsProcurementOwnedFacts(database).readRequestQuotationSourceAllocated(...args); } };
    return createEmployeeRequestsApplication({ repository, clock: { now: Date.now }, identifiers: { next: () => crypto.randomUUID() }, commands: { replay: (...args) => nativeReplayCommand(database, ...args), commit: (...args) => nativeExecuteCommand(database, ...args) }, prepare: async (holding) => {
            if (!provided)
                database = (await nativeHoldingResources(holding)).write;
        }, production: process.env.NODE_ENV === 'production', batch: effects => database.batch(effects), policyRead: (key, read) => nativePolicyRead(database, key, read), policyNow: nativePolicyNow, namedRoute: (...args) => fromNative(() => namedRoute(database, ...args)), stageRouteCheck: (key, step, check) => stageRecordRouteCheck(database, key, step, check), requirePermission: (actor, permission) => fromNativeSync(() => requirePermission(actor, permission)), activeStaff: (...args) => fromNative(() => activeStaff(database, ...args)), notification: (...args) => notification(database, ...args), prepareAttachmentBinding: (...args) => fromNative(() => prepareAttachmentBinding(database, ...args)), resourceRequestRoute: (...args) => fromNative(() => resourceRequestRoute(database, ...args)), expenseRoute: (...args) => fromNative(() => expenseRoute(database, ...args)), spendingStepCandidates: (...args) => fromNative(() => spendingStepCandidates(database, ...args)), expenseAuthoritySnapshot: (...args) => fromNative(() => expenseAuthoritySnapshot(database, ...args)), departmentRoute: (...args) => fromNative(() => departmentRoute(database, ...args)), departmentStepEligible: (...args) => fromNative(() => departmentStepEligible(database, ...args)), canOverrideStaffStep: (...args) => fromNative(() => canOverrideStaffStep(database, ...args)), staffOverrideGuards: (...args) => fromNative(() => staffOverrideGuards(database, ...args)), companyMember: (...args) => fromNative(() => companyMember(database, ...args)), inServiceWorkspace: (...args) => inServiceWorkspace(...args), serviceAuthoritySnapshot: (...args) => fromNative(() => serviceAuthoritySnapshot(database, ...args)), serviceMember: (...args) => fromNative(() => serviceMember(database, ...args)), serviceOwner: (...args) => fromNative(() => serviceOwner(database, ...args)), validateServiceScope: (...args) => fromNative(() => validateServiceScope(database, ...args)), resolveServiceTeam: (...args) => fromNative(() => resolveServiceTeam(database, ...args)), teamHandlers: (...args) => fromNative(() => teamHandlers(database, ...args)) });
}
async function fromNative<Result>(run: () => Promise<Result> | Result): Promise<Result> {
    try {
        return await run();
    }
    catch (error) {
        if (error instanceof NativeApiError)
            throw new BusinessRuleError(error.status, error.code, error.message);
        throw error;
    }
}
async function invoke<Result>(run: () => Promise<Result>) {
    try {
        return await run();
    }
    catch (error) {
        if (error instanceof BusinessRuleError)
            throw new NativeApiError(error.status, error.code, error.message);
        throw error;
    }
}
function invokeSync<Result>(run: () => Result) {
    try {
        return run();
    }
    catch (error) {
        if (error instanceof BusinessRuleError)
            throw new NativeApiError(error.status, error.code, error.message);
        throw error;
    }
}
export function requestVersionGuard(db: SqlDatabase, key: string, row: EmployeeRequest) { return invokeSync(() => application(db).requestVersionGuard(key, row)); }
export function requestOwn(actor: WorkContext, row: EmployeeRequest) { return invokeSync(() => purerequestOwn(actor, row)); }

export async function saveEmployeeRequest(db: SqlDatabase, actor: WorkContext, id: string | null, input: z.infer<typeof employeeRequestSchema>, key: string) { return invoke(() => application(db, actor).saveEmployeeRequest(actor, id, input, key)); }
export async function actOnEmployeeRequest(db: SqlDatabase, actor: WorkContext, id: string, input: z.infer<typeof requestActionSchema>, key: string) { return invoke(() => application(db, actor).actOnEmployeeRequest(actor, id, input, key)); }
export async function listEmployeeRequests(db: SqlDatabase, actor: WorkContext) { return invoke(() => application(db, actor).listEmployeeRequests(actor)); }
export async function employeeRequestWork(db: SqlDatabase, actor: WorkContext): Promise<WorkItem[]> { return invoke(() => application(db, actor).employeeRequestWork(actor)); }

export async function prepareRequestOrderHandoff(db: SqlDatabase, actor: WorkContext, requestId: string, orderId: string, key: string, expectedVersion?: number, editing = false, rfqId?: string) { return invoke(() => application(db, actor).prepareRequestOrderHandoff(actor, requestId, orderId, key, expectedVersion, editing, rfqId)); }
export async function requestQuotationSource(db: SqlDatabase, actor: WorkContext, id: string) { return invoke(() => application(db, actor).requestQuotationSource(actor, id)); }
export async function previewEmployeeRequest(db: SqlDatabase, actor: WorkContext, input: z.infer<typeof employeeRequestSchema>, id?: string) { return invoke(() => application(db, actor).previewEmployeeRequest(actor, input, id)); }
function fromNativeSync<Result>(run: () => Result) {
    try {
        return run();
    }
    catch (error) {
        if (error instanceof NativeApiError)
            throw new BusinessRuleError(error.status, error.code, error.message);
        throw error;
    }
}
