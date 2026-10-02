import "server-only";
import { hasPermission } from "@/lib/permissions";
import type { WorkContext } from "./authorization";
import { ApiError } from "./http";
import { iamActorFromWork } from "./iam-context";
import { iamOverview } from "./iam-service";
import { leaveAdministration } from "./leave-self-service";
import { organizationOverview } from "./organization";
import { peopleLifecycleData } from "./people-lifecycle";
import { directoryOptions } from "./people-directory";
import { payrollOptions } from "./people-payroll";
import { peopleTemplatesData } from "./people-templates";
import { holdingResources } from "./runtime";
import type { PostgresDatabase } from "./runtime/postgres";
import { defaultCatalog } from "./staff-defaults";
import { requireCompanyAuthority } from "./staff-policy";
import { staffSettings } from "./staff-settings";

export const peopleHrTopics = ["levels", "leave", "calendars", "pay-groups", "templates", "lifecycle"] as const;
export type PeopleHrTopic = typeof peopleHrTopics[number];

function fullCompany(actor: WorkContext) {
  if (actor.branchId || actor.departmentId) throw new ApiError(403, "COMPANY_SCOPE_REQUIRED", "Choose the Company workspace to edit HR settings.");
}
function need(actor: WorkContext, ...permissions: string[]) {
  if (!permissions.every(permission => hasPermission(actor, permission))) throw new ApiError(403, "HR_SETTINGS_FORBIDDEN", "Your role cannot access this HR setting.");
}

export async function peopleHrSettingsData(actor: WorkContext, topic?: PeopleHrTopic) {
  fullCompany(actor);
  if (!topic) return {
    topics: peopleHrTopics.filter(item => item === "pay-groups" ? hasPermission(actor, "people.payroll.details") : item === "leave" || item === "calendars" ? hasPermission(actor, "leave.policy.manage") : hasPermission(actor, "people.manage") && hasPermission(actor, "organization.manage")),
  };
  const { write } = await holdingResources(actor.holdingId);
  switch (topic) {
    case "levels": {
      need(actor, "people.manage", "organization.manage");
      await requireCompanyAuthority(write, actor);
      const [settings, defaults] = await Promise.all([staffSettings(write, actor), defaultCatalog(write, actor)]);
      return { topic, levels: settings.levels, defaults: defaults.leave };
    }
    case "leave":
    case "calendars": {
      const data = await leaveAdministration(actor);
      const holidays = (await write.prepare("SELECT h.id,h.calendar_id,h.holiday_date,h.name FROM leave_holidays h JOIN leave_holiday_calendars c ON c.id=h.calendar_id WHERE c.company_id=? ORDER BY h.holiday_date,h.name").bind(actor.companyId).all<{id:string;calendar_id:string;holiday_date:string;name:string}>()).results;
      return { topic, ...data, holidays };
    }
    case "pay-groups": {
      const groups = await payrollOptions(actor);
      return { topic, groups, canManage: hasPermission(actor, "payroll.prepare") };
    }
    case "templates": {
      need(actor, "people.manage", "organization.manage");
      const [organization, holding] = await Promise.all([organizationOverview(write, actor), peopleTemplatesData(write as PostgresDatabase, iamActorFromWork(actor))]);
      return { topic, templates: organization.templates, nodes: organization.nodes, holding };
    }
    case "lifecycle": {
      need(actor, "people.manage", "organization.manage");
      const lifecycle = await peopleLifecycleData(write, actor);
      let owners: {id:string;name:string}[] = [];
      if (hasPermission(actor, "iam.read")) {
        const iam = await iamOverview(write as PostgresDatabase, iamActorFromWork(actor));
        owners = iam.users.filter(user => iam.assignments.some(assignment => assignment.user_id === user.id && assignment.active && assignment.scope_kind === "holding" && iam.roles.some(role => role.id === assignment.role && role.permissions.includes("iam.directory.manage")))).map(user => ({id:user.id,name:user.name}));
      }
      return { topic, settings: lifecycle.settings, owners };
    }
  }
}

