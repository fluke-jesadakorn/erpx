import { clearanceKinds, clearanceLabels, clearancePermissions, type ClearanceKind } from "@erpx/feature-people_payroll/lib/retirement";
import { hasPermission } from "@erpx/feature-identity_access/lib/permissions";
import {
  packageInput,
  type AccessPackage,
  type AccessPackageDefinition,
  type LifecycleCase,
  type LifecycleCaseStatus,
  type LifecycleInput,
  type LifecyclePreview,
  type PeopleLifecycleData,
  type PreparationTask,
} from "@erpx/feature-people_payroll/lib/people-lifecycle";
import type { WorkItem } from "@erpx/feature-work_projection/lib/work-item";import type {eligibleClearanceOwner} from "@erpx/server-people_payroll/public/retirement-clearance";import type {effectiveGpoRules} from "@erpx/server-identity_access/public/gpo-rules";import type {calendarCommand} from "@erpx/server-work_projection/public/calendar/commands";import type {WorkContext} from "@erpx/server-identity_access/public/authorization";import type {iamActorFromWork,IamActor} from "@erpx/server-identity_access/public/iam-context";import type {requireFreshMfa} from "@erpx/server-identity_access/public/iam-mfa";import type { grantInput, proposeIamReview } from "@erpx/server-identity_access/public/iam-service";
import type {validateIamGrant,insertIamGrant} from '@erpx/server-identity_access/public/iam-grants';import type {organizationGuard,validateStaffOrganization} from "@erpx/server-organization/public/organization";import {BusinessRuleError,type Clock} from '@erpx/server-shared/index';import type {RosterCommands} from '@erpx/server-people_payroll/application/people-roster';import {fail,parse,company,readable,visible,type LegacyCaseToSupersede,lifecycleSettingsInput,rehireInput,type LifecyclePositionFact,type LifecycleGrantFact,type LifecyclePackageRevisionFact,type LifecycleRoleFact,type LifecyclePersonFact} from '@erpx/server-people_payroll/domain/people-lifecycle';import {z} from 'zod';type Tail<Value extends unknown[]>=Value extends[unknown,...infer Rest]?Rest:never;type PortResult<Value,Effect>=Value extends Promise<infer Result>?Promise<PortResult<Result,Effect>>:Value extends {bind(...args:never[]):unknown;run():unknown}?Effect:Value extends (...args:never[])=>unknown?Value:Value extends readonly unknown[]?{[Index in keyof Value]:PortResult<Value[Index],Effect>}:Value extends object?{[Key in keyof Value]:PortResult<Value[Key],Effect>}:Value;export interface PeopleLifecycleRepository<Effect>{readLegacyCaseForPlacementRow(companyId:string,personId:string):Promise<{id:string;version:number;status:string;kind:LifecycleCase["kind"];effective_at:number;branch_id:string|null;department_id:string|null}|null>;
readPeopleLifecycleDataPositions(companyId:string):Promise<{results:(PeopleLifecycleData["positions"][number])[]}>;
readPeopleLifecycleDataAllPackages(companyId:string):Promise<{results:(AccessPackage)[]}>;
readPeopleLifecycleDataCases(companyId:string,branchId:string | null,branchId2:string | null,departmentId:string | null,departmentId2:string | null,hrView:boolean):Promise<{results:(LifecycleCase | LifecycleCaseStatus)[]}>;
readPeopleLifecycleDataTasks(companyId:string,hrView:boolean):Promise<{results:(PreparationTask)[]}>;
readPeopleLifecycleDataSettings(companyId:string):Promise<{
      version: number;
      definition: Omit<PeopleLifecycleData["settings"], "version">;
    }|null>;
readPeopleLifecycleDataRoster(companyId:string,branchId:string | null,branchId2:string | null,departmentId:string | null,departmentId2:string | null):Promise<{results:(PeopleLifecycleData["roster"][number])[]}>;
readPeopleLifecycleDataPreparation(now:number,companyId:string):Promise<{results:({engagement_id:string;position_id:string|null;package_id:string;package_revision:number;active:boolean;status:string|null;pending:boolean})[]}>;
readPeopleLifecycleDataGpoAccounts(companyId:string):Promise<{results:({person_id:string;connection_id:string;external_id:string})[]}>;
readPeopleLifecycleDataGpoJobs(companyId:string):Promise<{results:({position_id:string;person_id:string;engagement_id:string|null;account_id:string|null;status:string;connection_id:string;rule_id:string})[]}>;
readPeopleLifecycleDataManagers(companyId:string):Promise<{results:({ id: string; name: string })[]}>;
readPeopleLifecycleDataNodes(companyId:string):Promise<{results:({ id: string; name: string; kind: string })[]}>;
readPeopleLifecycleDataClearanceOwners(companyId:string,now:number):Promise<{results:(NonNullable<PeopleLifecycleData["clearanceOwners"]>[number])[]}>;
readPackageImpactPositions(companyId:string,id:string):Promise<{results:({ id: string; title: string; version: number })[]}>;
readPackageImpactPeople(companyId:string,id:string):Promise<{results:({ id: string; version: number; legal_name: string })[]}>;
readPackageImpactSnapshots(companyId:string,id:string):Promise<{results:({ definition: AccessPackageDefinition })[]}>;
readPreviewPackageCurrent(companyId:string,id:string):Promise<Record<string,unknown>|null>;
readEngagementRow(engagementId:string,personId:string,companyId:string):Promise<{
      id: string;
      person_id: string;
      version: number;
      branch_id: string | null;
      department_id: string | null;
      position_id: string | null;
      level_id: string | null;
      manager_person_id: string | null;
      job_title: string;
      status: string;
      legal_name: string;
      user_id: string | null;
      person_status: string;
      start_date: string;
      end_date: string | null;
    }|null>;
readLinkedAccountsEffect(companyId:string,personId:string,companyId2:string,personId2:string,companyId3:string,activeOnly:boolean,value:"local" | "univention" | "active_directory" | null,value2:"local" | "univention" | "active_directory" | null,value3:string | null):Promise<{results:({ user_id: string })[]}>;
readPreviewLifecyclePosition(companyId:string,positionId:string):Promise<{
          id: string;
          branch_id: string | null;
          department_id: string | null;
          level_id: string;
          package_id: string | null;
          revision: number | null;
        }|null>;
readPreviewLifecycleRecommendedPay(companyId:string,id:string,branchId:string | null,branchId2:string | null,departmentId:string | null,departmentId2:string | null):Promise<{currency:string;monthly_amount_minor:number}|null>;
readPreviewLifecycleRecommendedLeave(companyId:string,level_id:string):Promise<{level_name:string;annual_days:number;sick_days:number}|null>;
readPreviewLifecycleRevision(companyId:string,package_id:string,revision:number | null):Promise<{ definition: AccessPackageDefinition }|null>;
readPreviewLifecycleEffect(companyId:string,positionId:string | null):Promise<Record<string,unknown>|null>;
readPreviewLifecycleOld(companyId:string,companyId2:string,personId:string):Promise<{results:({ id: string; role: string; source: string; version: number })[]}>;
readSaveLifecycleCurrent(id:string,companyId:string):Promise<LifecycleCase|null>;
readSaveLifecycleEffect(companyId:string,engagementId:string,value:string):Promise<Record<string,unknown>|null>;
readSaveLifecycleEffect2(companyId:string,companyId2:string,personId:string):Promise<Record<string,unknown>|null>;
readSaveLifecycleBinding(companyId:string,positionId:string):Promise<{ package_id: string; revision: number }|null>;
readSaveLifecyclePosition(companyId:string,positionId:string):Promise<{
          level_id: string;
          branch_id: string | null;
          department_id: string | null;
        }|null>;
guard31(key:string,value:"legacy-case-version",id:string,companyId:string,personId:string,version:number):Effect;
stageSaveLifecycleEffect(now:number,id:string,companyId:string):Effect;
stageSaveLifecycleEffect2(now:number,id:string):Effect;
stageSaveLifecycleEffect3(companyId:string,id:string):Effect;
stageSaveLifecycleEffect4(now:number,companyId:string,id:string):Effect;
stageSaveLifecycleEffect5(value:string,key:string,userId:string,assignmentId:string,value2:"people.lifecycle.supersede",id:string,value3:number,value4:string,now:number,key2:string):Effect;
guard37(key:string,value:"case-version",value2:(string | number)[],current:boolean):Effect;
stageSaveLifecycleEffect6(id:string,companyId:string,personId:string,engagementId:string,kind:"onboarding" | "transfer" | "offboarding" | "suspend",state:"draft" | "scheduled" | "pending_review" | "waiting_it",version:number,effectiveAt:number,value:string | null,value2:number | null,value3:string,userId:string,now:number):Effect;
stageSaveLifecycleEffect7(value:string,assignmentId:string,value2:"legacy" | "placement",id:string):Effect;
guard40(key:string,value:"protected-person",companyId:string,personId:string,userId:string,companyId2:string,companyId3:string,personId2:string):Effect;
stageSaveLifecycleEffect8(companyId:string,personId:string,id:string,effectiveAt:number):Effect;
guard42(key:string,value:"prepared-package",companyId:string,value2:string,package_id:string,revision:number):Effect;
stageSaveLifecycleEffect9(value:"retirement" | "other",id:string,companyId:string):Effect;
readSaveLifecycleSettings(companyId:string):Promise<{ definition: PeopleLifecycleData["settings"] }|null>;
readSaveLifecycleManager(engagementId:string,companyId:string):Promise<{user_id:string|null}|null>;
guard46(key:string,value:string,companyId:string,owner:string,value2:string | null,value3:string | null,kind:"equipment" | "directory" | "handover" | "payroll" | "finance" | "hr"):Effect;
stageSaveLifecycleEffect10(taskId:string,companyId:string,id:string,value:string,owner:string | null,now:number,value2:boolean,lastWorkingDate:string | null,kind:"equipment" | "directory" | "handover" | "payroll" | "finance" | "hr",kind2:"equipment" | "directory" | "handover" | "payroll" | "finance" | "hr"):Effect;
stageSaveLifecycleEffect11(value:string,companyId:string,owner:string,taskId:string,value2:string,now:number):Effect;
stageSaveLifecycleEffect12(value:string,companyId:string,id:string,title:string,value2:string | null,now:number,privileged:boolean):Effect;
readCancelLifecycleRow(companyId:string,id:string):Promise<LifecycleCase|null>;
guard51(key:string,value:"case-version",id:string,companyId:string,expectedVersion:number):Effect;
stageCancelLifecycleEffect(now:number,id:string):Effect;
stageCancelLifecycleEffect2(now:number,id:string):Effect;
stageCancelLifecycleEffect3(now:number,id:string):Effect;
stageCancelLifecycleEffect4(id:string):Effect;
stageCancelLifecycleEffect5(id:string):Effect;
readPeopleLifecycleWorkReviews(companyId:string):Promise<{results:({ id: string; reason: string; created_at: number })[]}>;
readPeopleLifecycleWorkOperations(companyId:string):Promise<{results:({
          id: string;
          status: string;
          definition: { reason: string; action: { kind: string } };
          last_error: string | null;
          updated_at: number;
          created_by: string;
        })[]}>;
readCreateRehireEngagementOld(companyId:string,engagementId:string,personId:string):Promise<{
      version: number;
      status: string;
      person_status: string;
      branch_id: string | null;
      department_id: string | null;
      worker_type: string;
      employment_type: string;
      currency: string;
    }|null>;
readCreateRehireEngagementPosition(companyId:string,positionId:string):Promise<{
      branch_id: string | null;
      department_id: string | null;
      level_id: string;
      title: string;
    }|null>;
guard61(key:string,value:"new-engagement",companyId:string,personId:string):Effect;
stageCreateRehireEngagementEffect(id:string,personId:string,companyId:string,value:string | null,value2:string | null,positionId:string,level_id:string,worker_type:string,employment_type:string,startDate:string,title:string,currency:string,userId:string,now:number,now2:number):Effect;
stageCreateRehireEngagementEffect2(now:number,companyId:string,personId:string):Effect;
guard64(key:string,value:"settings-version",companyId:string,expectedVersion:number):Effect;
guard65(key:string,value:"it-assignee",itAssigneeId:string | null,companyId:string,itAssigneeId2:string | null):Effect;
stageSaveLifecycleSettingsEffect(companyId:string,value:number,value2:string):Effect;
stageSaveLifecycleSettingsEffect2(itAssigneeId:string | null,companyId:string):Effect;
applyValidatePackageEffect(companyId:unknown,id:unknown):Promise<{rows:(Record<string,unknown>)[];rowCount:number|null}>;
readValidatePackagerole(companyId:unknown,roleId:unknown):Promise<{rows:(LifecycleRoleFact)[];rowCount:number|null}>;
applyValidatePackageEffect2(companyId:unknown,scopeNodeId:unknown,scopeKind:unknown):Promise<{rows:(Record<string,unknown>)[];rowCount:number|null}>;
applyValidatePackageEffect3(companyId:unknown,connectionId:unknown,account:unknown):Promise<{rows:(Record<string,unknown>)[];rowCount:number|null}>;
applyValidatePackageEffect4(connectionId:unknown,id:unknown):Promise<{rows:(Record<string,unknown>)[];rowCount:number|null}>;
readSavePackagecurrent(companyId:unknown,id:unknown):Promise<{rows:(AccessPackage)[];rowCount:number|null}>;
applySavePackageEffect(companyId:unknown,id:unknown,version:unknown,revision:unknown,value:unknown):Promise<{rows:(Record<string,unknown>)[];rowCount:number|null}>;
applySavePackageEffect2(companyId:unknown,id:unknown,revision:unknown,value:unknown,userId:unknown,value2:unknown):Promise<{rows:(Record<string,unknown>)[];rowCount:number|null}>;
applySavePackageEffect3(companyId:unknown,id:unknown,revision:unknown):Promise<{rows:(Record<string,unknown>)[];rowCount:number|null}>;
applyLifecycleOutboxEffect(value:unknown,key:unknown,event:unknown,id:unknown,version:unknown,value2:unknown,value3:unknown):Promise<{rows:(Record<string,unknown>)[];rowCount:number|null}>;
readBindPositionPackageposition(companyId:unknown,positionId:unknown):Promise<{rows:(LifecyclePositionFact)[];rowCount:number|null}>;
readBindPositionPackagepkg(companyId:unknown,packageId:unknown,revision:unknown):Promise<{rows:(LifecyclePackageRevisionFact)[];rowCount:number|null}>;
readBindPositionPackagepeople(companyId:unknown,id:unknown):Promise<{rows:(LifecyclePersonFact)[];rowCount:number|null}>;
applyBindPositionPackageEffect(companyId:unknown,id:unknown,packageId:unknown,revision:unknown):Promise<{rows:(Record<string,unknown>)[];rowCount:number|null}>;
applyBindPositionPackageEffect2(companyId:unknown,id:unknown):Promise<{rows:(Record<string,unknown>)[];rowCount:number|null}>;
applyBindPositionPackageEffect3(companyId:unknown,id:unknown,value:unknown):Promise<{rows:(Record<string,unknown>)[];rowCount:number|null}>;
readRequestLifecycleAccessrow(companyId:unknown,id:unknown):Promise<{rows:(LifecycleCase)[];rowCount:number|null}>;
applyRequestLifecycleAccessEffect(companyId:unknown,id:unknown):Promise<{rows:(Record<string,unknown>)[];rowCount:number|null}>;
readRequestLifecycleAccessdefinition(companyId:unknown,package_id:unknown,package_revision:unknown):Promise<{rows:(LifecyclePackageRevisionFact)[];rowCount:number|null}>;
applyRequestLifecycleAccessEffect2(grantId:unknown,companyId:unknown,id:unknown,userId:unknown,index:unknown,value:unknown,id2:unknown):Promise<{rows:(Record<string,unknown>)[];rowCount:number|null}>;
applyRequestLifecycleAccessEffect3(id:unknown,value:unknown):Promise<{rows:(Record<string,unknown>)[];rowCount:number|null}>;
readApplyLifecycleReviewrow(companyId:unknown,caseId:unknown):Promise<{rows:(LifecycleCase)[];rowCount:number|null}>;
applyApplyLifecycleReviewEffect(id:unknown,value:unknown):Promise<{rows:(Record<string,unknown>)[];rowCount:number|null}>;
applyApplyLifecycleReviewEffect2(id:unknown,value:unknown):Promise<{rows:(Record<string,unknown>)[];rowCount:number|null}>;
applyApplyLifecycleReviewEffect3(companyId:unknown,id:unknown):Promise<{rows:(Record<string,unknown>)[];rowCount:number|null}>;
applyApplyLifecycleReviewEffect4(companyId:unknown,person_id:unknown,userId:unknown):Promise<{rows:({same:boolean;ok:boolean})[];rowCount:number|null}>;
applyApplyLifecycleReviewEffect5(userId:unknown,created_by:unknown,companyId:unknown):Promise<{rows:({same:boolean})[];rowCount:number|null}>;
readApplyLifecycleReviewg(grantId:unknown,companyId:unknown,id:unknown):Promise<{rows:(LifecycleGrantFact)[];rowCount:number|null}>;
applyApplyLifecycleReviewEffect6(companyId:unknown,person_id:unknown,userId:unknown):Promise<{rows:({same:boolean;ok:boolean})[];rowCount:number|null}>;
applyApplyLifecycleReviewEffect7(assignment:unknown,effective_at:unknown):Promise<{rows:(Record<string,unknown>)[];rowCount:number|null}>;
applyApplyLifecycleReviewEffect8(id:unknown,assignment:unknown):Promise<{rows:(Record<string,unknown>)[];rowCount:number|null}>;
applyApplyLifecycleReviewEffect9(id:unknown):Promise<{rows:(Record<string,unknown>)[];rowCount:number|null}>;
applyApplyLifecycleReviewEffect10(id:unknown,value:unknown):Promise<{rows:(Record<string,unknown>)[];rowCount:number|null}>;}export interface PeopleLifecyclePorts<Effect>{repository:PeopleLifecycleRepository<Effect>;clock:Clock;ids:{newId():string};authorize(actor:WorkContext,permission:string):WorkContext;commands:RosterCommands<Effect>;digest(value:unknown):string;grantInput:typeof grantInput;iamCommand<Result>(actor:IamActor,key:string,operation:string,input:unknown,permission:string,run:(ports:PeopleLifecyclePorts<Effect>)=>Promise<Result>):Promise<Result>;effectiveGpoRules(...args:Tail<Parameters<typeof effectiveGpoRules>>):PortResult<ReturnType<typeof effectiveGpoRules>,Effect>;eligibleClearanceOwner(...args:Tail<Parameters<typeof eligibleClearanceOwner>>):PortResult<ReturnType<typeof eligibleClearanceOwner>,Effect>;iamActorFromWork(...args:Parameters<typeof iamActorFromWork>):PortResult<ReturnType<typeof iamActorFromWork>,Effect>;requireFreshMfa(...args:Parameters<typeof requireFreshMfa>):PortResult<ReturnType<typeof requireFreshMfa>,Effect>;validateIamGrant(...args:Tail<Parameters<typeof validateIamGrant>>):PortResult<ReturnType<typeof validateIamGrant>,Effect>;insertIamGrant(...args:Tail<Parameters<typeof insertIamGrant>>):PortResult<ReturnType<typeof insertIamGrant>,Effect>;proposeIamReview(...args:Tail<Parameters<typeof proposeIamReview>>):PortResult<ReturnType<typeof proposeIamReview>,Effect>;organizationGuard(...args:Tail<Parameters<typeof organizationGuard>>):PortResult<ReturnType<typeof organizationGuard>,Effect>;validateStaffOrganization(...args:Tail<Parameters<typeof validateStaffOrganization>>):PortResult<ReturnType<typeof validateStaffOrganization>,Effect>;calendarCommand(...args:Tail<Parameters<typeof calendarCommand>>):PortResult<ReturnType<typeof calendarCommand>,Effect>;}export function createPeopleLifecycleApplication<Effect>(ports:PeopleLifecyclePorts<Effect>){const repository=ports.repository;async function legacyCaseForPlacement(
  actor: WorkContext,
  personId: string,
): Promise<LegacyCaseToSupersede | null> {
  const row = await repository.readLegacyCaseForPlacementRow(actor.companyId,personId);
  if (!row) return null;
  if (!visible(actor, row)) fail("This person's existing case is outside your workspace.", "LEGACY_CASE_SCOPE", 403);
  if (row.kind !== "onboarding" && row.kind !== "transfer")
    fail("Cancel the existing departure or suspension through its own case before placing this person.", "LEGACY_DEPARTURE_ACTIVE");
  if (row.status === "scheduled" && row.effective_at <= ports.clock.now())
    fail("This existing access case has reached its effective time. Refresh after it completes.", "LEGACY_CASE_EFFECTIVE");
  return {id:row.id,version:row.version,status:row.status,kind:row.kind,effectiveAt:row.effective_at};
}
async function peopleLifecycleData(
  actor: WorkContext,
): Promise<PeopleLifecycleData> {
  readable(actor);
  const hrView = hasPermission(actor, "people.manage");

  const positions = (
    await repository.readPeopleLifecycleDataPositions(actor.companyId)
  ).results.filter((p) => visible(actor, p));
  const gpoPolicies = await ports.effectiveGpoRules(actor.companyId);
  const gpoByPosition = new Map(positions.map(position => [position.id, gpoPolicies.filter(policy => policy.position_id === position.id)]));
  for (const position of positions) {
    const policies = gpoByPosition.get(position.id) ?? [];
    position.gpo = { mapped: policies.length > 0, directoryStatus: !policies.length ? null
      : policies.some(policy => policy.directory_config_status === "blocked") ? "blocked"
      : policies.every(policy => policy.directory_config_status === "directory_applied") ? "directory_applied" : "pending" };
  }
  const allPackages = (
    await repository.readPeopleLifecycleDataAllPackages(actor.companyId)
  ).results;
  const packages =
    actor.branchId || actor.departmentId
      ? allPackages.filter((p) => positions.some((x) => x.package_id === p.id))
      : allPackages;
  // IT needs case delivery state, but the HR case definition, review snapshot,
  // author and raw error can contain private employment or compensation data.
  
  const cases = (
    await repository.readPeopleLifecycleDataCases(actor.companyId,actor.branchId,actor.branchId,actor.departmentId,actor.departmentId,hrView)
  ).results;
  for (const c of cases)
    c.cutoffEffective =
      ["scheduled", "completed"].includes(c.status) &&
      c.effective_at <= ports.clock.now();
  const tasks = (
    await repository.readPeopleLifecycleDataTasks(actor.companyId,hrView)
  ).results.filter((t) =>
    t.position_id
      ? positions.some((p) => p.id === t.position_id)
      : t.application_id ? !actor.branchId && !actor.departmentId && hasPermission(actor,"iam.directory.manage") : cases.some((c) => c.id === t.case_id),
  );
  const settings = await repository.readPeopleLifecycleDataSettings(actor.companyId);
  const roster = (
    await repository.readPeopleLifecycleDataRoster(actor.companyId,actor.branchId,actor.branchId,actor.departmentId,actor.departmentId)
  ).results;
  const preparation = (await repository.readPeopleLifecycleDataPreparation(ports.clock.now(),actor.companyId)).results;
  for(const employee of roster) {
    const applied=preparation.filter(a=>a.engagement_id===employee.engagementId),position=positions.find(p=>p.id===employee.positionId);
    employee.itStatus=applied.some(a=>a.status==="blocked")?"problem":applied.some(a=>["queued","running","retry"].includes(a.status??"") || a.pending)?"working":applied.some(a=>a.active && a.position_id===employee.positionId && a.package_id===position?.package_id && a.package_revision===position?.package_revision)?"ready":"waiting";
  }
  const gpoAccounts = (await repository.readPeopleLifecycleDataGpoAccounts(actor.companyId)).results;
  const gpoJobs = (await repository.readPeopleLifecycleDataGpoJobs(actor.companyId)).results;
  for (const employee of roster) {
    const policies = employee.positionId ? gpoByPosition.get(employee.positionId) ?? [] : [];
    if (employee.status !== "active" || !policies.length) { employee.gpoDelivery = "none"; continue; }
    const deliveries = policies.map(policy => {
      if (policy.directory_config_status === "blocked") return "blocked";
      const accounts = gpoAccounts.filter(account => account.person_id === employee.id && account.connection_id === policy.connection_id);
      const jobs = gpoJobs.filter(job => job.rule_id === policy.id && job.connection_id === policy.connection_id && job.position_id === employee.positionId && job.person_id === employee.id && job.engagement_id === employee.engagementId);
      if (jobs.some(job => job.status === "blocked")) return "blocked";
      if (!accounts.length) return "awaiting_identity";
      return policy.directory_config_status === "directory_applied" && accounts.every(account => jobs.some(job => job.account_id === account.external_id && job.status === "directory_applied")) ? "directory_applied" : "queued";
    });
    employee.gpoDelivery = deliveries.includes("blocked") ? "blocked" : deliveries.includes("awaiting_identity") ? "awaiting_identity" : deliveries.includes("queued") ? "queued" : "directory_applied";
  }
  const managers = (
    await repository.readPeopleLifecycleDataManagers(actor.companyId)
  ).results;
  const nodes = (
    await repository.readPeopleLifecycleDataNodes(actor.companyId)
  ).results;
  const clearanceOwners = (await repository.readPeopleLifecycleDataClearanceOwners(actor.companyId,ports.clock.now())).results;
  return {
    clearanceOwners: hrView ? clearanceOwners : [],
    roster,
    managers: hrView ? managers : [],
    nodes,
    positions,
    packages,
    cases,
    tasks: tasks.map(task => ({...task,document_references:[],completion_reason:["finance","payroll"].includes(task.clearance_type ?? "") ? undefined : task.completion_reason})),
    settings: hrView
      ? {
          version: settings?.version ?? 0,
          itAssigneeId: null,
          onboardingTasks: [],
          offboardingTasks: [
            "Hand over work and pending approvals",
            "Return assigned equipment",
          ],
          ...settings?.definition,
        }
      : {
          version: settings?.version ?? 0,
          itAssigneeId: settings?.definition.itAssigneeId ?? null,
          onboardingTasks: [],
          offboardingTasks: [],
        },
    canPrepare:
      hasPermission(actor, "iam.directory.manage") &&
      !actor.branchId &&
      !actor.departmentId,
    canManage: hasPermission(actor, "people.manage"),
    canReview: hasPermission(actor, "iam.manage"),
  };
}
async function packageImpact(
  actor: WorkContext,
  id: string,
  definition: AccessPackageDefinition,
) {
  const positions = (
    await repository.readPackageImpactPositions(actor.companyId,id)
  ).results;
  const people = (
    await repository.readPackageImpactPeople(actor.companyId,id)
  ).results;
  const snapshots = (
    await repository.readPackageImpactSnapshots(actor.companyId,id)
  ).results;
  const computers = [
    ...new Set(
      [
        ...snapshots.flatMap((r) => r.definition.windowsPolicies),
        ...definition.windowsPolicies,
      ].flatMap((p) => (p.computerTarget ? [p.computerTarget] : [])),
    ),
  ];
  return { positions, people, computers };
}
async function previewPackage(
  actor: WorkContext,
  input: z.infer<typeof packageInput>,
) {
  ports.authorize(actor, "iam.directory.manage");
  company(actor);
  const id = input.id ?? "new",
    impact = await packageImpact(actor, id, input.definition);
  const current = input.id
    ? await repository.readPreviewPackageCurrent(actor.companyId,id)
    : null;
  return {
    token: ports.digest({ company: actor.companyId, input, current, impact }),
    affectedPositions: impact.positions.map((p) => p.title),
    affectedPeople: impact.people.map((p) => p.legal_name),
    affectedComputers: impact.computers,
  };
}
async function validatePackage(
  actor: IamActor,
  definition: AccessPackageDefinition,
) {
  for (const id of definition.groupIds) {
    if (
      !(
        await repository.applyValidatePackageEffect(actor.companyId,id)
      ).rowCount
    )
      fail("A selected group is unavailable.");
  }
  for (const grant of definition.grants) {
    const role = (
      await repository.readValidatePackagerole(actor.companyId,grant.roleId)
    ).rows[0];
    if (
      !role ||
      role.published_revision !== grant.expectedRoleRevision ||
      !role.scopes_json.includes(grant.scopeKind) ||
      grant.validUntil <= ports.clock.now()
    )
      fail("Refresh the role revision, scope, and expiry before publishing.");
    if (
      grant.scopeKind === "holding"
        ? grant.scopeNodeId !== null
        : !(
            await repository.applyValidatePackageEffect2(actor.companyId,grant.scopeNodeId,grant.scopeKind)
          ).rowCount
    )
      fail("Choose an access scope in this Company.");
  }
  if (definition.connectionId) {
    if (
      !(
        await repository.applyValidatePackageEffect3(actor.companyId,definition.connectionId,definition.account)
      ).rowCount
    )
      fail("Choose an active connection matching the account provider.");
    for (const id of definition.directoryGroupIds)
      if (
        !(
          await repository.applyValidatePackageEffect4(definition.connectionId,id)
        ).rowCount
      )
        fail("A directory group is missing from the latest snapshot.");
  }
}
async function savePackage(
  actor: WorkContext,
  input: z.infer<typeof packageInput>,
  key: string,
  publish: boolean,
  token?: string,
) {
  ports.authorize(actor, "iam.directory.manage");
  company(actor);
  const id = input.id ?? `package-${key}`,
    iam = ports.iamActorFromWork(actor);
  return ports.iamCommand<{id:string;version:number;revision:number}>(iam,
    key,
    publish ? "people.package.publish" : "people.package.draft",
    { input, publish, token },
    "iam.directory.manage",
    transaction=>createPeopleLifecycleApplication(transaction).savePackageMutation(actor,input,key,publish,token,id,iam),
  );
}
async function lifecycleOutbox(
  actor: WorkContext,
  key: string,
  event: string,
  id: string,
  version: number,
) {
  await repository.applyLifecycleOutboxEffect(ports.ids.newId(),key,event,id,version,JSON.stringify({ id, companyId: actor.companyId, version }),ports.clock.now());
}
async function bindPositionPackage(
  actor: WorkContext,
  input: {
    positionId: string;
    packageId: string;
    revision: number;
    expectedVersion: number;
    previewToken?: string;
  },
  key: string,
  previewOnly = false,
) {
  ports.authorize(actor, "iam.directory.manage");
  company(actor);
  return ports.iamCommand(ports.iamActorFromWork(actor),
    key,
    previewOnly ? "people.position.preview" : "people.position.bind",
    input,
    "iam.directory.manage",
    transaction=>createPeopleLifecycleApplication(transaction).bindPositionPackageMutation(actor,input,key,previewOnly),
  );
}
async function engagement(
  actor: WorkContext,
  input: LifecycleInput,
) {
  const row = await repository.readEngagementRow(input.engagementId,input.personId,actor.companyId);
  if (!row || !visible(actor, row))
    fail(
      "Choose an engagement in your workspace.",
      "ENGAGEMENT_NOT_FOUND",
      404,
    );
  return row;
}
async function linkedAccounts(
  actor: WorkContext,
  personId: string,
  activeOnly = false,
  definition?: AccessPackageDefinition,
) {
  return (
    await repository.readLinkedAccountsEffect(actor.companyId,personId,actor.companyId,personId,actor.companyId,activeOnly,definition?.account ?? null,definition?.account ?? null,definition?.connectionId ?? null)
  ).results.map((r) => r.user_id);
}
async function previewLifecycle(
  actor: WorkContext,
  input: LifecycleInput,
  placementOnly = false,
): Promise<LifecyclePreview> {
  readable(actor);
  if (input.departureReason === "retirement" && input.kind !== "offboarding") fail("Retirement uses offboarding.","RETIREMENT_KIND",422);
  const
    e = await engagement(actor, input),
    blockers: string[] = [];
  const position = input.positionId
    ? await repository.readPreviewLifecyclePosition(actor.companyId,input.positionId)
    : null;
  const incoming = ["onboarding", "transfer"].includes(input.kind);
  if(input.acceptRecommendedPay&&!hasPermission(actor,"people.compensation.read"))throw new BusinessRuleError(403,"PAY_ACCESS_REQUIRED","Compensation access is required.");
  const recommendedPay=incoming&&position&&hasPermission(actor,"people.compensation.read")?await repository.readPreviewLifecycleRecommendedPay(actor.companyId,position.id,actor.branchId,actor.branchId,actor.departmentId,actor.departmentId):null;
  const recommendedLeave=incoming&&position?await repository.readPreviewLifecycleRecommendedLeave(actor.companyId,position.level_id):null;
  if(input.acceptRecommendedPay&&!recommendedPay)blockers.push("This Position has no default pay. Review the position before submitting.");
  if(input.acceptRecommendedLeave&&!recommendedLeave)blockers.push("This Level has no default leave policy. Review People settings before submitting.");
  if (
    incoming &&
    (!position ||
      !visible(actor, {
        branch_id: position.branch_id ?? e.branch_id,
        department_id: position.department_id ?? e.department_id,
      }))
  )
    fail("Choose a position in your workspace.", "POSITION_REQUIRED", 422);
  const revision = position?.package_id
    ? await repository.readPreviewLifecycleRevision(actor.companyId,position.package_id,position.revision)
    : null;
  if (incoming && !placementOnly && !revision)
    blockers.push("IT has not prepared this position's access package.");
  if (
    incoming &&
    !placementOnly &&
    revision &&
    !(await repository.readPreviewLifecycleEffect(actor.companyId,input.positionId))
  )
    blockers.push("IT must review the position's updated work requirements.");
  if (incoming && e.status === "ended")
    blockers.push("Create a new engagement before rehiring this person.");
  if (incoming) {
    try {
      await ports.validateStaffOrganization(actor.companyId,
        {
          person_id: input.personId,
          branch_id: position!.branch_id ?? e.branch_id,
          department_id: position!.department_id ?? e.department_id,
          position_id: input.positionId,
          level_id: position!.level_id,
          manager_person_id: input.managerPersonId,
        },
        true,
      );
    } catch (error) {
      if (error instanceof BusinessRuleError) blockers.push(error.message);
      else throw error;
    }
  }
  const accounts = await linkedAccounts(actor,
    input.personId,
    incoming,
    incoming ? revision?.definition : undefined,
  );
  if (incoming && !placementOnly && !accounts.length)
    blockers.push(
      "Identity verification and account linking are required before requesting access.",
    );
  const old = (
    await repository.readPreviewLifecycleOld(actor.companyId,actor.companyId,input.personId)
  ).results;
  const removed = old
    .filter((g) => !incoming || (!placementOnly && g.source === "lifecycle"))
    .map((g) => g.role);
  const definition = incoming && !placementOnly ? (revision?.definition ?? null) : null;
  const legacyCaseToSupersede = placementOnly && incoming
    ? await legacyCaseForPlacement(actor, input.personId)
    : null;
  return {
    recommendation:incoming?{levelName:recommendedLeave?.level_name??null,currency:recommendedPay?.currency??null,monthlyAmountMinor:recommendedPay?.monthly_amount_minor??null,annualDays:recommendedLeave?.annual_days??null,sickDays:recommendedLeave?.sick_days??null}:undefined,
    token: ports.digest(placementOnly && incoming ? {input, e, position: position && {id:position.id,branch_id:position.branch_id,department_id:position.department_id,level_id:position.level_id}, placementOnly, legacyCaseToSupersede} : { input, e, position, revision, accounts, old }),
    ready: !blockers.length,
    blockers,
    legacyCaseToSupersede,
    definition,
    removed,
    added: definition?.grants.map((g) => g.roleId) ?? [],
    accounts,
    affectedPositions: [],
    affectedPeople: [e.legal_name],
    affectedComputers:
      definition?.windowsPolicies.flatMap((p) =>
        p.computerTarget ? [p.computerTarget] : [],
      ) ?? [],
  };
}
async function saveLifecycle(
  actor: WorkContext,
  input: LifecycleInput,
  key: string,
  submit: boolean,
  token?: string,
  placementOnly = false,
  supersedeLegacyCase?: {id:string;expectedVersion:number},
) {
  actor = ports.authorize(actor, "people.manage");
  const id = input.id ?? `lifecycle-${key}`,
    operation = submit ? "people.lifecycle.submit" : "people.lifecycle.draft",
    payload = { input, submit, token, placementOnly, supersedeLegacyCase };
  const prior = await ports.commands.replay<{id:string;version:number;status:string}>(actor, key, operation, id, payload);
  if (prior) return prior;
  const graphGuard = await ports.organizationGuard(actor.companyId, key);
  const e = await engagement(actor, input),
    preview = await previewLifecycle(actor, input, placementOnly);
  const current = input.id
    ? await repository.readSaveLifecycleCurrent(id,actor.companyId)
    : null;
  if (placementOnly && current?.workflow === "legacy")
    fail("Start a new HR placement and explicitly supersede the legacy access case.", "LEGACY_CASE_REQUIRES_NEW_PLACEMENT");
  if (supersedeLegacyCase && (!placementOnly || !submit || !["onboarding","transfer"].includes(input.kind)))
    fail("Legacy access cases can be superseded only when submitting an HR placement.", "LEGACY_SUPERSEDE_INVALID", 422);
  const legacy = placementOnly && submit && ["onboarding","transfer"].includes(input.kind)
    ? preview.legacyCaseToSupersede
    : null;
  if (legacy && !supersedeLegacyCase)
    fail("Confirm that the existing access case will be cancelled before submitting this HR placement.", "LEGACY_SUPERSEDE_REQUIRED", 422);
  if (supersedeLegacyCase && (!legacy || supersedeLegacyCase.id !== legacy.id || supersedeLegacyCase.expectedVersion !== legacy.version || id === legacy.id))
    fail("The existing access case changed. Preview the HR placement again.", "LEGACY_CASE_CHANGED");
  if (
    input.kind === "onboarding" &&
    (await repository.readSaveLifecycleEffect(actor.companyId,input.engagementId,legacy?.id ?? ""))
  )
    fail(
      "This engagement has already been onboarded. Use a transfer to review replacement access.",
      "TRANSFER_REQUIRED",
    );
  if (
    input.id &&
    (!current ||
      !["draft", "waiting_it"].includes(current.status) ||
      current.person_id !== input.personId ||
      current.kind !== input.kind ||
      current.engagement_id !== input.engagementId)
  )
    fail("Only this person's draft or waiting case can be edited.");
  if (submit && token !== preview.token)
    fail(
      "Review the current access and affected accounts before submitting.",
      "PREVIEW_CHANGED",
    );
  if (input.departureReason === "retirement" && input.kind !== "offboarding") fail("Retirement uses offboarding.","RETIREMENT_KIND",422);
  const outgoing = input.kind === "offboarding" || input.kind === "suspend";
  if (outgoing) {
    company(actor);
    if (e.status === "ended")
      fail(
        "This engagement has already ended. Choose the current engagement.",
        "ENGAGEMENT_ENDED",
      );
    if (input.lastWorkingDate && input.lastWorkingDate < e.start_date)
      fail(
        "The last working date cannot precede the engagement start date.",
        "INVALID_DATES",
        422,
      );
    if (preview.accounts.includes(actor.userId))
      fail(
        "Another HR administrator must end or suspend your access.",
        "SELF_ACCESS_CHANGE",
        403,
      );
    if (
      await repository.readSaveLifecycleEffect2(actor.companyId,actor.companyId,input.personId)
    )
      fail(
        "Use the protected administrator workflow before ending this person's access.",
        "PROTECTED_ADMINISTRATOR",
      );
    if (input.kind === "offboarding" && !input.lastWorkingDate)
      fail("Specify the last working date.", "LAST_WORKING_DATE_REQUIRED", 422);
  }
  if (submit && placementOnly && !preview.ready) fail(preview.blockers.join(" "), "PLACEMENT_NOT_READY", 422);
  const state = !submit
    ? "draft"
    : outgoing || placementOnly
      ? "scheduled"
      : preview.ready
        ? "pending_review"
        : "waiting_it";
  const binding = input.positionId && !placementOnly
    ? await repository.readSaveLifecycleBinding(actor.companyId,input.positionId)
    : null;
  const position = input.positionId
    ? await repository.readSaveLifecyclePosition(actor.companyId,input.positionId)
    : null;
  const snapshot = {
    engagementVersion: e.version,
    engagementPlacement: {
      position_id: e.position_id,
      branch_id: e.branch_id,
      department_id: e.department_id,
      level_id: e.level_id,
      manager_person_id: e.manager_person_id,
      job_title: e.job_title,
      status: e.status,
      start_date: e.start_date,
      end_date: e.end_date,
    },
    levelId: position?.level_id ?? null,
    positionBranch: position?.branch_id ?? null,
    positionDepartment: position?.department_id ?? null,
  };
  const version = input.expectedVersion + 1,
    now = ports.clock.now(),
    steps: Effect[] = [graphGuard];
  if (legacy) steps.push(
    repository.guard31(key,"legacy-case-version",legacy.id,actor.companyId,input.personId,legacy.version),
    repository.stageSaveLifecycleEffect(now,legacy.id,actor.companyId),
    repository.stageSaveLifecycleEffect2(now,legacy.id),
    repository.stageSaveLifecycleEffect3(actor.companyId,legacy.id),
    repository.stageSaveLifecycleEffect4(now,actor.companyId,legacy.id),
    repository.stageSaveLifecycleEffect5(ports.ids.newId(),key,actor.userId,actor.assignmentId,"people.lifecycle.supersede",legacy.id,legacy.version+1,`Superseded by HR placement ${id}`,now,key),
  );
  steps.push(
    repository.guard37(key,"case-version",current ? [id, input.expectedVersion] : [input.expectedVersion],Boolean(current)),
  );
  steps.push(
    repository.stageSaveLifecycleEffect6(id,actor.companyId,input.personId,input.engagementId,input.kind,state,version,input.effectiveAt,binding?.package_id ?? null,binding?.revision ?? null,JSON.stringify(input),actor.userId,now),
  );
  steps.push(
    repository.stageSaveLifecycleEffect7(JSON.stringify(snapshot),actor.assignmentId,placementOnly ? "placement" : "legacy",id),
  );
  if (submit && outgoing)
    steps.push(
      repository.guard40(key,"protected-person",actor.companyId,input.personId,actor.userId,actor.companyId,actor.companyId,input.personId),
      repository.stageSaveLifecycleEffect8(actor.companyId,input.personId,id,input.effectiveAt),
    );
  if (submit && binding)
    steps.push(
      repository.guard42(key,"prepared-package",actor.companyId,input.positionId!,binding.package_id,binding.revision),
    );
  steps.push(repository.stageSaveLifecycleEffect9(input.departureReason ?? "other",id,actor.companyId));
  if (submit) {
    const settings = await repository.readSaveLifecycleSettings(actor.companyId);
    if(input.departureReason === "retirement") {
      const manager = await repository.readSaveLifecycleManager(input.engagementId,actor.companyId);
      const owners: Record<ClearanceKind,string|null> = {hr:actor.userId,handover:manager?.user_id??null,equipment:input.clearanceOwners?.equipment??null,directory:settings?.definition.itAssigneeId??null,payroll:input.clearanceOwners?.payroll??null,finance:input.clearanceOwners?.finance??null};
      for(const kind of clearanceKinds) {
        let owner = owners[kind];
        if(owner && !await ports.eligibleClearanceOwner(actor.companyId,owner,kind)) {
          if(["equipment","payroll","finance"].includes(kind)) fail("Choose a currently eligible clearance owner.","CLEARANCE_ASSIGNEE",422);
          owner = null;
        }
        if(owner) steps.push(repository.guard46(key,`clearance-owner-${kind}`,actor.companyId,owner,clearancePermissions[kind],clearancePermissions[kind],kind));
        const taskId = `${id}-clearance-${kind}`;
        steps.push(repository.stageSaveLifecycleEffect10(taskId,actor.companyId,id,clearanceLabels[kind],owner,now,kind === "directory",input.lastWorkingDate,kind,kind));
        if(owner)steps.push(repository.stageSaveLifecycleEffect11(`${taskId}-notification`,actor.companyId,owner,taskId,`${clearanceLabels[kind]} · ${e.legal_name}`,now));
      }
    } else {
    const tasks: {title:string;privileged:boolean}[] = [
      ...(outgoing ? (settings?.definition.offboardingTasks ?? ["Hand over work and pending approvals", "Return assigned equipment"]) : (settings?.definition.onboardingTasks ?? [])).map(title => ({title, privileged:false})),
      ...(!outgoing && placementOnly ? [{title:"Prepare access for employee placement",privileged:true}] : []),
      ...(preview.definition?.tasks.map(t => ({title:t.label,privileged:!["equipment","handover"].includes(t.kind)})) ?? []),
      ...(preview.definition?.directoryGroupIds.map(id => ({title:`Prepare directory membership · ${id}`,privileged:true})) ?? []),
      ...(preview.definition?.windowsPolicies.map(p => ({title:`Apply and verify GPO · ${p.name} · ${p.targetOu}${p.computerTarget ? ` · Computers: ${p.computerTarget}` : ""}`,privileged:true})) ?? []),
      ...(outgoing ? [{title:"Review linked directory accounts; ERPX cutoff is independent of directory completion",privileged:true}] : []),
    ];
    tasks.forEach((task, i) =>
      steps.push(
        repository.stageSaveLifecycleEffect12(`${id}-task-${i}`,actor.companyId,id,task.title,settings?.definition.itAssigneeId ?? null,now,task.privileged),
      ),
    );
  }
  }
  return ports.commands.commit(actor,
    key,
    operation,
    payload,
    id,
    version,
    { id, version, status: state },
    steps,
    {
      companyId: actor.companyId,
      branchId: e.branch_id,
      departmentId: e.department_id,
    },
  );
}
async function cancelLifecycle(
  actor: WorkContext,
  id: string,
  expectedVersion: number,
  key: string,
) {
  actor = ports.authorize(actor, "people.manage");
  const replay = await ports.commands.replay(actor,
    key,
    "people.lifecycle.cancel",
    id,
    { id, expectedVersion },
  );
  if (replay) return replay;
  const row = await repository.readCancelLifecycleRow(actor.companyId,id);
  if (!row) fail("Lifecycle case not found.", "CASE_NOT_FOUND", 404);
  await engagement(actor, parse(row.definition));
  const now = ports.clock.now();
  if (
    row.status === "completed" ||
    row.status === "cancelled" ||
    (row.status === "scheduled" && row.effective_at <= now)
  )
    fail(
      "This change has taken effect. Request new access to restore it.",
      "ALREADY_EFFECTIVE",
    );
  return ports.commands.commit(actor,
    key,
    "people.lifecycle.cancel",
    { id, expectedVersion },
    id,
    expectedVersion + 1,
    { id, status: "cancelled" },
    [
      repository.guard51(key,"case-version",id,actor.companyId,expectedVersion),
      repository.stageCancelLifecycleEffect(now,id),
      repository.stageCancelLifecycleEffect2(now,id),
      repository.stageCancelLifecycleEffect3(now,id),
      repository.stageCancelLifecycleEffect4(id),
      repository.stageCancelLifecycleEffect5(id),
    ],
  );
}
async function requestLifecycleAccess(
  actor: WorkContext,
  id: string,
  version: number,
  key: string,
) {
  ports.authorize(actor, "iam.manage");
  const iam = ports.iamActorFromWork(actor);
  await ports.requireFreshMfa(iam);
  return ports.iamCommand(iam,
    key,
    "people.lifecycle.request-access",
    { id, version },
    "iam.manage",
    transaction=>createPeopleLifecycleApplication(transaction).requestLifecycleAccessMutation(actor,id,version,key,iam),
  );
}
async function applyLifecycleReview(
  actor: IamActor,
  kind: string,
  p: Record<string, unknown>,
  decision: "approve" | "reject" = "approve",
) {
  const row = (
    await repository.readApplyLifecycleReviewrow(actor.companyId,p.caseId)
  ).rows[0] as LifecycleCase | undefined;
  if (!row) fail("The lifecycle request was changed or cancelled.");
  if (decision === "reject") {
    await repository.applyApplyLifecycleReviewEffect(row.id,ports.clock.now());
    await repository.applyApplyLifecycleReviewEffect2(row.id,ports.clock.now());
    await repository.applyApplyLifecycleReviewEffect3(actor.companyId,row.id);
    return;
  }
  if (
    (
      await repository.applyApplyLifecycleReviewEffect4(actor.companyId,row.person_id,actor.userId)
    ).rows[0].same
  )
    fail(
      "You cannot approve your own employment access.",
      "SELF_ACCESS_CHANGE",
      403,
    );
  if (
    (
      await repository.applyApplyLifecycleReviewEffect5(actor.userId,row.created_by,actor.companyId)
    ).rows[0].same
  )
    fail(
      "An independent reviewer must approve this HR request.",
      "INDEPENDENT_REVIEW_REQUIRED",
      403,
    );
  if (kind === "people.lifecycle.grant") {
    const g = (
      await repository.readApplyLifecycleReviewg(p.grantId,actor.companyId,row.id)
    ).rows[0];
    if (!g || g.assignment_id)
      fail("This access request was already processed.");
    const grant = ports.grantInput.parse(g.definition);
    if (grant.validUntil <= Math.max(ports.clock.now(), row.effective_at))
      fail("Refresh the expired access request.");
    if (
      !(
        await repository.applyApplyLifecycleReviewEffect6(actor.companyId,row.person_id,grant.userId)
      ).rows[0].ok
    )
      fail("The verified account link changed.");
    await ports.validateIamGrant(actor, grant, String(p.parentId));
    const assignment = await ports.insertIamGrant(actor,
      grant,
      String(p.parentId),
      "lifecycle",
      g.id,
    );
    await repository.applyApplyLifecycleReviewEffect7(assignment,row.effective_at);
    await repository.applyApplyLifecycleReviewEffect8(g.id,assignment);
  }
  if (
    !(
      await repository.applyApplyLifecycleReviewEffect9(row.id)
    ).rowCount
  )
    await repository.applyApplyLifecycleReviewEffect10(row.id,ports.clock.now());
}
async function peopleLifecycleWork(
  actor: WorkContext,
): Promise<WorkItem[]> {
  if (
    !["people.manage", "iam.read", "organization.manage"].some((p) =>
      hasPermission(actor, p),
    )
  )
    return [];
  const data = await peopleLifecycleData(actor);
  const tasks: WorkItem[] = data.tasks
    .filter((t) => t.status === "pending" && !t.clearance_type)
    .map((t) => ({
      id: t.id,
      type: "people_lifecycle",
      title: t.title,
      detail: t.assignee_id ? "IT preparation" : "Unassigned IT queue",
      href: t.position_id
        ? `/people/organization?position=${encodeURIComponent(t.position_id)}`
        : `/people?case=${encodeURIComponent(t.case_id!)}`,
      status: t.status,
      updatedAt: t.updated_at,
      canAct: data.canPrepare && t.assignee_id === actor.userId,
      category: t.assignee_id ? "operational" : "setup",
      setupReason: t.assignee_id ? null : "Assign an IT owner",
      queue:
        data.canPrepare && (!t.assignee_id || t.assignee_id === actor.userId)
          ? "todo"
          : "waiting",
      responsibleQueue: t.assignee_id ?? "Unassigned IT",
      relationships: t.assignee_id === actor.userId ? ["assigned"] : ["team"],
    }));
  tasks.push(
    ...data.cases
      .filter((c) => !["completed", "cancelled"].includes(c.status))
      .map((c) => ({
        id: c.id,
        type: "people_lifecycle" as const,
        title: "definition" in c ? `${c.kind} · ${c.definition.reason}` : `${c.kind} · IT preparation`,
        blocker: c.last_error ?? null,
        detail:
          c.status === "waiting_it"
            ? "Waiting for position preparation or verified identity"
            : c.status.replaceAll("_", " "),
        href: `/people?case=${encodeURIComponent(c.id)}`,
        status: c.status,
        updatedAt: c.updated_at,
        dueAt: c.effective_at,
        queue:
          c.status === "pending_review" && data.canReview
            ? ("todo" as const)
            : ("waiting" as const),
        responsibleQueue:
          c.status === "pending_review" ? "Access reviewers" : "HR / IT",
        relationships:
          "created_by" in c && c.created_by === actor.userId
            ? ["requested" as const]
            : ["team" as const],
      })),
  );
  if (
    hasPermission(actor, "iam.review") &&
    !actor.branchId &&
    !actor.departmentId
  ) {
    const reviews = (
      await repository.readPeopleLifecycleWorkReviews(actor.companyId)
    ).results;
    tasks.push(
      ...reviews.map((r) => ({
        id: r.id,
        type: "people_lifecycle" as const,
        title: r.reason,
        detail: "Independent access review",
        href: "/people/settings?section=reviews",
        status: "pending_review",
        updatedAt: r.created_at,
        queue: "todo" as const,
        responsibleQueue: "Access reviewers",
        relationships: ["approval" as const],
      })),
    );
  }
  if (
    hasPermission(actor, "iam.read") &&
    !actor.branchId &&
    !actor.departmentId
  ) {
    const operations = (
      await repository.readPeopleLifecycleWorkOperations(actor.companyId)
    ).results;
    tasks.push(
      ...operations.map((op) => ({
        id: op.id,
        type: "people_lifecycle" as const,
        title: op.definition.reason,
        detail: "Directory · " + op.definition.action.kind + " · " + op.status,
        blocker: op.last_error,
        href: "/people/access?view=devices",
        status: op.status,
        updatedAt: op.updated_at,
        queue:
          op.status === "blocked" && data.canPrepare
            ? ("todo" as const)
            : ("waiting" as const),
        responsibleQueue:
          op.status === "pending_review" ? "Access reviewers" : "IT directory",
        relationships:
          op.created_by === actor.userId
            ? ["requested" as const]
            : ["team" as const],
      })),
    );
  }
  return tasks;
}
async function createRehireEngagement(
  actor: WorkContext,
  input: z.infer<typeof rehireInput>,
  key: string,
) {
  actor = ports.authorize(actor, "people.manage");
  const id = `eng-rehire-${key}`,
    operation = "people.lifecycle.rehire";
  const prior = await ports.commands.replay(actor, key, operation, id, input);
  if (prior) return prior;
  const guard = await ports.organizationGuard(actor.companyId, key);
  const old = await repository.readCreateRehireEngagementOld(actor.companyId,input.engagementId,input.personId);
  if (!old || !visible(actor, old))
    fail(
      "Choose the previous engagement in your workspace.",
      "ENGAGEMENT_NOT_FOUND",
      404,
    );
  if (old.status !== "ended" || old.person_status === "archived")
    fail(
      "End the previous engagement and restore an archived profile before rehiring.",
    );
  const position = await repository.readCreateRehireEngagementPosition(actor.companyId,input.positionId);
  if (
    !position ||
    !visible(actor, {
      branch_id: position.branch_id ?? old.branch_id,
      department_id: position.department_id ?? old.department_id,
    })
  )
    fail("Choose a position in your workspace.", "POSITION_REQUIRED", 422);
  const now = ports.clock.now();
  return ports.commands.commit(actor,
    key,
    operation,
    input,
    id,
    1,
    { id, personId: input.personId, status: "onboarding" },
    [
      guard,
      repository.guard61(key,"new-engagement",actor.companyId,input.personId),
      repository.stageCreateRehireEngagementEffect(id,input.personId,actor.companyId,position.branch_id ?? old.branch_id,position.department_id ?? old.department_id,input.positionId,position.level_id,old.worker_type,old.employment_type,input.startDate,position.title,old.currency,actor.userId,now,now),
      repository.stageCreateRehireEngagementEffect2(now,actor.companyId,input.personId),
    ],
  );
}
async function saveLifecycleSettings(
  actor: WorkContext,
  input: z.infer<typeof lifecycleSettingsInput>,
  key: string,
) {
  actor = ports.authorize(actor, "organization.manage");
  company(actor);
  return ports.commands.commit(actor,
    key,
    "people.lifecycle.settings",
    input,
    actor.companyId,
    input.expectedVersion + 1,
    { version: input.expectedVersion + 1 },
    [
      repository.guard64(key,"settings-version",actor.companyId,input.expectedVersion),
      repository.guard65(key,"it-assignee",input.itAssigneeId,actor.companyId,input.itAssigneeId),
      repository.stageSaveLifecycleSettingsEffect(actor.companyId,input.expectedVersion + 1,JSON.stringify(input)),
      repository.stageSaveLifecycleSettingsEffect2(input.itAssigneeId,actor.companyId),
    ],
  );
}
async function completePreparationTask(
  actor: WorkContext,
  id: string,
  version: number,
  key: string,
) {
  return ports.calendarCommand(actor, {action:"task.complete",id,expectedVersion:version},key);
}
async function savePackageMutation(actor:WorkContext,input:z.infer<typeof packageInput>,key:string,publish:boolean,token:string|undefined,id:string,iam:IamActor){
      const current = (
        await repository.readSavePackagecurrent(actor.companyId,id)
      ).rows[0];
      if (
        Number(current?.version ?? 0) !== input.expectedVersion ||
        (input.id && !current)
      )
        fail("The access package changed. Refresh before saving.");
      if (publish) {
        // Preview reads and row writes share this SERIALIZABLE transaction.
        
        if (
          !token ||
          token !== (await previewPackage(actor,input)).token
        )
          fail(
            "Review the current positions, people, and computer targets before publishing.",
            "PREVIEW_CHANGED",
          );
        await validatePackage(iam, input.definition);
      }
      const version = input.expectedVersion + 1,
        revision = publish
          ? Number(current?.published_revision ?? 0) + 1
          : Number(current?.published_revision ?? 0);
      await repository.applySavePackageEffect(actor.companyId,id,version,revision,JSON.stringify(input.definition));
      if (publish) {
        await repository.applySavePackageEffect2(actor.companyId,id,revision,JSON.stringify(input.definition),actor.userId,ports.clock.now());
        await repository.applySavePackageEffect3(actor.companyId,id,revision);
      }
      await lifecycleOutbox(actor,
        key,
        "people.package.changed",
        id,
        version,
      );
      return { id, version, revision };
    }
async function bindPositionPackageMutation(actor:WorkContext,input:Parameters<typeof bindPositionPackage>[1],key:string,previewOnly:boolean){
      const position = (
        await repository.readBindPositionPackageposition(actor.companyId,input.positionId)
      ).rows[0];
      const pkg = (
        await repository.readBindPositionPackagepkg(actor.companyId,input.packageId,input.revision)
      ).rows[0];
      if (!position || position.version !== input.expectedVersion || !pkg)
        fail("Refresh the position and choose a published access package.");
      const people = (
        await repository.readBindPositionPackagepeople(actor.companyId,position.id)
      ).rows;
      const token = ports.digest({ position, pkg, people });
      if (previewOnly)
        return {
          token,
          people: people.map((p) => p.legal_name),
          definition: pkg.definition,
        };
      if (input.previewToken !== token)
        fail(
          "Review who uses this position before applying the package.",
          "PREVIEW_CHANGED",
        );
      await repository.applyBindPositionPackageEffect(actor.companyId,position.id,input.packageId,input.revision);
      await repository.applyBindPositionPackageEffect2(actor.companyId,position.id);
      await repository.applyBindPositionPackageEffect3(actor.companyId,position.id,ports.clock.now());
      await lifecycleOutbox(actor,
        key,
        "people.position.prepared",
        position.id,
        position.version + 1,
      );
      return { id: position.id, version: position.version + 1 };
    }
async function requestLifecycleAccessMutation(actor:WorkContext,id:string,version:number,key:string,iam:IamActor){
      const row = (
        await repository.readRequestLifecycleAccessrow(actor.companyId,id)
      ).rows[0] as LifecycleCase | undefined;
      if (!row || row.version !== version || row.status !== "pending_review")
        fail("Refresh the lifecycle case before requesting personal access.");
      if (
        (
          await repository.applyRequestLifecycleAccessEffect(actor.companyId,id)
        ).rowCount
      )
        fail(
          "Individual reviews have already been requested. Follow them in Work.",
          "REVIEWS_ALREADY_REQUESTED",
        );
      const e = await engagement(actor, row.definition);
      const readiness = await previewLifecycle(actor,
        row.definition,
      );
      if (!readiness.ready)
        fail(readiness.blockers.join(" "), "LIFECYCLE_NOT_READY");
      const definition = (
        await repository.readRequestLifecycleAccessdefinition(actor.companyId,row.package_id,row.package_revision)
      ).rows[0]?.definition as AccessPackageDefinition | undefined;
      if (!definition) fail("IT must publish this position's package first.");
      const users = await linkedAccounts(actor,
        e.person_id,
        true,
        definition,
      );
      if (!users.length) fail("Verify and link an account first.");
      if (users.includes(actor.userId))
        fail(
          "Another administrator must request your access.",
          "SELF_ACCESS_CHANGE",
          403,
        );
      const reviews: string[] = [];
      for (const userId of users)
        for (const [index, g] of definition.grants.entries()) {
          const grant = ports.grantInput.parse({
            ...g,
            userId,
            reason: row.definition.reason,
          });
          if (grant.validUntil <= row.effective_at)
            fail("Access expires before the requested start date.");
          await ports.validateIamGrant(iam, grant);
          const grantId = `${id}-${userId}-${index}`;
          const review = await ports.proposeIamReview(iam,
            "people.lifecycle.grant",
            { ...grant, caseId: id, grantId, parentId: actor.assignmentId },
            `Review individual access · ${e.legal_name}`,
          );
          await repository.applyRequestLifecycleAccessEffect2(grantId,actor.companyId,id,userId,index,JSON.stringify(grant),review.id);
          reviews.push(review.id);
        }
      // Even an empty recipe needs a separate reviewer before activating the engagement.
      if (!reviews.length) {
        const review = await ports.proposeIamReview(iam,
          "people.lifecycle.activate",
          { caseId: id, parentId: actor.assignmentId },
          `Review onboarding · ${e.legal_name}`,
        );
        reviews.push(review.id);
      }
      await repository.applyRequestLifecycleAccessEffect3(id,ports.clock.now());
      await lifecycleOutbox(actor,
        key,
        "people.lifecycle.review-requested",
        id,
        version + 1,
      );
      return { id, reviews };
    }return {peopleLifecycleData,previewPackage,savePackage,bindPositionPackage,previewLifecycle,saveLifecycle,cancelLifecycle,requestLifecycleAccess,applyLifecycleReview,peopleLifecycleWork,createRehireEngagement,saveLifecycleSettings,completePreparationTask,savePackageMutation,bindPositionPackageMutation,requestLifecycleAccessMutation};}
