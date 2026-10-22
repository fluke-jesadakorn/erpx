import { finalRootOwnerProjectionSources as identity_accessProjectionSources } from "@erpx/server-identity_access/public/final-root-owner-projection-sources";
import { finalRootOwnerProjectionSources as organizationProjectionSources } from "@erpx/server-organization/public/final-root-owner-projection-sources";
import { finalRootOwnerProjectionSources as people_payrollProjectionSources } from "@erpx/server-people_payroll/public/final-root-owner-projection-sources";
import type { SqlDatabase } from "@erpx/server-runtime/public/runtime/types";
import { holdingResources as nativeHoldingResources } from '../../../../packages/server-runtime/src/public/runtime';
import { policyRead as nativePolicyRead, policyNow as nativePolicyNow } from '@erpx/server-identity_access/public/iam-read-snapshot';
import { replayCommand as nativeReplayCommand, executeCommand as nativeExecuteCommand } from '@erpx/server-workflow/public/commands';
import { ApiError as NativeApiError } from '@erpx/server-runtime/public/http';
import { type StaffActor, type RecordScope, type StaffPerson, scopeActor as purescopeActor, type ApprovalRoute, reportingChain as purereportingChain } from '@erpx/server-operations/domain/staff-policy';
import { createStaffPolicyApplication, type StaffPolicyRepository } from '@erpx/server-operations/application/staff-policy';
import { BusinessRuleError } from '@erpx/server-shared/index';
import type { SqlStatement } from '@erpx/server-runtime/public/runtime/types';
import { StaffPolicyIdentityAccessFacts } from '@erpx/server-identity_access/public/operations-staff-policy-facts';
import { StaffPolicyOrganizationFacts } from '@erpx/server-organization/public/operations-staff-policy-facts';
import { StaffPolicyCollaborationFacts } from '@erpx/server-collaboration/public/operations-staff-policy-facts';
export type { StaffActor } from '@erpx/server-operations/domain/staff-policy';
export type { RecordScope } from '@erpx/server-operations/domain/staff-policy';
export type { StaffPerson } from '@erpx/server-operations/domain/staff-policy';
export type { RouteStep } from '@erpx/server-operations/domain/staff-policy';
export type { ApprovalRoute } from '@erpx/server-operations/domain/staff-policy';
const staffSelect = (datePredicate: string) => `SELECT p.id person_id,p.user_id,p.legal_name,e.id engagement_id,e.version engagement_version,
  e.company_id,e.branch_id,e.department_id,e.worker_type,e.start_date,e.end_date,e.manager_person_id,e.level_id,l.name level_name,l.rank level_rank,l.level_number,e.position_id,pos.is_ceo,pos.roster_required,e.self_service_top_level
  FROM (${people_payrollProjectionSources.hr_people}) p JOIN (${people_payrollProjectionSources.hr_engagements}) e ON e.person_id=p.id AND e.company_id=p.company_id
  LEFT JOIN (${organizationProjectionSources.staff_levels}) l ON l.id=e.level_id AND l.company_id=e.company_id
  LEFT JOIN (${organizationProjectionSources.organization_positions}) pos ON pos.id=e.position_id AND pos.company_id=e.company_id AND pos.active=1
  WHERE p.company_id=? AND p.status='active' AND p.merged_into_id IS NULL AND p.user_id IS NOT NULL
  AND e.status='active' ${datePredicate}
  AND EXISTS(SELECT 1 FROM (${identity_accessProjectionSources.assignments}) a WHERE a.user_id=p.user_id AND a.company_id=p.company_id
    AND a.revoked_at IS NULL AND iam_assignment_active(a.id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) AND iam_principal_active(a.user_id,a.company_id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) AND a.valid_from<=? AND a.valid_until>?)`;