export async function peopleWorkingTimeData(actor: WorkContext, engagementId: string) {
  need(actor, "people.manage");
  const { write } = await holdingResources(actor.holdingId);
  const engagement = await write.prepare("SELECT id,person_id,branch_id,department_id FROM hr_engagements WHERE id=? AND company_id=? AND (?::text IS NULL OR branch_id=?) AND (?::text IS NULL OR department_id=?)").bind(engagementId, actor.companyId, actor.branchId ?? null, actor.branchId ?? null, actor.departmentId ?? null, actor.departmentId ?? null).first<{id:string;person_id:string;branch_id:string|null;department_id:string|null}>();
  if (!engagement) throw new ApiError(404, "ENGAGEMENT_NOT_FOUND", "This staff engagement is outside your workspace.");
  const [assignments, policies, calendars, eligibility] = await Promise.all([
    write.prepare("SELECT a.id,a.policy_version_id,a.holiday_calendar_id,a.effective_from,a.effective_to,a.version,p.name policy,p.workweek_json,p.timezone,c.name calendar FROM leave_policy_assignments a JOIN leave_policy_versions p ON p.id=a.policy_version_id LEFT JOIN leave_holiday_calendars c ON c.id=a.holiday_calendar_id WHERE a.company_id=? AND a.engagement_id=? ORDER BY a.effective_from DESC").bind(actor.companyId, engagementId).all<{id:string;policy_version_id:string;holiday_calendar_id:string|null;effective_from:string;effective_to:string|null;version:number;policy:string;workweek_json:number[]|string;timezone:string;calendar:string|null}>(),
    write.prepare("SELECT id,name,version,effective_from FROM leave_policy_versions WHERE company_id=? AND status='active' ORDER BY name,version DESC").bind(actor.companyId).all<{id:string;name:string;version:number;effective_from:string}>(),
    write.prepare("SELECT id,name FROM leave_holiday_calendars WHERE company_id=? AND active=true ORDER BY name").bind(actor.companyId).all<{id:string;name:string}>(),
    write.prepare("SELECT o.eligible,o.version,e.self_service_top_level top_level FROM hr_engagements e LEFT JOIN overtime_eligibility o ON o.engagement_id=e.id AND o.company_id=e.company_id WHERE e.id=? AND e.company_id=?").bind(engagementId,actor.companyId).first<{eligible:boolean|null;version:number|null;top_level:boolean}>(),
  ]);
  return { assignments: assignments.results.map(assignment=>({...assignment,workweek_json:typeof assignment.workweek_json==="string"?JSON.parse(assignment.workweek_json) as number[]:assignment.workweek_json})), policies: policies.results, calendars: calendars.results, eligibility, canManage: !actor.branchId && !actor.departmentId && hasPermission(actor, "leave.policy.manage"), asOf: new Intl.DateTimeFormat("en-CA", {timeZone:"Asia/Bangkok",year:"numeric",month:"2-digit",day:"2-digit"}).format(new Date()) };
}

export async function peoplePositionWorkingTimeData(actor: WorkContext, positionId: string) {
  need(actor, "people.manage", "organization.manage");
  const { write } = await holdingResources(actor.holdingId);
  const organization = await organizationOverview(write, actor);
  const position = organization.positions.find(item => item.id === positionId);
  if (!position) throw new ApiError(404, "POSITION_NOT_FOUND", "This position is outside your workspace.");
  return { position };
}

export async function peopleNewPersonData(actor: WorkContext) {
  need(actor,"people.manage");
  const {write}=await holdingResources(actor.holdingId);
  const [editorNodes,staffOptions,organization,lifecycle]=await Promise.all([directoryOptions(actor),staffSettings(write,actor),hasPermission(actor,"organization.read")?organizationOverview(write,actor):Promise.resolve(null),peopleLifecycleData(write,actor)]);
  return {editorNodes,staffOptions,organization,lifecycle};
}

