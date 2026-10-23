import {hasPermission} from "@erpx/feature-identity_access/lib/permissions";import type {WorkContext} from '@erpx/server-identity_access/public/authorization';
import type {iamOverview} from '@erpx/server-identity_access/public/iam-service-reads';import type {staffSettings} from '@erpx/server-operations/public/staff-settings';import type {defaultCatalog} from '@erpx/server-organization/public/staff-defaults';import type {organizationOverview} from '@erpx/server-organization/public/organization';import type {leaveAdministration} from '@erpx/server-people_payroll/public/leave-self-service';import type {directoryOptions} from '@erpx/server-people_payroll/public/people-directory';import type {payrollOptions} from '@erpx/server-people_payroll/public/people-payroll';import type {peopleLifecycleData} from '@erpx/server-people_payroll/public/people-lifecycle';import type {peopleTemplatesData} from '@erpx/server-people_payroll/public/people-templates';import {BusinessRuleError,type Clock} from '@erpx/server-shared/index';import {peopleHrTopics,type PeopleHrTopic} from '@erpx/server-people_payroll/domain/people-hr-settings';
export interface PeopleHrRepository{holidays(companyId:string):Promise<{results:{id:string;calendar_id:string;holiday_date:string;name:string}[]}>;
engagement(engagementId:string,companyId:string,value:string | null,value2:string | null,value3:string | null,value4:string | null):Promise<{id:string;person_id:string;branch_id:string|null;department_id:string|null}|null>;
assignments(companyId:string,engagementId:string):Promise<{results:{id:string;policy_version_id:string;holiday_calendar_id:string|null;effective_from:string;effective_to:string|null;version:number;policy:string;workweek_json:number[]|string;timezone:string;calendar:string|null}[]}>;
policies(companyId:string):Promise<{results:{id:string;name:string;version:number;effective_from:string}[]}>;
calendars(companyId:string):Promise<{results:{id:string;name:string}[]}>;
eligibility(engagementId:string,companyId:string):Promise<{eligible:boolean|null;version:number|null;top_level:boolean}|null>;
affectedPeople(companyId:string,encoded:string,value:string | null,value2:string | null,value3:string | null,value4:string | null):Promise<{results:{engagement_id:string;person_id:string;name:string}[]}>;
futureShifts(companyId:string,encoded:string,now:number):Promise<{results:{engagement_id:string;total:number}[]}>;
futureLeaves(companyId:string,today:string,companyId2:string,encoded:string):Promise<{results:{person_id:string;total:number}[]}>;
currentPolicies(companyId:string,encoded:string,today:string,today2:string):Promise<{results:{engagement_id:string;policy:string;calendar:string|null}[]}>;
payGroups(companyId:string,encoded:string):Promise<{results:{engagement_id:string;name:string|null}[]}>;}export interface PeopleHrPorts{clock:Clock;repository:PeopleHrRepository;prepare(actor:WorkContext):Promise<void>;companyAuthority(actor:WorkContext):Promise<unknown>;staffSettings(actor:WorkContext):ReturnType<typeof staffSettings>;defaults(actor:WorkContext):ReturnType<typeof defaultCatalog>;leave(actor:WorkContext):ReturnType<typeof leaveAdministration>;payrollOptions(actor:WorkContext):ReturnType<typeof payrollOptions>;organization(actor:WorkContext):ReturnType<typeof organizationOverview>;templates(actor:WorkContext):ReturnType<typeof peopleTemplatesData>;lifecycle(actor:WorkContext):ReturnType<typeof peopleLifecycleData>;iam(actor:WorkContext):ReturnType<typeof iamOverview>;directoryOptions(actor:WorkContext):ReturnType<typeof directoryOptions>;}
export function createPeopleHrApplication(ports:PeopleHrPorts){const repository=ports.repository;function fullCompany(actor: WorkContext) {
  if (actor.branchId || actor.departmentId) throw new BusinessRuleError(403, "COMPANY_SCOPE_REQUIRED", "Choose the Company workspace to edit HR settings.");
}
function need(actor: WorkContext, ...permissions: string[]) {
  if (!permissions.every(permission => hasPermission(actor, permission))) throw new BusinessRuleError(403, "HR_SETTINGS_FORBIDDEN", "Your role cannot access this HR setting.");
}
async function peopleHrSettingsData(actor: WorkContext, topic?: PeopleHrTopic) {
  fullCompany(actor);
  if (!topic) return {
    topics: peopleHrTopics.filter(item => item === "pay-groups" ? hasPermission(actor, "people.payroll.details") : item === "leave" || item === "calendars" ? hasPermission(actor, "leave.policy.manage") : hasPermission(actor, "people.manage") && hasPermission(actor, "organization.manage")),
  };
  await ports.prepare(actor);
  switch (topic) {
    case "levels": {
      need(actor, "people.manage", "organization.manage");
      await ports.companyAuthority(actor);
      const [settings, defaults] = await Promise.all([ports.staffSettings(actor), ports.defaults(actor)]);
      return { topic, levels: settings.levels, defaults: defaults.leave };
    }
    case "leave":
    case "calendars": {
      const data = await ports.leave(actor);
      const holidays = (await repository.holidays(actor.companyId)).results;
      return { topic, ...data, holidays };
    }
    case "pay-groups": {
      const groups = await ports.payrollOptions(actor);
      return { topic, groups, canManage: hasPermission(actor, "payroll.prepare") };
    }
    case "templates": {
      need(actor, "people.manage", "organization.manage");
      const [organization, holding] = await Promise.all([ports.organization(actor), ports.templates(actor)]);
      return { topic, templates: organization.templates, nodes: organization.nodes, holding };
    }
    case "lifecycle": {
      need(actor, "people.manage", "organization.manage");
      const lifecycle = await ports.lifecycle(actor);
      let owners: {id:string;name:string}[] = [];
      if (hasPermission(actor, "iam.read")) {
        const iam = await ports.iam(actor);
        owners = iam.users.filter(user => iam.assignments.some(assignment => assignment.user_id === user.id && assignment.active && assignment.scope_kind === "holding" && iam.roles.some(role => role.id === assignment.role && role.permissions.includes("iam.directory.manage")))).map(user => ({id:user.id,name:user.name}));
      }
      return { topic, settings: lifecycle.settings, owners };
    }
  }
}
async function peopleWorkingTimeData(actor: WorkContext, engagementId: string) {
  need(actor, "people.manage");
  await ports.prepare(actor);
  const engagement = await repository.engagement(engagementId,actor.companyId,actor.branchId ?? null,actor.branchId ?? null,actor.departmentId ?? null,actor.departmentId ?? null);
  if (!engagement) throw new BusinessRuleError(404, "ENGAGEMENT_NOT_FOUND", "This staff engagement is outside your workspace.");
  const [assignments, policies, calendars, eligibility] = await Promise.all([
    repository.assignments(actor.companyId,engagementId),
    repository.policies(actor.companyId),
    repository.calendars(actor.companyId),
    repository.eligibility(engagementId,actor.companyId),
  ]);
  return { assignments: assignments.results.map(assignment=>({...assignment,workweek_json:typeof assignment.workweek_json==="string"?JSON.parse(assignment.workweek_json) as number[]:assignment.workweek_json})), policies: policies.results, calendars: calendars.results, eligibility, canManage: !actor.branchId && !actor.departmentId && hasPermission(actor, "leave.policy.manage"), asOf: new Intl.DateTimeFormat("en-CA", {timeZone:"Asia/Bangkok",year:"numeric",month:"2-digit",day:"2-digit"}).format(new Date(ports.clock.now())) };
}
async function peoplePositionWorkingTimeData(actor: WorkContext, positionId: string) {
  need(actor, "people.manage", "organization.manage");
  await ports.prepare(actor);
  const organization = await ports.organization(actor);
  const position = organization.positions.find(item => item.id === positionId);
  if (!position) throw new BusinessRuleError(404, "POSITION_NOT_FOUND", "This position is outside your workspace.");
  return { position };
}
async function peopleNewPersonData(actor: WorkContext) {
  need(actor,"people.manage");
  await ports.prepare(actor);
  const [editorNodes,staffOptions,organization,lifecycle]=await Promise.all([ports.directoryOptions(actor),ports.staffSettings(actor),hasPermission(actor,"organization.manage")?ports.organization(actor):Promise.resolve(null),ports.lifecycle(actor)]);
  return {editorNodes,staffOptions,organization,lifecycle};
}
async function peopleMoveWorkImpact(actor: WorkContext, engagementIds: string[]) {
  need(actor, "people.manage");
  await ports.prepare(actor);
  const ids = [...new Set(engagementIds)], encoded = JSON.stringify(ids);
  const people = (await repository.affectedPeople(actor.companyId,encoded,actor.branchId??null,actor.branchId??null,actor.departmentId??null,actor.departmentId??null)).results;
  if (people.length!==ids.length) throw new BusinessRuleError(404,"ENGAGEMENT_NOT_FOUND","Affected staff are no longer available in this workspace. Refresh the move before confirming.");
  const today = new Intl.DateTimeFormat("en-CA",{timeZone:"Asia/Bangkok",year:"numeric",month:"2-digit",day:"2-digit"}).format(new Date(ports.clock.now()));
  const canReadRoster = ["roster.read","roster.manage","roster.approve"].some(permission=>hasPermission(actor,permission));
  const canReadPay = ["people.compensation.read","people.compensation.payroll"].some(permission=>hasPermission(actor,permission));
  const [shifts, leave, policies, groups] = await Promise.all([
    canReadRoster ? repository.futureShifts(actor.companyId,encoded,ports.clock.now()) : Promise.resolve({results:[] as {engagement_id:string;total:number}[]}),
    repository.futureLeaves(actor.companyId,today,actor.companyId,encoded),
    repository.currentPolicies(actor.companyId,encoded,today,today),
    canReadPay ? repository.payGroups(actor.companyId,encoded) : Promise.resolve({results:[] as {engagement_id:string;name:string|null}[]}),
  ]);
  return {people:people.map(person=>({engagementId:person.engagement_id,name:person.name,upcomingShifts:canReadRoster ? shifts.results.find(item=>item.engagement_id===person.engagement_id)?.total??0 : null,upcomingLeave:leave.results.find(item=>item.person_id===person.person_id)?.total??0,policy:policies.results.find(item=>item.engagement_id===person.engagement_id)?.policy??null,calendar:policies.results.find(item=>item.engagement_id===person.engagement_id)?.calendar??null,...(canReadPay?{payGroup:groups.results.find(item=>item.engagement_id===person.engagement_id)?.name??null}:{})}))};
}return {peopleHrSettingsData,peopleWorkingTimeData,peoplePositionWorkingTimeData,peopleNewPersonData,peopleMoveWorkImpact};}
