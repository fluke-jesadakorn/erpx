

import { EmployeeRequestsSupportOwnedFacts } from '@erpx/server-support/public/records-employee-requests-owned-facts';
import { EmployeeRequestsFilesOwnedFacts } from '@erpx/server-files/public/records-employee-requests-owned-facts';

import type { SqlDatabase, SqlStatement } from "@erpx/server-runtime/public/runtime/types";

import type { WorkContext } from "@erpx/server-identity_access/public/authorization";

import { resourceRequestRoute, spendingStepCandidates } from "@erpx/server-people_payroll/public/expense-routing";
import { departmentStepEligible } from '@erpx/server-organization/public/department-authority-reads';
import { canOverrideStaffStep } from '@erpx/server-workflow/public/approval-planning';
import { inServiceWorkspace, serviceMember, serviceOwner } from "@erpx/server-support/public/service-access";
import { teamHandlers } from '@erpx/server-support/public/service-team-reads';

import { ApiError as NativeApiError } from '@erpx/server-runtime/public/http';
import { type EmployeeRequest } from '@erpx/server-records/domain/employee-requests';

import { BusinessRuleError } from '@erpx/server-shared/index';
import { requestActorNameSql } from '@erpx/server-people_payroll/public/records-request-person-projection';

import{createEmployeeRequestRead,type createEmployeeRequestReadPorts}from '@erpx/server-records/application/employee-requests';

export function nativeEmployeeRequestReadRepository(database:SqlDatabase):createEmployeeRequestReadPorts<SqlStatement>['repository']{return{readReadRequestForOverrideRow(id: string, companyId: string) { return database.prepare("SELECT * FROM employee_requests WHERE id=? AND company_id=?").bind(id, companyId).first<EmployeeRequest>(); },readReadEmployeeRequestRow(id: string, companyId: string) {
            return database.prepare("SELECT * FROM employee_requests WHERE id=? AND company_id=?")
                .bind(id, companyId)
                .first<EmployeeRequest>();
        },readRequestAttachmentsEffect(...args) { return EmployeeRequestsFilesOwnedFacts(database).readRequestAttachmentsEffect(...args); },readEmployeeRequestDetailEvents(id: string, companyId: string) {
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
        },readEmployeeRequestDetailTeam(...args) { return EmployeeRequestsSupportOwnedFacts(database).readEmployeeRequestDetailTeam(...args); },readEmployeeRequestDetailChildren(id: string, companyId: string) {
            return database.prepare("SELECT id,title,kind,state FROM employee_requests WHERE parent_request_id=? AND company_id=? ORDER BY created_at LIMIT 50")
                .bind(id, companyId)
                .all<{
                id: string;
                title: string;
                kind: string;
                state: string;
            }>();
        }};}
function application(provided?: SqlDatabase){const database=provided!;return createEmployeeRequestRead({repository:{readReadRequestForOverrideRow:(...args)=>nativeEmployeeRequestReadRepository(database).readReadRequestForOverrideRow(...args),readReadEmployeeRequestRow:(...args)=>nativeEmployeeRequestReadRepository(database).readReadEmployeeRequestRow(...args),readRequestAttachmentsEffect:(...args)=>nativeEmployeeRequestReadRepository(database).readRequestAttachmentsEffect(...args),readEmployeeRequestDetailEvents:(...args)=>nativeEmployeeRequestReadRepository(database).readEmployeeRequestDetailEvents(...args),readEmployeeRequestDetailTeam:(...args)=>nativeEmployeeRequestReadRepository(database).readEmployeeRequestDetailTeam(...args),readEmployeeRequestDetailChildren:(...args)=>nativeEmployeeRequestReadRepository(database).readEmployeeRequestDetailChildren(...args)},resourceRequestRoute: (...args) => fromNative(() => resourceRequestRoute(database, ...args)),spendingStepCandidates: (...args) => fromNative(() => spendingStepCandidates(database, ...args)),departmentStepEligible: (...args) => fromNative(() => departmentStepEligible(database, ...args)),canOverrideStaffStep: (...args) => fromNative(() => canOverrideStaffStep(database, ...args)),inServiceWorkspace: (...args) => inServiceWorkspace(...args),serviceMember: (...args) => fromNative(() => serviceMember(database, ...args)),serviceOwner: (...args) => fromNative(() => serviceOwner(database, ...args)),teamHandlers: (...args) => fromNative(() => teamHandlers(database, ...args))});}
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

export async function getEmployeeRequest(db: SqlDatabase, actor: WorkContext, id: string, override = false) { return invoke(() => application(db).getEmployeeRequest(actor, id, override)); }
export async function requestAttachments(db: SqlDatabase, id: string) { return invoke(() => application(db).requestAttachments(id)); }
export async function employeeRequestDetail(db: SqlDatabase, actor: WorkContext, id: string, includeEvidence = true, override = false) { return invoke(() => application(db).employeeRequestDetail(actor, id, includeEvidence, override)); }
export async function employeeRequestUpload(db: SqlDatabase, actor: WorkContext, id: string, purpose?: string) { return invoke(() => application(db).employeeRequestUpload(actor, id, purpose)); }
