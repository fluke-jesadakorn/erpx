import { EmployeeRequestsCommerceOwnedFacts } from '@erpx/server-commerce/public/records-employee-requests-owned-facts';
import { EmployeeRequestsProcurementOwnedFacts } from '@erpx/server-procurement/public/records-employee-requests-owned-facts';
import { EmployeeRequestsSupportOwnedFacts } from '@erpx/server-support/public/records-employee-requests-owned-facts';
import { EmployeeRequestsFilesOwnedFacts } from '@erpx/server-files/public/records-employee-requests-owned-facts';
import { namedRoute } from "@erpx/server-records/public/request-preview";
import { stageRecordRouteCheck } from "@erpx/server-workflow/public/records-route-checks";
import { requirePermission } from "@erpx/server-identity_access/public/iam-permissions";
import type { SqlDatabase, SqlStatement } from "@erpx/server-runtime/public/runtime/types";
import { z } from "zod";
import type { WorkContext } from "@erpx/server-identity_access/public/authorization";
import { activeStaff, notification } from "@erpx/server-operations/public/staff-policy";
import {assertion} from "@erpx/server-workflow/public/command-assertions";
import { prepareAttachmentBinding } from "@erpx/server-files/public/attachments/service";
import { resourceRequestRoute, expenseRoute, spendingStepCandidates, expenseAuthoritySnapshot } from "@erpx/server-people_payroll/public/expense-routing";
import { departmentRoute, departmentStepEligible } from "@erpx/server-organization/public/department-authority";
import { canOverrideStaffStep, staffOverrideGuards } from "@erpx/server-workflow/public/approvals";
import { companyMember, inServiceWorkspace, serviceAuthoritySnapshot, serviceMember, serviceOwner, validateServiceScope } from "@erpx/server-support/public/service-access";
import { resolveServiceTeam, teamHandlers } from "@erpx/server-support/public/service-teams";
import type { WorkItem } from "@erpx/feature-work_projection/lib/work-item";
import { holdingResources as nativeHoldingResources } from '../../../../packages/server-runtime/src/public/runtime';
import { policyRead as nativePolicyRead, policyNow as nativePolicyNow } from '@erpx/server-identity_access/public/iam-read-snapshot';
import { replayCommand as nativeReplayCommand, executeCommand as nativeExecuteCommand } from '@erpx/server-workflow/public/commands';
import { ApiError as NativeApiError } from '@erpx/server-runtime/public/http';
import { employeeRequestSchema, requestActionSchema, type EmployeeRequest, requestOwn as purerequestOwn } from '@erpx/server-records/domain/employee-requests';
import { createEmployeeRequestsApplication, type EmployeeRequestsRepository } from '@erpx/server-records/application/employee-requests';
import { BusinessRuleError } from '@erpx/server-shared/index';
import { requestActorNameSql } from '@erpx/server-people_payroll/public/records-request-person-projection';
import { EmployeeRequestsIdentityAccessFacts } from '@erpx/server-identity_access/public/records-employee-requests-facts';
export { employeeRequestSchema } from '@erpx/server-records/domain/employee-requests';
export { requestActionSchema } from '@erpx/server-records/domain/employee-requests';
export type { EmployeeRequest } from '@erpx/server-records/domain/employee-requests';
export type { SavedApproval } from '@erpx/server-records/domain/employee-requests';
function application(provided?: SqlDatabase) {
    let database = provided!;
    const repository: EmployeeRequestsRepository<SqlStatement> = { guard1(key: string, value: "request-version", id: string, company_id: string, version: number, state: string, evidence_version: number) {
            return assertion(database, key, value, "EXISTS(SELECT 1 FROM employee_requests WHERE id=? AND company_id=? AND version=? AND state=? AND evidence_version=?)", [id, company_id, version, state, evidence_version]);
        },
        readReadRequestForOverrideRow(id: string, companyId: string) { return database.prepare("SELECT * FROM employee_requests WHERE id=? AND company_id=?").bind(id, companyId).first<EmployeeRequest>(); },
        readReadEmployeeRequestRow(id: string, companyId: string) {
            return database.prepare("SELECT * FROM employee_requests WHERE id=? AND company_id=?")
                .bind(id, companyId)
                .first<EmployeeRequest>();
        },
        readRequestAttachmentsEffect(...args) { return EmployeeRequestsFilesOwnedFacts(database).readRequestAttachmentsEffect(...args); },
        readEmployeeRequestDetailEvents(id: string, companyId: string) {
            return database.prepare(`SELECT e.id,e.action,e.actor_id,${requestActorNameSql} actor_name,e.note,e.evidence_json,e.request_version,e.created_at FROM employee_request_events e WHERE request_id=? AND company_id=? ORDER BY created_at,id LIMIT 250`)
                .bind(id, companyId)
                .all<{
                id: string;
                action: string;
                actor_id: string;
                actor_name: string;
                note: string;
                evidence_json: string;
                request_version: number;
                created_at: number;
            }>();
        },
        readEmployeeRequestDetailTeam(...args) { return EmployeeRequestsSupportOwnedFacts(database).readEmployeeRequestDetailTeam(...args); },
        readEmployeeRequestDetailChildren(id: string, companyId: string) {
            return database.prepare("SELECT id,title,kind,state FROM employee_requests WHERE parent_request_id=? AND company_id=? ORDER BY created_at LIMIT 50")
                .bind(id, companyId)
                .all<{
                id: string;
                title: string;
                kind: string;
                state: string;
            }>();
        },
        stageEventEffect(value: string, company_id: string, id: string, version: number, action: string, userId: string, note: string, value2: string, now: number) {
            return database.prepare("INSERT INTO employee_request_events(id,company_id,request_id,request_version,action,actor_id,note,evidence_json,created_at) VALUES(?,?,?,?,?,?,?,?,?)")
                .bind(value, company_id, id, version, action, userId, note, value2, now);
        },
        stageSaveEmployeeRequestStatements(values: (string | null)[], version: number, now: number, recordId: string, companyId: string) {
            return database.prepare("UPDATE employee_requests SET branch_id=?,department_id=?,category=?,title=?,description=?,location=?,impact=?,needed_date=?,items_json=?,currency=?,state='draft',blocked_reason=NULL,approval_json='{}',version=?,updated_at=? WHERE id=? AND company_id=?")
                .bind(...values, version, now, recordId, companyId);
        },
        stageSaveEmployeeRequestStatements2(recordId: string, companyId: string, value: string | null, values: (string | null)[], kind: "service" | "equipment" | "purchase", requester: string, userId: string, parentRequestId: string | null, version: number, now: number, now2: number) {
            return database.prepare("INSERT INTO employee_requests(id,company_id,engagement_id,branch_id,department_id,category,title,description,location,impact,needed_date,items_json,currency,kind,requester_user_id,created_by,parent_request_id,version,created_at,updated_at) VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?)")
                .bind(recordId, companyId, value, ...values, kind, requester, userId, parentRequestId, version, now, now2);
        },
        stageActOnEmployeeRequestEffect(engagement_id: string, id: string, company_id: string) { return database.prepare('UPDATE employee_requests SET engagement_id=? WHERE id=? AND company_id=?').bind(engagement_id, id, company_id); },
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
            return database.prepare("UPDATE employee_requests SET state=?,team_id=?,assignee_user_id=?,routing_json=?,approval_json=?,blocked_reason=?,resolution=?,purchase_order_id=?,handover_json=?,version=?,updated_at=? WHERE id=? AND company_id=?")
                .bind(state, team_id, assignee_user_id, routing_json, approval_json, blocked_reason, resolution, purchase_order_id, handover_json, version, updated_at, id, company_id);
        },
        readListEmployeeRequestsRows(...args) { return EmployeeRequestsIdentityAccessFacts(database).readListEmployeeRequestsRows(...args); },
        readPrepareRequestOrderHandoffQuotation(...args) { return EmployeeRequestsProcurementOwnedFacts(database).readPrepareRequestOrderHandoffQuotation(...args); },
        readPrepareRequestOrderHandoffActiveQuotationCount(...args) { return EmployeeRequestsProcurementOwnedFacts(database).readPrepareRequestOrderHandoffActiveQuotationCount(...args); },
        readPrepareRequestOrderHandoffExistingLink(...args) { return EmployeeRequestsProcurementOwnedFacts(database).readPrepareRequestOrderHandoffExistingLink(...args); },
        stagePrepareRequestOrderHandoffEffect(orderId: string, now: number, id: string, company_id: string) { return database.prepare("UPDATE employee_requests SET purchase_order_id=COALESCE(purchase_order_id,?),version=version+1,updated_at=? WHERE id=? AND company_id=?").bind(orderId, now, id, company_id); },
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
export async function canOverrideEmployeeRequest(db: SqlDatabase, actor: Pick<WorkContext, "companyId" | "userId" | "role" | "assignmentId">, row: EmployeeRequest) { return invoke(() => application(db).canOverrideEmployeeRequest(actor, row)); }
export async function readRequestForOverride(db: SqlDatabase, companyId: string, id: string) { return invoke(() => application(db).readRequestForOverride(companyId, id)); }
export async function getEmployeeRequest(db: SqlDatabase, actor: WorkContext, id: string, override = false) { return invoke(() => application(db).getEmployeeRequest(actor, id, override)); }
export async function requestAttachments(db: SqlDatabase, id: string) { return invoke(() => application(db).requestAttachments(id)); }
export async function employeeRequestDetail(db: SqlDatabase, actor: WorkContext, id: string, includeEvidence = true, override = false) { return invoke(() => application(db).employeeRequestDetail(actor, id, includeEvidence, override)); }
export async function saveEmployeeRequest(db: SqlDatabase, actor: WorkContext, id: string | null, input: z.infer<typeof employeeRequestSchema>, key: string) { return invoke(() => application(db).saveEmployeeRequest(actor, id, input, key)); }
export async function actOnEmployeeRequest(db: SqlDatabase, actor: WorkContext, id: string, input: z.infer<typeof requestActionSchema>, key: string) { return invoke(() => application(db).actOnEmployeeRequest(actor, id, input, key)); }
export async function listEmployeeRequests(db: SqlDatabase, actor: WorkContext) { return invoke(() => application(db).listEmployeeRequests(actor)); }
export async function employeeRequestWork(db: SqlDatabase, actor: WorkContext): Promise<WorkItem[]> { return invoke(() => application(db).employeeRequestWork(actor)); }
export async function employeeRequestUpload(db: SqlDatabase, actor: WorkContext, id: string, purpose?: string) { return invoke(() => application(db).employeeRequestUpload(actor, id, purpose)); }
export async function prepareRequestOrderHandoff(db: SqlDatabase, actor: WorkContext, requestId: string, orderId: string, key: string, expectedVersion?: number, editing = false, rfqId?: string) { return invoke(() => application(db).prepareRequestOrderHandoff(actor, requestId, orderId, key, expectedVersion, editing, rfqId)); }
export async function requestQuotationSource(db: SqlDatabase, actor: WorkContext, id: string) { return invoke(() => application(db).requestQuotationSource(actor, id)); }
export async function previewEmployeeRequest(db: SqlDatabase, actor: WorkContext, input: z.infer<typeof employeeRequestSchema>, id?: string) { return invoke(() => application(db).previewEmployeeRequest(actor, input, id)); }
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
