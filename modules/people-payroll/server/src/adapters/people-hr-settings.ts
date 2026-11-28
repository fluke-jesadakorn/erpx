import {peoplePurchaseControls} from "@erpx/people-payroll-server/adapters/people-purchases";
import {ppRd43ActiveLeavePolicyProjection} from './pp-read-tail-r43-sources';
import {ppRd61HolidayCalendarsByCompanySql} from './pp-read-tail-r61-sources';
import { readHrAffectedPeople, readHrCurrentPolicies, readHrEligibility, readHrEngagement, readHrFutureShifts, readHrPayGroups, type Lane8Actor } from './native-lane-8/client';
import "server-only";
import type { WorkContext } from "@erpx/identity-access-server/public/authorization";
import { ApiError } from "@erpx/platform-runtime-server/public/http";
import { iamActorFromWork } from "@erpx/identity-access-server/public/iam-context";

import {iamOverview} from '@erpx/identity-access-server/public/iam-service-reads';
import { leaveAdministration } from "@erpx/people-payroll-server/adapters/leave-self-service";
import {organizationOverview} from '@erpx/organization-server/public/organization';
import { peopleLifecycleData } from "@erpx/people-payroll-server/adapters/people-lifecycle";
import { directoryOptions } from "@erpx/people-payroll-server/adapters/people-directory";
import { nativePayGroups } from "@erpx/people-payroll-server/adapters/people-payroll";
import { peopleTemplatesData } from "@erpx/people-payroll-server/adapters/people-templates";
import { holdingResources } from "../../../../../platform/runtime/server/src/public/runtime";
import type { PostgresDatabase } from "@erpx/platform-runtime-server/public/runtime/postgres";
import { defaultCatalog } from "@erpx/organization-server/public/staff-defaults";
import { requireCompanyAuthority } from "@erpx/operations-server/public/staff-policy";
import { staffSettings } from "@erpx/operations-server/public/staff-settings";
import type { SqlDatabase } from '@erpx/platform-runtime-server/public/runtime/types';
import { BusinessRuleError } from '@erpx/platform-shared-server/index';
import { createPeopleHrApplication, type PeopleHrRepository } from '@erpx/people-payroll-server/application/people-hr-settings';
import type { PeopleHrTopic } from '@erpx/people-payroll-server/domain/people-hr-settings';
export { peopleHrTopics } from '@erpx/people-payroll-server/domain/people-hr-settings';
export type { PeopleHrTopic } from '@erpx/people-payroll-server/domain/people-hr-settings';
function application() {
    let write: SqlDatabase;
    let lane8Actor: Lane8Actor | undefined;
    const repository: PeopleHrRepository = { holidays: (companyId: string) => write.prepare("SELECT h.id,h.calendar_id,h.holiday_date,h.name FROM leave_holidays h JOIN leave_holiday_calendars c ON c.id=h.calendar_id WHERE c.company_id=? ORDER BY h.holiday_date,h.name").bind(companyId).all<{
            id: string;
            calendar_id: string;
            holiday_date: string;
            name: string;
        }>(),
        engagement: (engagementId: string, companyId: string, value: string | null, _value2: string | null, value3: string | null, _value4: string | null) => readHrEngagement(write, companyId, engagementId, value, value3, lane8Actor),
        assignments: (companyId: string, engagementId: string) => write.prepare("SELECT a.id,a.policy_version_id,a.holiday_calendar_id,a.effective_from,a.effective_to,a.version,p.name policy,p.workweek_json,p.timezone,c.name calendar FROM leave_policy_assignments a JOIN leave_policy_versions p ON p.id=a.policy_version_id LEFT JOIN leave_holiday_calendars c ON c.id=a.holiday_calendar_id WHERE a.company_id=? AND a.engagement_id=? ORDER BY a.effective_from DESC").bind(companyId, engagementId).all<{
            id: string;
            policy_version_id: string;
            holiday_calendar_id: string | null;
            effective_from: string;
            effective_to: string | null;
            version: number;
            policy: string;
            workweek_json: number[] | string;
            timezone: string;
            calendar: string | null;
        }>(),
        policies: (companyId: string) => write.prepare(ppRd43ActiveLeavePolicyProjection('id,name,version,effective_from')).bind(companyId).all<{
            id: string;
            name: string;
            version: number;
            effective_from: string;
        }>(),
        calendars: (companyId: string) => write.prepare(ppRd61HolidayCalendarsByCompanySql(' AND active=true ORDER BY name')).bind(companyId).all<{
            id: string;
            name: string;
        }>(),
        eligibility: (engagementId: string, companyId: string) => readHrEligibility(write, companyId, engagementId, lane8Actor),
        affectedPeople: (companyId: string, encoded: string, value: string | null, _value2: string | null, value3: string | null, _value4: string | null) => readHrAffectedPeople(write, companyId, JSON.parse(encoded) as string[], value, value3, lane8Actor),
        futureShifts: (companyId: string, encoded: string, now: number) => readHrFutureShifts(write, companyId, JSON.parse(encoded) as string[], now, lane8Actor),
        futureLeaves: (companyId: string, today: string, companyId2: string, encoded: string) => write.prepare("SELECT coalesce(l.person_id,e.person_id) person_id,count(*)::int total FROM leave_requests l LEFT JOIN employees e ON e.id=l.employee_id AND e.company_id=l.company_id WHERE l.company_id=? AND l.state='approved' AND coalesce(l.end_date::date,l.start_date::date+(l.days::int-1))>=?::date AND coalesce(l.person_id,e.person_id) IN (SELECT person_id FROM hr_engagements WHERE company_id=? AND id IN (SELECT jsonb_array_elements_text(?::jsonb))) GROUP BY coalesce(l.person_id,e.person_id)").bind(companyId, today, companyId2, encoded).all<{
            person_id: string;
            total: number;
        }>(),
        currentPolicies: (companyId: string, encoded: string, today: string, _today2: string) => readHrCurrentPolicies(write, companyId, JSON.parse(encoded) as string[], today, lane8Actor),
        payGroups: (companyId: string, encoded: string) => readHrPayGroups(write, companyId, JSON.parse(encoded) as string[], lane8Actor) };
    return createPeopleHrApplication({ clock: { now: Date.now }, repository, prepare: async (actor) => { write = (await holdingResources(actor.holdingId)).write; lane8Actor = actor; }, companyAuthority: actor => requireCompanyAuthority(write, actor), staffSettings: actor => staffSettings(write, actor), defaults: actor => defaultCatalog(write, actor), leave: leaveAdministration, payrollOptions: nativePayGroups, organization: actor => organizationOverview(write, actor), templates: actor => peopleTemplatesData(write as PostgresDatabase, iamActorFromWork(actor)), lifecycle: actor => peopleLifecycleData(write, actor), iam: actor => iamOverview(write as PostgresDatabase, iamActorFromWork(actor)), directoryOptions });
}
async function invoke<Result>(run: () => Promise<Result>) { try {
    return await run();
}
catch (error) {
    if (error instanceof BusinessRuleError)
        throw new ApiError(error.status, error.code, error.message);
    throw error;
} }
export async function peopleHrSettingsData(actor: WorkContext, topic?: PeopleHrTopic) { return invoke(() => application().peopleHrSettingsData(actor, topic)); }
export async function peopleWorkingTimeData(actor: WorkContext, engagementId: string) { return invoke(() => application().peopleWorkingTimeData(actor, engagementId)); }
export async function peoplePositionWorkingTimeData(actor: WorkContext, positionId: string) { return invoke(() => application().peoplePositionWorkingTimeData(actor, positionId)); }
export async function peopleNewPersonData(actor: WorkContext) { const data=await invoke(() => application().peopleNewPersonData(actor));return {...data,purchases:await peoplePurchaseControls(actor)}; }
export async function peopleMoveWorkImpact(actor: WorkContext, engagementIds: string[]) { return invoke(() => application().peopleMoveWorkImpact(actor, engagementIds)); }
