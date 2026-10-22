import {operationsLevelHistoryEffects} from "@erpx/server-organization/public/operations-level-history-effects";
import { organizationGuard, validateStaffOrganization } from "@erpx/server-organization/public/organization";
import { recommendationStatements } from "@erpx/server-organization/public/staff-defaults";
import type { SqlDatabase } from "@erpx/server-runtime/public/runtime/types";
import { z } from "zod";
import { activeStaff, authorityGuard, companyAuthority, requireCompanyAuthority, staffPeople, staffGuard, type StaffActor } from "@erpx/server-operations/public/staff-policy";
import { holdingResources as nativeHoldingResources } from '../../../../packages/server-runtime/src/public/runtime';
import { policyRead as nativePolicyRead, policyNow as nativePolicyNow } from '@erpx/server-identity_access/public/iam-read-snapshot';
import { replayCommand as nativeReplayCommand, executeCommand as nativeExecuteCommand } from '@erpx/server-workflow/public/commands';
import { ApiError as NativeApiError } from '@erpx/server-runtime/public/http';
import { levelSchema, levelAssignmentSchema, headSchema } from '@erpx/server-operations/domain/staff-settings';
import { createStaffSettingsApplication, type StaffSettingsRepository } from '@erpx/server-operations/application/staff-settings';
import { BusinessRuleError } from '@erpx/server-shared/index';
import type { SqlStatement } from '@erpx/server-runtime/public/runtime/types';
import { StaffSettingsOrganizationFacts } from '@erpx/server-organization/public/operations-staff-settings-facts';
import { StaffSettingsPeoplePayrollFacts } from '@erpx/server-people_payroll/public/operations-staff-settings-facts';
export { levelSchema } from '@erpx/server-operations/domain/staff-settings';
export { levelAssignmentSchema } from '@erpx/server-operations/domain/staff-settings';
export { headSchema } from '@erpx/server-operations/domain/staff-settings';
function application(provided?: SqlDatabase) {
    let database = provided!;
    const repository: StaffSettingsRepository<SqlStatement> = { readListLevelsEffect(...args) { return StaffSettingsOrganizationFacts(database).readListLevelsEffect(...args); },
        readStaffSettingsLevelspeopledepartmentscanManage(...args) { return StaffSettingsPeoplePayrollFacts(database).readStaffSettingsLevelspeopledepartmentscanManage(...args); },
        readSaveLevelPrevious(...args) { return StaffSettingsOrganizationFacts(database).readSaveLevelPrevious(...args); },
        readSaveLevelEffect(...args) { return StaffSettingsPeoplePayrollFacts(database).readSaveLevelEffect(...args); },
        guard5(...args) { return StaffSettingsOrganizationFacts(database).guard5(...args); },
        stageSaveLevelEffect(...args) { return StaffSettingsOrganizationFacts(database).stageSaveLevelEffect(...args); },
        stageSaveLevelEffect2(...args) { return StaffSettingsOrganizationFacts(database).stageSaveLevelEffect2(...args); },
        guard8(...args) { return StaffSettingsOrganizationFacts(database).guard8(...args); },
        stageLevelChangeStatementsEffect(value: string, engagementId: string, previous: string | null, levelId: string, userId: string, reason: string, key: string, now: number) {
            return operationsLevelHistoryEffects(database).stageChange(value, engagementId, previous, levelId, userId, reason, key, now);
        },
        readAssignLevelEngagement(...args) { return StaffSettingsPeoplePayrollFacts(database).readAssignLevelEngagement(...args); },
        guard11(...args) { return StaffSettingsPeoplePayrollFacts(database).guard11(...args); },
        stageAssignLevelEffect(...args) { return StaffSettingsPeoplePayrollFacts(database).stageAssignLevelEffect(...args); },
        readSetDepartmentHeadDepartment(...args) { return StaffSettingsOrganizationFacts(database).readSetDepartmentHeadDepartment(...args); },
        guard14(...args) { return StaffSettingsOrganizationFacts(database).guard14(...args); },
        guard15(...args) { return StaffSettingsOrganizationFacts(database).guard15(...args); },
        stageSetDepartmentHeadEffect(...args) { return StaffSettingsOrganizationFacts(database).stageSetDepartmentHeadEffect(...args); } };
    return createStaffSettingsApplication({ repository, clock: { now: Date.now }, identifiers: { next: () => crypto.randomUUID() }, commands: { replay: (...args) => nativeReplayCommand(database, ...args), commit: (...args) => nativeExecuteCommand(database, ...args) }, prepare: async (holding) => { if (!provided)
            database = (await nativeHoldingResources(holding)).write; }, production: process.env.NODE_ENV === 'production', batch: effects => database.batch(effects), policyRead: (key, read) => nativePolicyRead(database, key, read), policyNow: nativePolicyNow, organizationGuard: (...args) => fromNative(() => organizationGuard(database, ...args)), validateStaffOrganization: (...args) => fromNative(() => validateStaffOrganization(database, ...args)), recommendationStatements: (...args) => fromNative(() => recommendationStatements(database, ...args)), activeStaff: (...args) => fromNative(() => activeStaff(database, ...args)), authorityGuard: (...args) => authorityGuard(database, ...args), companyAuthority: (...args) => fromNative(() => companyAuthority(database, ...args)), requireCompanyAuthority: (...args) => fromNative(() => requireCompanyAuthority(database, ...args)), staffPeople: (...args) => fromNative(() => staffPeople(database, ...args)), staffGuard: (...args) => staffGuard(database, ...args) });
}
async function fromNative<Result>(run: () => Promise<Result> | Result): Promise<Result> { try {
    return await run();
}
catch (error) {
    if (error instanceof NativeApiError)
        throw new BusinessRuleError(error.status, error.code, error.message);
    throw error;
} }
async function invoke<Result>(run: () => Promise<Result>) { try {
    return await run();
}
catch (error) {
    if (error instanceof BusinessRuleError)
        throw new NativeApiError(error.status, error.code, error.message);
    throw error;
} }
function invokeSync<Result>(run: () => Result) { try {
    return run();
}
catch (error) {
    if (error instanceof BusinessRuleError)
        throw new NativeApiError(error.status, error.code, error.message);
    throw error;
} }
export async function listLevels(db: SqlDatabase, companyId: string) { return invoke(() => application(db).listLevels(companyId)); }
export async function staffSettings(db: SqlDatabase, actor: StaffActor) { return invoke(() => application(db).staffSettings(actor)); }
export async function saveLevel(db: SqlDatabase, actor: StaffActor, id: string | null, input: z.infer<typeof levelSchema>, key: string) { return invoke(() => application(db).saveLevel(actor, id, input, key)); }
export function levelChangeStatements(db: SqlDatabase, key: string, actor: StaffActor, engagementId: string, levelId: string, previous: string | null, reason: string) { return invokeSync(() => application(db).levelChangeStatements(key, actor, engagementId, levelId, previous, reason)); }
export async function assignLevel(db: SqlDatabase, actor: StaffActor, id: string, input: z.infer<typeof levelAssignmentSchema>, key: string) { return invoke(() => application(db).assignLevel(actor, id, input, key)); }
export async function setDepartmentHead(db: SqlDatabase, actor: StaffActor, id: string, input: z.infer<typeof headSchema>, key: string) { return invoke(() => application(db).setDepartmentHead(actor, id, input, key)); }
