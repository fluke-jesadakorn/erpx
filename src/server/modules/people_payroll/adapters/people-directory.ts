import { requirePermission } from "@/server/modules/identity_access/public/iam-permissions";
import type { SqlDatabase } from "@/server/modules/runtime/public/runtime/types";
import { z } from "zod";
import type { WorkContext } from "@/server/modules/identity_access/public/authorization";
import { holdingResources } from "@/server/modules/runtime/public/runtime";
import { ApiError } from "@/server/modules/runtime/public/http";
import { levelChangeStatements } from "@/server/modules/operations/public/staff-settings";
import { assertion, executeCommand, replayCommand } from "@/server/modules/workflow/public/commands";
import { requireCompanyAuthority, authorityGuard } from "@/server/modules/operations/public/staff-policy";
import { organizationGuard, validateStaffOrganization } from "@/server/modules/organization/public/organization";
import { rosterPositionExitStatements } from "@/server/modules/people_payroll/public/roster-position-exit";
import { recommendationStatements } from "@/server/modules/organization/public/staff-defaults";
import type { SqlStatement } from '../../runtime/public/runtime/types';
import { BusinessRuleError } from '../../shared';
import { directoryUpdateSchema, directoryStatusSchema } from '../domain/people-directory';
import { createPeopleDirectoryApplication, type PeopleDirectoryRepository } from '../application/people-directory';
import { peopleDirectoryOrganizationFacts } from '../../organization/public/people-directory-facts';
import { peopleDirectoryIdentityFacts } from '../../identity_access/public/people-directory-facts';
import { peopleDirectoryEmployeeFacts } from '../../operations/public/people-directory-facts';
export { directorySchema, directoryStatusSchema, directoryUpdateSchema, canManagePeople, canReadCompensation } from '../domain/people-directory';
function application(provided?: SqlDatabase) {
    let database = provided!;
    const repository: PeopleDirectoryRepository<SqlStatement> = { readDirectoryOptionsNodes(...args) { return peopleDirectoryOrganizationFacts(database).readDirectoryOptionsNodes(...args); },
        readDirectoryPersonPerson(...args) { return peopleDirectoryOrganizationFacts(database).readDirectoryPersonPerson(...args); },
        guard3(...args) { return peopleDirectoryOrganizationFacts(database).guard3(...args); },
        guard4(...args) { return peopleDirectoryOrganizationFacts(database).guard4(...args); },
        guard5(id: string, value: "manager", managerPersonId: string | null, managerPersonId2: string | null, companyId: string) {
            return assertion(database, id, value, "?::text IS NULL OR EXISTS(SELECT 1 FROM hr_people WHERE id=? AND company_id=? AND status='active' AND merged_into_id IS NULL)", [managerPersonId, managerPersonId2, companyId]);
        },
        readSaveDirectoryPersonAccount(...args) { return peopleDirectoryIdentityFacts(database).readSaveDirectoryPersonAccount(...args); },
        guard7(...args) { return peopleDirectoryIdentityFacts(database).guard7(...args); },
        guard8(commandId: string, value: "engagement-version", engagement: string, value2: number) {
            return assertion(database, commandId, value, "EXISTS(SELECT 1 FROM hr_engagements WHERE id=? AND version=?)", [engagement, value2]);
        },
        guard9(commandId: string, value: "email", companyId: string, email: string, id: string) {
            return assertion(database, commandId, value, "NOT EXISTS(SELECT 1 FROM hr_people WHERE company_id=? AND lower(email)=? AND id!=? AND merged_into_id IS NULL)", [companyId, email, id]);
        },
        guard10(commandId: string, value: "version", id: string, expectedVersion: number) {
            return assertion(database, commandId, value, "EXISTS(SELECT 1 FROM hr_people WHERE id=? AND version=? AND status!='archived')", [id, expectedVersion]);
        },
        stageSaveDirectoryPersonEffect(legalName: string, value: string | null, email: string, value2: string | null, value3: string | number | null, value4: string | number | null, countryCode: string, now: number, id: string) {
            return database.prepare("UPDATE hr_people SET legal_name=?,preferred_name=?,email=?,phone=?,work_email=?,work_phone=?,country_code=?,version=version+1,updated_at=? WHERE id=?")
                .bind(legalName, value, email, value2, value3, value4, countryCode, now, id);
        },
        stageSaveDirectoryPersonEffect2(branchId: string, departmentId: string | null, jobTitle: string, workerType: "employee" | "contractor", employmentType: "shift" | "contractor" | "salaried" | "daily" | "hourly", startDate: string, currency: "USD" | "EUR" | "GBP" | "THB" | "SGD" | "CAD", managerPersonId: string | null, now: number, engagement: string) {
            return database.prepare("UPDATE hr_engagements SET branch_id=?,department_id=?,job_title=?,worker_type=?,employment_type=?,start_date=?,currency=?,manager_person_id=?,version=version+1,updated_at=? WHERE id=?")
                .bind(branchId, departmentId, jobTitle, workerType, employmentType, startDate, currency, managerPersonId, now, engagement);
        },
        stageSaveDirectoryPersonEffect3(...args) { return peopleDirectoryEmployeeFacts(database).stageSaveDirectoryPersonEffect3(...args); },
        stageSaveDirectoryPersonEffect4(id: string, companyId: string, legalName: string, value: string | null, email: string, value2: string | null, value3: string | null, value4: string | null, countryCode: string, userId: string, now: number, now2: number) {
            return database.prepare("INSERT INTO hr_people(id,company_id,legal_name,preferred_name,email,phone,work_email,work_phone,country_code,created_by,created_at,updated_at) VALUES(?,?,?,?,?,?,?,?,?,?,?,?)")
                .bind(id, companyId, legalName, value, email, value2, value3, value4, countryCode, userId, now, now2);
        },
        stageSaveDirectoryPersonEffect5(engagement: string, id: string, companyId: string, branchId: string, departmentId: string | null, workerType: "employee" | "contractor", employmentType: "shift" | "contractor" | "salaried" | "daily" | "hourly", startDate: string, jobTitle: string, currency: "USD" | "EUR" | "GBP" | "THB" | "SGD" | "CAD", managerPersonId: string | null, userId: string, now: number, now2: number) {
            return database.prepare("INSERT INTO hr_engagements(id,person_id,company_id,branch_id,department_id,worker_type,employment_type,status,start_date,job_title,currency,manager_person_id,created_by,created_at,updated_at) VALUES(?,?,?,?,?,?,?,'onboarding',?,?,?,?,?,?,?)")
                .bind(engagement, id, companyId, branchId, departmentId, workerType, employmentType, startDate, jobTitle, currency, managerPersonId, userId, now, now2);
        },
        stageSaveDirectoryPersonEffect6(...args) { return peopleDirectoryEmployeeFacts(database).stageSaveDirectoryPersonEffect6(...args); },
        stageSaveDirectoryPersonEffect7(levelId: string, engagement: string) {
            return database.prepare("UPDATE hr_engagements SET level_id=? WHERE id=?")
                .bind(levelId, engagement);
        },
        stageSaveDirectoryPersonEffect8(linkedMemberUserId: string, id: string) {
            return database.prepare("UPDATE hr_people SET user_id=? WHERE id=?")
                .bind(linkedMemberUserId, id);
        },
        stageSaveDirectoryPersonEffect9(...args) { return peopleDirectoryEmployeeFacts(database).stageSaveDirectoryPersonEffect9(...args); },
        stageSaveDirectoryPersonEffect10(positionId: string | null, engagement: string) { return database.prepare("UPDATE hr_engagements SET position_id=? WHERE id=?").bind(positionId, engagement); },
        stageSaveDirectoryPersonEffect11(value: string, companyId: string, engagement: string, value2: string, value3: string, userId: string, reason: string, commandId: string, now: number) { return database.prepare("INSERT INTO staff_organization_history(id,company_id,engagement_id,before_json,after_json,actor_id,reason,command_id,created_at) VALUES(?,?,?,?,?,?,?,?,?)").bind(value, companyId, engagement, value2, value3, userId, reason, commandId, now); },
        readSaveDirectoryPersonPrevious(...args) { return peopleDirectoryOrganizationFacts(database).readSaveDirectoryPersonPrevious(...args); },
        readSaveDirectoryPersonNext(...args) { return peopleDirectoryOrganizationFacts(database).readSaveDirectoryPersonNext(...args); },
        readSetDirectoryStatusEffect(companyId: string, personId: string) { return database.prepare("SELECT 1 FROM people_access_cutoffs WHERE company_id=? AND person_id=? AND cancelled_at IS NULL AND restored_at IS NULL").bind(companyId, personId).first(); },
        guard25(commandId: string, value: "ended-engagements", companyId: string, personId: string) { return assertion(database, commandId, value, "NOT EXISTS(SELECT 1 FROM hr_engagements WHERE company_id=? AND person_id=? AND status!='ended')", [companyId, personId]); },
        guard26(commandId: string, value: "version", personId: string, expectedVersion: number) {
            return assertion(database, commandId, value, "EXISTS(SELECT 1 FROM hr_people WHERE id=? AND version=?)", [personId, expectedVersion]);
        },
        guard27(...args) { return peopleDirectoryOrganizationFacts(database).guard27(...args); },
        stageSetDirectoryStatusEffect(status: "active" | "inactive" | "archived", now: number, personId: string) {
            return database.prepare("UPDATE hr_people SET status=?,version=version+1,updated_at=? WHERE id=?")
                .bind(status, now, personId);
        },
        stageSetDirectoryStatusEffect2(value: "active" | "suspended", now: number, personId: string) {
            return database.prepare("UPDATE hr_engagements SET status=?,version=version+1,updated_at=? WHERE person_id=? AND status!='ended'")
                .bind(value, now, personId);
        },
        stageSetDirectoryStatusEffect3(...args) { return peopleDirectoryEmployeeFacts(database).stageSetDirectoryStatusEffect3(...args); },
        stageSetDirectoryStatusEffect4(...args) { return peopleDirectoryIdentityFacts(database).stageSetDirectoryStatusEffect4(...args); },
        readPersonAccessAccount(...args) { return peopleDirectoryIdentityFacts(database).readPersonAccessAccount(...args); },
        readPersonAccessAssignments(...args) { return peopleDirectoryIdentityFacts(database).readPersonAccessAssignments(...args); } };
    return createPeopleDirectoryApplication({ repository, clock: { now: Date.now }, authorize: requirePermission, prepare: async (holding) => {
            if (!provided)
                database = (await holdingResources(holding)).write;
        }, commands: { replay: (...args) => replayCommand(database, ...args), commit: (...args) => executeCommand(database, ...args) }, levelChangeStatements: (...args) => levelChangeStatements(database, ...args), requireCompanyAuthority: (...args) => fromNative(() => requireCompanyAuthority(database, ...args)), authorityGuard: (...args) => authorityGuard(database, ...args), organizationGuard: (...args) => fromNative(() => organizationGuard(database, ...args)), validateStaffOrganization: (...args) => fromNative(() => validateStaffOrganization(database, ...args)), rosterPositionExitStatements: (...args) => rosterPositionExitStatements(database, ...args), recommendationStatements: (...args) => fromNative(() => recommendationStatements(database, ...args)) });
}
async function fromNative<Result>(run: () => Promise<Result> | Result): Promise<Result> {
    try {
        return await run();
    }
    catch (error) {
        if (error instanceof ApiError)
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
            throw new ApiError(error.status, error.code, error.message);
        throw error;
    }
}
export async function directoryOptions(actor: WorkContext) { return invoke(() => application().directoryOptions(actor)); }
export async function directoryPerson(actor: WorkContext, personId: string, engagementId?: string, provided?: SqlDatabase) { return invoke(() => application(provided).directoryPerson(actor, personId, engagementId)); }
export async function saveDirectoryPerson(actor: WorkContext, personId: string | null, input: z.infer<typeof directoryUpdateSchema>, commandId: string) { return invoke(() => application().saveDirectoryPerson(actor, personId, input, commandId)); }
export async function setDirectoryStatus(actor: WorkContext, personId: string, input: z.infer<typeof directoryStatusSchema>, commandId: string) { return invoke(() => application().setDirectoryStatus(actor, personId, input, commandId)); }
export async function personAccess(actor: WorkContext, personId: string) { return invoke(() => application().personAccess(actor, personId)); }
