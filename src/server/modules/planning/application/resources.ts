import type {WorkContext} from '../../identity_access/public/authorization';import type {PlanningContext} from '../domain/capacity';import {hasModuleAccess} from '@/data/module-access';import {BusinessRuleError,type Clock} from '../../shared';import {resourceSchema,calendarSchema} from '../domain/resources';export interface PlanningResourceRepository<Effect>{guardBranch(key:string,value:"branch",branchId:string,companyId:string):Effect;
guardResourceVersion(key:string,value:"resource",id:string,companyId:string,branchId:string,expectedVersion:number):Effect;
guardNoActiveAllocation(key:string,value:"active-resource-allocation",id:string):Effect;
prepareResourceRevision(code:string,name:string,dailyMinutes:number,value:string,status:"active" | "archived",now:number,id:string):Effect;
prepareResource(resourceId:string,companyId:string,branchId:string,code:string,name:string,dailyMinutes:number,value:string,status:"active" | "archived",userId:string,now:number,now2:number):Effect;
lockResource(id:string):Effect;
guardActiveResourceVersion(key:string,value:"resource",id:string,companyId:string,resourceVersion:number,branchId:string | null,branchId2:string | null):Effect;
guardCalendarVersion(key:string,value:"calendar-version",id:string,workDate:string,expectedVersion:number):Effect;
guardReleasedCapacity(key:string,value:"released-capacity",id:string,workDate:string,availableMinutes:number):Effect;
prepareCalendar(id:string,workDate:string,availableMinutes:number,reason:string,userId:string,now:number):Effect;
lockSourceOrder(id:string):Effect;
guardSourceOrderVersion(key:string,value:string,id:string,companyId:string,version:number):Effect;
lockAllocatedResource(resourceId:string):Effect;
guardAllocatedResourceVersion(key:string,value:string,resourceId:string,companyId:string,resourceVersion:number):Effect;
guardAllocatedCalendarVersion(key:string,value:string,resourceId:string,workDate:string,calendarVersion:number):Effect;}export interface OwnerCommands<Effect>{replay<Result=Record<string,unknown>>(actor:WorkContext,key:string,operation:string,id:string,input:unknown):Promise<Result|null>;commit<Result>(actor:WorkContext,key:string,operation:string,input:unknown,id:string,version:number,result:Result,effects:Effect[],scope?:Pick<WorkContext,'companyId'|'branchId'|'departmentId'>):Promise<Result>;}export interface PlanningResourcePorts<Effect>{repository:PlanningResourceRepository<Effect>;commands:OwnerCommands<Effect>;clock:Clock;requirePermission(actor:WorkContext,permission:string):WorkContext;}export function createPlanningResourceApplication<Effect>(ports:PlanningResourcePorts<Effect>){const r=ports.repository;function access(a: WorkContext) {
  ports.requirePermission(a, "records.write");
  ports.requirePermission(a, "records.review");
  if (!hasModuleAccess(a.modules, "planning") || a.departmentId || a.beta)
    throw new BusinessRuleError(403, "PLANNING_RESOURCE_SCOPE", "Choose a Company or Branch planning reviewer to configure resources.");
}
async function saveProductionResource(a: WorkContext, id: string | null, raw: unknown, key: string) {
  access(a);
  const i = resourceSchema.parse(raw), resourceId = id ?? `production-resource-${key}`,
    op = id ? "planning.resource.update" : "planning.resource.create", prior = await ports.commands.replay( a, key, op, resourceId, i);
  if (prior) return prior;
  if (a.branchId && i.branchId !== a.branchId) throw new BusinessRuleError(403, "PLANNING_RESOURCE_SCOPE", "Configure resources only in the current branch.");
  const now = ports.clock.now(), weekdays = [...new Set(i.workWeekdays)].sort(), steps: Effect[] = [
    r.guardBranch(key,"branch",i.branchId,a.companyId),
  ];
  if (id) {
    if (!i.expectedVersion) throw new BusinessRuleError(422, "VERSION_REQUIRED", "Use the current resource version.");
    steps.push(
      r.guardResourceVersion(key,"resource",id,a.companyId,i.branchId,i.expectedVersion),
      r.guardNoActiveAllocation(key,"active-resource-allocation",id),
      r.prepareResourceRevision(i.code,i.name,i.dailyMinutes,JSON.stringify(weekdays),i.status,now,id),
    );
  } else steps.push(r.prepareResource(resourceId,a.companyId,i.branchId,i.code,i.name,i.dailyMinutes,JSON.stringify(weekdays),i.status,a.userId,now,now));
  return ports.commands.commit( a, key, op, i, resourceId, (i.expectedVersion ?? 0) + 1, { id: resourceId, version: (i.expectedVersion ?? 0) + 1, status: i.status }, steps);
}
async function saveProductionResourceCalendar(a: WorkContext, id: string, raw: unknown, key: string) {
  access(a);
  const i = calendarSchema.parse(raw), op = "planning.resource.calendar",
    prior = await ports.commands.replay( a, key, op, id, i);
  if (prior) return prior;
  return ports.commands.commit( a, key, op, i, id, i.expectedVersion + 1, { id, workDate: i.workDate, version: i.expectedVersion + 1 }, [
    r.lockResource(id),
    r.guardActiveResourceVersion(key,"resource",id,a.companyId,i.resourceVersion,a.branchId,a.branchId),
    r.guardCalendarVersion(key,"calendar-version",id,i.workDate,i.expectedVersion),
    r.guardReleasedCapacity(key,"released-capacity",id,i.workDate,i.availableMinutes),
    r.prepareCalendar(id,i.workDate,i.availableMinutes,i.reason,a.userId,ports.clock.now()),
  ]);
}
function planningContextGuards( a: WorkContext, key: string, context: PlanningContext): Effect[] {
  const guards: Effect[] = [];
  for (const [n, source] of [context.demandSource, ...(context.incomingSupply ?? [])].filter((x): x is NonNullable<typeof x> => Boolean(x)).entries()) {
    guards.push(r.lockSourceOrder(source.id),
      r.guardSourceOrderVersion(key,`planning-source-${n}`,source.id,a.companyId,source.version));
  }
  for (const [n, allocation] of (context.allocations ?? []).entries()) {
    guards.push(r.lockAllocatedResource(allocation.resourceId),
      r.guardAllocatedResourceVersion(key,`planning-resource-${n}`,allocation.resourceId,a.companyId,allocation.resourceVersion),
      r.guardAllocatedCalendarVersion(key,`planning-calendar-${n}`,allocation.resourceId,allocation.workDate,allocation.calendarVersion));
  }
  return guards;
}return {saveProductionResource,saveProductionResourceCalendar,planningContextGuards};}
