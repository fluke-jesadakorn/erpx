import { z } from "zod";import type {WorkContext} from "@erpx/server-identity_access/public/authorization";import type {StaffActor} from "@erpx/server-operations/public/staff-policy";import type {organizationGuard} from "@erpx/server-organization/public/organization";import type {validateStaffOrganization} from "@erpx/server-organization/public/organization";import type {recommendationStatements} from "@erpx/server-organization/public/staff-defaults";import type {activeStaff} from "@erpx/server-operations/public/staff-policy";import type {authorityGuard} from "@erpx/server-operations/public/staff-policy";import type {companyAuthority} from "@erpx/server-operations/public/staff-policy";import type {requireCompanyAuthority} from "@erpx/server-operations/public/staff-policy";import type {staffPeople} from "@erpx/server-operations/public/staff-policy";import type {staffGuard} from "@erpx/server-operations/public/staff-policy";import {levelSchema,levelAssignmentSchema,headSchema} from '@erpx/server-operations/domain/staff-settings';import {BusinessRuleError,type Clock,type IdentifierSource} from '@erpx/server-shared/index';import type {OperationsCommands} from '@erpx/server-operations/domain/commands';type Tail<Value extends unknown[]>=Value extends[unknown,...infer Rest]?Rest:never;type PortResult<Value,Effect>=Value extends Promise<infer Result>?Promise<PortResult<Result,Effect>>:Value extends {bind(...args:never[]):unknown;run():unknown}?Effect:Value extends (...args:never[])=>unknown?Value:Value extends readonly unknown[]?{[Index in keyof Value]:PortResult<Value[Index],Effect>}:Value extends object?{[Key in keyof Value]:PortResult<Value[Key],Effect>}:Value;export interface StaffSettingsRepository<Effect>{readListLevelsEffect(companyId:string):Promise<{results:({
        id: string;
        company_id: string;
        code: string;
        name: string;
        rank: number;
        level_number: number | null;
        active: number;
        version: number;
      })[]}>;
readStaffSettingsLevelspeopledepartmentscanManage(companyId:string):Promise<{results:({
        id: string;
        display_name: string;
        branch_id: string | null;
        person_id: string | null;
        version: number | null;
        head_name: string;
      })[]}>;
readSaveLevelPrevious(id:string,companyId:string):Promise<{level_number:number|null}|null>;
readSaveLevelEffect(id:string,id2:string):Promise<Record<string,unknown>|null>;
guard5(key:string,value:"level-version",id:string,companyId:string,expectedVersion:number):Effect;
stageSaveLevelEffect(code:string,name:string,rank:number,value:0 | 1,value2:number | null,now:number,id:string):Effect;
stageSaveLevelEffect2(recordId:string,companyId:string,code:string,name:string,rank:number,value:0 | 1,value2:number | null,now:number,now2:number):Effect;
guard8(key:string,value:"level-active",levelId:string,companyId:string):Effect;
stageLevelChangeStatementsEffect(value:string,engagementId:string,previous:string | null,levelId:string,userId:string,reason:string,key:string,now:number):Effect;
readAssignLevelEngagement(id:string,companyId:string):Promise<{
      person_id:string;position_id:string|null;manager_person_id:string|null;status:string;
      level_id: string | null;
      start_date: string;
      currency: string;
      branch_id: string | null;
      department_id: string | null;
    }|null>;
guard11(key:string,value:"engagement-version",id:string,companyId:string,expectedVersion:number):Effect;
stageAssignLevelEffect(levelId:string,now:number,id:string):Effect;
readSetDepartmentHeadDepartment(id:string,companyId:string):Promise<{ branch_id: string | null }|null>;
guard14(key:string,value:"department",id:string,companyId:string):Effect;
guard15(key:string,value:"head-version",expectedVersion:number,id:string,id2:string,expectedVersion2:number):Effect;
stageSetDepartmentHeadEffect(id:string,companyId:string,personId:string,value:number,userId:string,now:number):Effect;}export interface StaffSettingsPorts<Effect>{repository:StaffSettingsRepository<Effect>;clock:Clock;identifiers:IdentifierSource;commands:OperationsCommands<Effect>;prepare(holdingId:string):Promise<void>;production:boolean;batch(effects:Effect[]):Promise<unknown>;policyRead<Result>(key:string,read:()=>Promise<Result>):Promise<Result>;policyNow():number;organizationGuard(...args:Tail<Parameters<typeof organizationGuard>>):PortResult<ReturnType<typeof organizationGuard>,Effect>;validateStaffOrganization(...args:Tail<Parameters<typeof validateStaffOrganization>>) :PortResult<ReturnType<typeof validateStaffOrganization>,Effect>;recommendationStatements(...args:Tail<Parameters<typeof recommendationStatements>>) :PortResult<ReturnType<typeof recommendationStatements>,Effect>;activeStaff(...args:Tail<Parameters<typeof activeStaff>>):PortResult<ReturnType<typeof activeStaff>,Effect>;authorityGuard(...args:Tail<Parameters<typeof authorityGuard>>):PortResult<ReturnType<typeof authorityGuard>,Effect>;companyAuthority(...args:Tail<Parameters<typeof companyAuthority>>):PortResult<ReturnType<typeof companyAuthority>,Effect>;requireCompanyAuthority(...args:Tail<Parameters<typeof requireCompanyAuthority>>):PortResult<ReturnType<typeof requireCompanyAuthority>,Effect>;staffPeople(...args:Tail<Parameters<typeof staffPeople>>):PortResult<ReturnType<typeof staffPeople>,Effect>;staffGuard(...args:Tail<Parameters<typeof staffGuard>>):PortResult<ReturnType<typeof staffGuard>,Effect>;}export function createStaffSettingsApplication<Effect>(ports:StaffSettingsPorts<Effect>){const r=ports.repository;async function listLevels(companyId: string) {
  return (
    await r.readListLevelsEffect(companyId)
  ).results;
}
async function staffSettings(actor: StaffActor) {
  const [levels, people, departments, canManage] = await Promise.all([
    listLevels(actor.companyId),
    ports.staffPeople(actor.companyId),
    r.readStaffSettingsLevelspeopledepartmentscanManage(actor.companyId),
    ports.companyAuthority(actor),
  ]);
  return { levels, people, departments: departments.results, canManage };
}
async function saveLevel(
  actor: StaffActor,
  id: string | null,
  input: z.infer<typeof levelSchema>,
  key: string,
) {
  await ports.requireCompanyAuthority(actor);
  const recordId = id ?? `level-${key}`,
    operation = id ? "staff.level.update" : "staff.level.create";
  const prior = await ports.commands.replay(actor, key, operation, recordId, input);
  if (prior) return prior;
  const guard=await ports.organizationGuard(actor.companyId,key);
  const previous=id?await r.readSaveLevelPrevious(id,actor.companyId):null;
  if(previous?.level_number && input.levelNumber && previous.level_number!==input.levelNumber)throw new BusinessRuleError(422,"LEVEL_NUMBER_IMMUTABLE","Map staff explicitly to a different level; existing numbered levels keep their meaning.");
  if(id&&!input.active&&await r.readSaveLevelEffect(id,id))throw new BusinessRuleError(422,"LEVEL_IN_USE","Reassign active staff and positions before deactivating this level.");
  const now = ports.clock.now(),
    version = id ? input.expectedVersion + 1 : 1;
  return ports.commands.commit(actor,
    key,
    operation,
    input,
    recordId,
    version,
    { id: recordId, version },
    [
      guard,
      ports.authorityGuard(key, actor),
      ...(id
        ? [
            r.guard5(key,"level-version",id,actor.companyId,input.expectedVersion),
          ]
        : []),
      id
        ? r.stageSaveLevelEffect(input.code,input.name,input.rank,input.active ? 1 : 0,input.levelNumber ?? null,now,id)
        : r.stageSaveLevelEffect2(recordId,actor.companyId,input.code,input.name,input.rank,input.active ? 1 : 0,input.levelNumber ?? null,now,now),
    ],
    { companyId: actor.companyId, branchId: null, departmentId: null },
  );
}
function levelChangeStatements(
  key: string,
  actor: StaffActor,
  engagementId: string,
  levelId: string,
  previous: string | null,
  reason: string,
) {
  return [
    r.guard8(key,"level-active",levelId,actor.companyId),
    ...(levelId === previous
      ? []
      : [
          r.stageLevelChangeStatementsEffect(`level-history-${key}`,engagementId,previous,levelId,actor.userId,reason,key,ports.clock.now()),
        ]),
  ];
}
async function assignLevel(
  actor: StaffActor,
  id: string,
  input: z.infer<typeof levelAssignmentSchema>,
  key: string,
) {
  await ports.requireCompanyAuthority(actor);
  const prior = await ports.commands.replay(actor,
    key,
    "staff.level.assign",
    id,
    input,
  );
  if (prior) return prior;
  const guard=await ports.organizationGuard(actor.companyId,key);
  const engagement = await r.readAssignLevelEngagement(id,actor.companyId);
  if (!engagement)
    throw new BusinessRuleError(404, "ENGAGEMENT_NOT_FOUND", "Engagement not found.");
  if(engagement.position_id)await ports.validateStaffOrganization(actor.companyId,{...engagement,level_id:input.levelId},engagement.status==="active");
  return ports.commands.commit(actor,
    key,
    "staff.level.assign",
    input,
    id,
    input.expectedVersion + 1,
    { id, version: input.expectedVersion + 1 },
    [
      guard,
      ports.authorityGuard(key, actor),
      r.guard11(key,"engagement-version",id,actor.companyId,input.expectedVersion),
      ...levelChangeStatements(key,
        actor,
        id,
        input.levelId,
        engagement.level_id,
        input.reason,
      ),
      r.stageAssignLevelEffect(input.levelId,ports.clock.now(),id),
      ...await ports.recommendationStatements(actor as WorkContext,{
        engagementId:id,positionId:engagement.position_id,levelId:input.levelId,
        priorLevelId:engagement.level_id,startDate:new Date(ports.clock.now()).toISOString().slice(0,10),currency:engagement.currency,
        acceptPay:false,acceptLeave:true,commandId:key,
      }),
    ],
    {
      companyId: actor.companyId,
      branchId: engagement.branch_id,
      departmentId: engagement.department_id,
    },
  );
}
async function setDepartmentHead(
  actor: StaffActor,
  id: string,
  input: z.infer<typeof headSchema>,
  key: string,
) {
  await ports.requireCompanyAuthority(actor);
  const prior = await ports.commands.replay(actor,
    key,
    "staff.department.head",
    id,
    input,
  );
  if (prior) return prior;
  const people = await ports.staffPeople(actor.companyId),
    person = people.find((p) => p.person_id === input.personId);
  if (!person)
    throw new BusinessRuleError(
      422,
      "HEAD_UNAVAILABLE",
      "Choose active company staff with a linked account.",
    );
  await ports.activeStaff(actor.companyId, person.user_id);
  const department = await r.readSetDepartmentHeadDepartment(id,actor.companyId);
  return ports.commands.commit(actor,
    key,
    "staff.department.head",
    input,
    id,
    input.expectedVersion + 1,
    { id, version: input.expectedVersion + 1 },
    [
      ports.authorityGuard(key, actor),
      ports.staffGuard(key, actor.companyId, person.user_id),
      r.guard14(key,"department",id,actor.companyId),
      r.guard15(key,"head-version",input.expectedVersion,id,id,input.expectedVersion),
      r.stageSetDepartmentHeadEffect(id,actor.companyId,input.personId,input.expectedVersion + 1,actor.userId,ports.clock.now()),
    ],
    {
      companyId: actor.companyId,
      branchId: department?.branch_id ?? null,
      departmentId: id,
    },
  );
}return {listLevels,staffSettings,saveLevel,levelChangeStatements,assignLevel,setDepartmentHead};}