export const activeStaffSelect = staffSelect("AND e.start_date<=to_char(current_timestamp AT TIME ZONE 'UTC','YYYY-MM-DD') AND (e.end_date IS NULL OR e.end_date>=to_char(current_timestamp AT TIME ZONE 'UTC','YYYY-MM-DD'))");
function application(provided?: SqlDatabase) {
    let database = provided!;
    const repository: StaffPolicyRepository<SqlStatement> = { readStaffPeopleResults(companyId: string, now: number, now2: number, value: string | null, value2: string | null, value3: string | null, value4: string | null, value5: string | null) {
            return database.prepare(`${activeStaffSelect} AND (?::text IS NULL OR e.department_id=?) AND (?::text IS NULL OR (?='unset' AND e.level_id IS NULL) OR e.level_id=?) ORDER BY p.legal_name,e.start_date DESC,e.id`)
                .bind(companyId, now, now2, value, value2, value3, value4, value5)
                .all<StaffPerson>();
        },
        readRosterStaffPeopleResults(companyId: string, lastDate: string, firstDate: string, now: number, now2: number, departmentId: string) {
            const plannedDates = "AND e.start_date::date<=?::date AND (e.end_date IS NULL OR e.end_date::date>=?::date)";
            return database.prepare(`${staffSelect(plannedDates)} AND e.department_id=? ORDER BY p.legal_name,e.start_date DESC,e.id`)
                .bind(companyId, lastDate, firstDate, now, now2, departmentId).all<StaffPerson>();
        },
        readReadActiveStaffResults(...args) { return StaffPolicyIdentityAccessFacts(database).readReadActiveStaffResults(...args); },
        guard4(...args) { return StaffPolicyIdentityAccessFacts(database).guard4(...args); },
        readCompanyAuthorityEffect(...args) { return StaffPolicyIdentityAccessFacts(database).readCompanyAuthorityEffect(...args); },
        guard6(...args) { return StaffPolicyIdentityAccessFacts(database).guard6(...args); },
        readEligibleRoleUsersResults(...args) { return StaffPolicyIdentityAccessFacts(database).readEligibleRoleUsersResults(...args); },
        readRequestRouteEffect(...args) { return StaffPolicyIdentityAccessFacts(database).readRequestRouteEffect(...args); },
        readRequestRouteHead(...args) { return StaffPolicyOrganizationFacts(database).readRequestRouteHead(...args); },
        stageNotificationEffect(...args) { return StaffPolicyCollaborationFacts(database).stageNotificationEffect(...args); } };
    return createStaffPolicyApplication({ repository, clock: { now: Date.now }, identifiers: { next: () => crypto.randomUUID() }, commands: { replay: (...args) => nativeReplayCommand(database, ...args), commit: (...args) => nativeExecuteCommand(database, ...args) }, prepare: async (holding) => {
            if (!provided)
                database = (await nativeHoldingResources(holding)).write;
        }, production: process.env.NODE_ENV === 'production', batch: effects => database.batch(effects), policyRead: (key, read) => nativePolicyRead(database, key, read), policyNow: nativePolicyNow, routeChecks: (people, companyId, head) => [...(head ? [{ query: `EXISTS(SELECT 1 FROM (${organizationProjectionSources.department_heads}) h JOIN (${organizationProjectionSources.organization_nodes}) n ON n.id=h.department_id WHERE h.department_id=? AND h.company_id=? AND h.version=? AND n.status='active')`, args: [head.departmentId, companyId, head.version] }] : []), ...people.map(person => ({ query: `EXISTS(SELECT 1 FROM (${people_payrollProjectionSources.hr_engagements}) hr_engagements WHERE id=? AND company_id=? AND version=? AND status='active')`, args: [person.engagement_id, companyId, person.engagement_version] }))], });
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
export async function staffPeople(db: SqlDatabase, companyId: string, filters: {
    departmentId?: string;
    levelId?: string;
} = {}) { return invoke(() => application(db).staffPeople(companyId, filters)); }
export async function rosterStaffPeople(db: SqlDatabase, companyId: string, departmentId: string, firstDate: string, lastDate: string) { return invoke(() => application(db).rosterStaffPeople(companyId, departmentId, firstDate, lastDate)); }
export async function activeStaff(db: SqlDatabase, companyId: string, userId: string): Promise<StaffPerson> { return invoke(() => application(db).activeStaff(companyId, userId)); }
export function staffGuard(db: SqlDatabase, key: string, companyId: string, userId: string, step = `staff-${userId}`) { return invokeSync(() => application(db).staffGuard(key, companyId, userId, step)); }
export async function companyAuthority(db: SqlDatabase, actor: Pick<StaffActor, "userId" | "companyId" | "assignmentId" | "role">, roles: string[] | string = "organization.manage") { return invoke(() => application(db).companyAuthority(actor, roles)); }
export function authorityGuard(db: SqlDatabase, key: string, actor: Pick<StaffActor, "userId" | "companyId" | "assignmentId" | "role">, roles: string[] | string = "organization.manage") { return invokeSync(() => application(db).authorityGuard(key, actor, roles)); }
export async function requireCompanyAuthority(db: SqlDatabase, actor: StaffActor, roles?: string[] | string) { return invoke(() => application(db).requireCompanyAuthority(actor, roles)); }
export function scopeActor<T extends StaffActor>(actor: T, record: RecordScope): T { return invokeSync(() => purescopeActor(actor, record)); }
export function reportingChain(people: StaffPerson[], personId: string): StaffPerson[] { return invokeSync(() => purereportingChain(people, personId)); }
export async function eligibleRoleUsers(db: SqlDatabase, scope: RecordScope, role: "hr_admin" | "holding_owner") { return invoke(() => application(db).eligibleRoleUsers(scope, role)); }
export async function requestRoute(db: SqlDatabase, companyId: string, userId: string, kind: "expense" | "leave", excluded: string[] = []): Promise<ApprovalRoute> { return invoke(() => application(db).requestRoute(companyId, userId, kind, excluded)); }
export function notification(db: SqlDatabase, scope: RecordScope, userId: string, type: string, id: string, version: number, message: string) { return invokeSync(() => application(db).notification(scope, userId, type, id, version, message)); }