export async function peopleMoveWorkImpact(actor: WorkContext, engagementIds: string[]) {
  need(actor, "people.manage");
  const { write } = await holdingResources(actor.holdingId);
  const ids = [...new Set(engagementIds)], encoded = JSON.stringify(ids);
  const people = (await write.prepare("SELECT e.id engagement_id,e.person_id,p.legal_name name FROM hr_engagements e JOIN hr_people p ON p.id=e.person_id AND p.company_id=e.company_id WHERE e.company_id=? AND e.id IN (SELECT jsonb_array_elements_text(?::jsonb)) AND (?::text IS NULL OR e.branch_id=?) AND (?::text IS NULL OR e.department_id=?) ORDER BY p.legal_name,e.id").bind(actor.companyId,encoded,actor.branchId??null,actor.branchId??null,actor.departmentId??null,actor.departmentId??null).all<{engagement_id:string;person_id:string;name:string}>()).results;
  if (people.length!==ids.length) throw new ApiError(404,"ENGAGEMENT_NOT_FOUND","Affected staff are no longer available in this workspace. Refresh the move before confirming.");
  const today = new Intl.DateTimeFormat("en-CA",{timeZone:"Asia/Bangkok",year:"numeric",month:"2-digit",day:"2-digit"}).format(new Date());
  const canReadRoster = ["roster.read","roster.manage","roster.approve"].some(permission=>hasPermission(actor,permission));
  const canReadPay = ["people.compensation.read","people.compensation.payroll"].some(permission=>hasPermission(actor,permission));
  const [shifts, leave, policies, groups] = await Promise.all([
    canReadRoster ? write.prepare("SELECT a.engagement_id,count(*)::int total FROM people_roster_assignments a JOIN people_roster_shifts s ON s.id=a.shift_id JOIN people_roster_weeks w ON w.id=s.week_id WHERE w.company_id=? AND a.engagement_id IN (SELECT jsonb_array_elements_text(?::jsonb)) AND a.state='assigned' AND s.state='active' AND s.ends_at>? GROUP BY a.engagement_id").bind(actor.companyId,encoded,Date.now()).all<{engagement_id:string;total:number}>() : Promise.resolve({results:[] as {engagement_id:string;total:number}[]}),
    write.prepare("SELECT coalesce(l.person_id,e.person_id) person_id,count(*)::int total FROM leave_requests l LEFT JOIN employees e ON e.id=l.employee_id AND e.company_id=l.company_id WHERE l.company_id=? AND l.state='approved' AND coalesce(l.end_date::date,l.start_date::date+(l.days::int-1))>=?::date AND coalesce(l.person_id,e.person_id) IN (SELECT person_id FROM hr_engagements WHERE company_id=? AND id IN (SELECT jsonb_array_elements_text(?::jsonb))) GROUP BY coalesce(l.person_id,e.person_id)").bind(actor.companyId,today,actor.companyId,encoded).all<{person_id:string;total:number}>(),
    write.prepare("SELECT DISTINCT ON (a.engagement_id) a.engagement_id,p.name policy,c.name calendar FROM leave_policy_assignments a JOIN leave_policy_versions p ON p.id=a.policy_version_id LEFT JOIN leave_holiday_calendars c ON c.id=a.holiday_calendar_id WHERE a.company_id=? AND a.engagement_id IN (SELECT jsonb_array_elements_text(?::jsonb)) AND a.effective_from<=? AND coalesce(a.effective_to,'9999-12-31')>=? ORDER BY a.engagement_id,a.effective_from DESC").bind(actor.companyId,encoded,today,today).all<{engagement_id:string;policy:string;calendar:string|null}>(),
    canReadPay ? write.prepare("SELECT e.id engagement_id,g.name FROM hr_engagements e LEFT JOIN hr_pay_groups g ON g.id=e.pay_group_id AND g.company_id=e.company_id WHERE e.company_id=? AND e.id IN (SELECT jsonb_array_elements_text(?::jsonb))").bind(actor.companyId,encoded).all<{engagement_id:string;name:string|null}>() : Promise.resolve({results:[] as {engagement_id:string;name:string|null}[]}),
  ]);
  return {people:people.map(person=>({engagementId:person.engagement_id,name:person.name,upcomingShifts:canReadRoster ? shifts.results.find(item=>item.engagement_id===person.engagement_id)?.total??0 : null,upcomingLeave:leave.results.find(item=>item.person_id===person.person_id)?.total??0,policy:policies.results.find(item=>item.engagement_id===person.engagement_id)?.policy??null,calendar:policies.results.find(item=>item.engagement_id===person.engagement_id)?.calendar??null,...(canReadPay?{payGroup:groups.results.find(item=>item.engagement_id===person.engagement_id)?.name??null}:{})}))};
}
