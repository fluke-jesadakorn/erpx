import "server-only";
import type { WorkContext } from "@/server/modules/identity_access/public/authorization";
import { ApiError } from "@/server/modules/runtime/public/http";
import { iamActorFromWork } from "@/server/modules/identity_access/public/iam-context";
import { iamOverview } from "@/server/modules/identity_access/public/iam-service";
import { leaveAdministration } from "@/server/modules/people_payroll/adapters/leave-self-service";
import { organizationOverview } from "@/server/modules/organization/public/organization";
import { peopleLifecycleData } from "@/server/modules/people_payroll/adapters/people-lifecycle";
import { directoryOptions } from "@/server/modules/people_payroll/adapters/people-directory";
import { payrollOptions } from "@/server/modules/people_payroll/adapters/people-payroll";
import { peopleTemplatesData } from "@/server/modules/people_payroll/adapters/people-templates";
import { holdingResources } from "@/server/modules/runtime/public/runtime";
import type { PostgresDatabase } from "@/server/modules/runtime/public/runtime/postgres";
import { defaultCatalog } from "@/server/modules/organization/public/staff-defaults";
import { requireCompanyAuthority } from "@/server/modules/operations/public/staff-policy";
import { staffSettings } from "@/server/modules/operations/public/staff-settings";
import type { SqlDatabase } from '../../runtime/public/runtime/types';
import { BusinessRuleError } from '../../shared';
import { createPeopleHrApplication, type PeopleHrRepository } from '../application/people-hr-settings';
import type { PeopleHrTopic } from '../domain/people-hr-settings';
export { peopleHrTopics } from '../domain/people-hr-settings';
export type { PeopleHrTopic } from '../domain/people-hr-settings';
function application() {
    let write: SqlDatabase;
    const repository: PeopleHrRepository = { holidays: (companyId: string) => write.prepare("SELECT h.id,h.calendar_id,h.holiday_date,h.name FROM leave_holidays h JOIN leave_holiday_calendars c ON c.id=h.calendar_id WHERE c.company_id=? ORDER BY h.holiday_date,h.name").bind(companyId).all<{
            id: string;
            calendar_id: string;
            holiday_date: string;
            name: string;
        }>(),
        engagement: (engagementId: string, companyId: string, value: string | null, value2: string | null, value3: string | null, value4: string | null) => write.prepare("SELECT id,person_id,branch_id,department_id FROM hr_engagements WHERE id=? AND company_id=? AND (?::text IS NULL OR branch_id=?) AND (?::text IS NULL OR department_id=?)").bind(engagementId, companyId, value, value2, value3, value4).first<{
            id: string;
            person_id: string;
            branch_id: string | null;
            department_id: string | null;
        }>(),
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
        policies: (companyId: string) => write.prepare("SELECT id,name,version,effective_from FROM leave_policy_versions WHERE company_id=? AND status='active' ORDER BY name,version DESC").bind(companyId).all<{
            id: string;
            name: string;
            version: number;
            effective_from: string;
        }>(),
        calendars: (companyId: string) => write.prepare("SELECT id,name FROM leave_holiday_calendars WHERE company_id=? AND active=true ORDER BY name").bind(companyId).all<{
            id: string;
            name: string;
        }>(),
        eligibility: (engagementId: string, companyId: string) => write.prepare("SELECT o.eligible,o.version,e.self_service_top_level top_level FROM hr_engagements e LEFT JOIN overtime_eligibility o ON o.engagement_id=e.id AND o.company_id=e.company_id WHERE e.id=? AND e.company_id=?").bind(engagementId, companyId).first<{
            eligible: boolean | null;
            version: number | null;
            top_level: boolean;
        }>(),
        affectedPeople: (companyId: string, encoded: string, value: string | null, value2: string | null, value3: string | null, value4: string | null) => write.prepare("SELECT e.id engagement_id,e.person_id,p.legal_name name FROM hr_engagements e JOIN hr_people p ON p.id=e.person_id AND p.company_id=e.company_id WHERE e.company_id=? AND e.id IN (SELECT jsonb_array_elements_text(?::jsonb)) AND (?::text IS NULL OR e.branch_id=?) AND (?::text IS NULL OR e.department_id=?) ORDER BY p.legal_name,e.id").bind(companyId, encoded, value, value2, value3, value4).all<{
            engagement_id: string;
            person_id: string;
            name: string;
        }>(),
        futureShifts: (companyId: string, encoded: string, now: number) => write.prepare("SELECT a.engagement_id,count(*)::int total FROM people_roster_assignments a JOIN people_roster_shifts s ON s.id=a.shift_id JOIN people_roster_weeks w ON w.id=s.week_id WHERE w.company_id=? AND a.engagement_id IN (SELECT jsonb_array_elements_text(?::jsonb)) AND a.state='assigned' AND s.state='active' AND s.ends_at>? GROUP BY a.engagement_id").bind(companyId, encoded, now).all<{
            engagement_id: string;
            total: number;
        }>(),
        futureLeaves: (companyId: string, today: string, companyId2: string, encoded: string) => write.prepare("SELECT coalesce(l.person_id,e.person_id) person_id,count(*)::int total FROM leave_requests l LEFT JOIN employees e ON e.id=l.employee_id AND e.company_id=l.company_id WHERE l.company_id=? AND l.state='approved' AND coalesce(l.end_date::date,l.start_date::date+(l.days::int-1))>=?::date AND coalesce(l.person_id,e.person_id) IN (SELECT person_id FROM hr_engagements WHERE company_id=? AND id IN (SELECT jsonb_array_elements_text(?::jsonb))) GROUP BY coalesce(l.person_id,e.person_id)").bind(companyId, today, companyId2, encoded).all<{
            person_id: string;
            total: number;
        }>(),
        currentPolicies: (companyId: string, encoded: string, today: string, today2: string) => write.prepare("SELECT DISTINCT ON (a.engagement_id) a.engagement_id,p.name policy,c.name calendar FROM leave_policy_assignments a JOIN leave_policy_versions p ON p.id=a.policy_version_id LEFT JOIN leave_holiday_calendars c ON c.id=a.holiday_calendar_id WHERE a.company_id=? AND a.engagement_id IN (SELECT jsonb_array_elements_text(?::jsonb)) AND a.effective_from<=? AND coalesce(a.effective_to,'9999-12-31')>=? ORDER BY a.engagement_id,a.effective_from DESC").bind(companyId, encoded, today, today2).all<{
            engagement_id: string;
            policy: string;
            calendar: string | null;
        }>(),
        payGroups: (companyId: string, encoded: string) => write.prepare("SELECT e.id engagement_id,g.name FROM hr_engagements e LEFT JOIN hr_pay_groups g ON g.id=e.pay_group_id AND g.company_id=e.company_id WHERE e.company_id=? AND e.id IN (SELECT jsonb_array_elements_text(?::jsonb))").bind(companyId, encoded).all<{
            engagement_id: string;
            name: string | null;
        }>() };
    return createPeopleHrApplication({ clock: { now: Date.now }, repository, prepare: async (actor) => { write = (await holdingResources(actor.holdingId)).write; }, companyAuthority: actor => requireCompanyAuthority(write, actor), staffSettings: actor => staffSettings(write, actor), defaults: actor => defaultCatalog(write, actor), leave: leaveAdministration, payrollOptions, organization: actor => organizationOverview(write, actor), templates: actor => peopleTemplatesData(write as PostgresDatabase, iamActorFromWork(actor)), lifecycle: actor => peopleLifecycleData(write, actor), iam: actor => iamOverview(write as PostgresDatabase, iamActorFromWork(actor)), directoryOptions });
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
export async function peopleNewPersonData(actor: WorkContext) { return invoke(() => application().peopleNewPersonData(actor)); }
export async function peopleMoveWorkImpact(actor: WorkContext, engagementIds: string[]) { return invoke(() => application().peopleMoveWorkImpact(actor, engagementIds)); }
