import {BusinessRuleError,type Clock} from '@erpx/server-shared/index';
import type {WorkContext} from '@erpx/server-identity_access/public/authorization';
import type {GpoRuleRow} from '@erpx/server-identity_access/public/gpo-rules';
import type {OrganizationPerson,OrganizationPosition} from "@erpx/server-organization/public/rules/organization-domain";
import {positionMoveDate,type PositionMoveInput,type PositionMovePreview,type PositionMoveRecord,type PositionMovePolicyImpact} from "@erpx/feature-people_payroll/lib/position-moves";
import {decode,scoped,positionGpoMatches,positionMoveGraphIssues,positionMoveLeaveImpact,type Leave,type LeaveDefault} from '@erpx/server-organization/domain/position-moves';
const placement=(person:OrganizationPerson)=>({branchId:person.branch_id,departmentId:person.department_id,levelId:person.level_id,managerPersonId:person.manager_person_id});
const positionPlacement=(position:OrganizationPosition)=>({branchId:position.branch_id,departmentId:position.department_id,levelId:position.level_id,reportsToPositionId:position.reports_to_position_id});
export type MoveMember={id:string;person_id:string;version:number;status:string;start_date:string;end_date:string|null;currency:string};
export type PreparedPositionMove={preview:PositionMovePreview;snapshotHash:string;members:MoveMember[];position:OrganizationPosition;candidate:OrganizationPosition};
export type ClaimedPositionMove=PositionMoveRecord&{snapshot_hash:string;actor_json:WorkContext|string;authorizing_assignment_id:string};
export interface PositionMovePorts{clock:Clock;hash(value:unknown):string;authorize(actor:WorkContext,permission:'organization.manage'|'people.manage'):void;canReadPeople(actor:WorkContext):boolean;transaction<T>(run:(ports:PositionMovePorts)=>Promise<T>):Promise<T>;roster(companyId:string):Promise<{people:OrganizationPerson[];positions:OrganizationPosition[]}>;nodes(companyId:string):Promise<{id:string;kind:string;branch_id:string|null;status:string;hierarchy_revision:number}[]>;levels(companyId:string):Promise<{id:string;level_number:number;active:number;version:number}[]>;members(companyId:string,positionId:string):Promise<MoveMember[]>;openPlacements(companyId:string,positionId:string):Promise<{id:string}[]>;pending(companyId:string,moveId:string|null,positionId:string):Promise<boolean>;defaults(companyId:string):Promise<LeaveDefault[]>;leave(companyId:string,positionId:string,date:string):Promise<Leave[]>;rules(companyId:string):Promise<GpoRuleRow[]>;rows(actor:WorkContext,id?:string):Promise<PositionMoveRecord[]>;replay<Result>(actor:WorkContext,key:string,operation:string,id:string,payload:unknown):Promise<Result|null>;schedule(actor:WorkContext,input:PositionMoveInput,key:string,payload:{input:PositionMoveInput;previewToken:string;expiresAt:number},prepared:PreparedPositionMove,now:number):Promise<{id:string;version:number;status:'scheduled';effectiveAt:number}>;cancel(actor:WorkContext,id:string,expectedVersion:number,key:string,now:number):Promise<{id:string;version:number;status:string}>;claim(now:number):Promise<ClaimedPositionMove|null>;assignmentValid(row:ClaimedPositionMove,actor:WorkContext,affectedCount:number,now:number):Promise<boolean>;setPayrollDate(date:string):Promise<void>;applyPosition(companyId:string,candidate:OrganizationPosition,now:number):Promise<void>;applyHolder(row:ClaimedPositionMove,input:PositionMoveInput,holder:PositionMovePreview['people'][number],original:MoveMember,now:number):Promise<void>;recommendLeave(actor:WorkContext,row:ClaimedPositionMove,input:PositionMoveInput,candidate:OrganizationPosition,holder:PositionMovePreview['people'][number],original:MoveMember):Promise<void>;complete(row:ClaimedPositionMove,input:PositionMoveInput,affectedCount:number,now:number):Promise<void>;needsReview(id:string,message:string,now:number):Promise<void>;}
async function prepareMove(ports:PositionMovePorts, actor: WorkContext, input: PositionMoveInput, expiresAt: number, moveId: string | null = null) {
  ports.authorize(actor,"organization.manage");
  const { people, positions } = await ports.roster(actor.companyId);
  const position = positions.find(item => item.id === input.positionId && item.active === 1);
  if (!position || !scoped(actor, position)) throw new BusinessRuleError(404, "POSITION_NOT_FOUND", "Choose an active position in your authorized workspace.");
  const blockers: string[] = [];
  if (position.version !== input.expectedVersion) blockers.push("The position changed. Reload it and preview again.");
  const nodes=await ports.nodes(actor.companyId);
  const levels=await ports.levels(actor.companyId);
  const candidate = { ...position, reports_to_position_id: input.reportsToPositionId === undefined ? position.reports_to_position_id : input.reportsToPositionId };
  if (input.targetKind === "department") {
    const department = nodes.find(node => node.id === input.targetId && node.kind === "department" && node.status === "active");
    if (!department || !nodes.some(node => node.id === department.branch_id && node.kind === "branch" && node.status === "active"))
      throw new BusinessRuleError(422, "DEPARTMENT_INVALID", "Choose an active department in an active branch.");
    candidate.branch_id = department.branch_id; candidate.department_id = department.id;
  } else {
    const level = levels.find(level => level.id === input.targetId && level.active === 1 && level.level_number >= 1 && level.level_number <= 6);
    if (!level) throw new BusinessRuleError(422, "LEVEL_INVALID", "Choose one of the six active staff Levels.");
    candidate.level_id = level.id; candidate.level_number = level.level_number;
  }
  if (!scoped(actor, candidate)) throw new BusinessRuleError(403, "SCOPE_FORBIDDEN", "The destination is outside your authorized workspace.");
  const members=await ports.members(actor.companyId,position.id);
  const cohort = people.filter(person => person.position_id === position.id);
  if (members.length) ports.authorize(actor,"people.manage");
  if (members.length !== cohort.length) blockers.push("One or more non-ended holders are unavailable. Review their employee records before moving the position.");
  if (new Set(cohort.map(person => person.person_id)).size !== cohort.length || cohort.some(person => people.filter(other => other.person_id === person.person_id).length !== 1))
    blockers.push("Each employee must have exactly one non-ended engagement before a whole-position move.");
  if (cohort.some(person => !scoped(actor, person))) throw new BusinessRuleError(403, "SCOPE_FORBIDDEN", "Every position holder must be in your authorized workspace.");
  if (input.managerAssignments.some(item => !cohort.some(person => person.engagement_id === item.engagementId))) blockers.push("Manager changes must reference only the reviewed position holders.");
  const projected = cohort.map(person => ({ ...person,
    ...(input.targetKind === "department" ? { branch_id: candidate.branch_id, department_id: candidate.department_id } : {}),
    ...(input.targetKind === "level" ? { level_id: candidate.level_id, level_number: candidate.level_number } : {}),
    manager_person_id: input.managerAssignments.find(item => item.engagementId === person.engagement_id)?.managerPersonId ?? (input.managerAssignments.some(item => item.engagementId === person.engagement_id) ? null : person.manager_person_id),
  }));
  const date = positionMoveDate(input.effectiveAt, input.timezone);
  blockers.push(...positionMoveGraphIssues(people, positions, candidate, projected, date));
  const overlaps=await ports.openPlacements(actor.companyId,position.id);
  const pending=await ports.pending(actor.companyId,moveId,position.id);
  if (overlaps.length || pending) blockers.push("A holder or this position already has an open placement or position move. Cancel or finish it before previewing again.");
  const defaults=await ports.defaults(actor.companyId);
  const leave=await ports.leave(actor.companyId,position.id,date);
  const impacts = projected.map(person => {
    const before = cohort.find(item => item.engagement_id === person.engagement_id)!;
    const impact = positionMoveLeaveImpact(leave.find(item => item.engagement_id === person.engagement_id), defaults.find(item => item.level_id === person.level_id), before.level_id, person.level_id!, date);
    if (impact.blocker) blockers.push(`${person.legal_name}: ${impact.blocker}`);
    return { engagementId: person.engagement_id, personId: person.person_id, name: person.legal_name, before: placement(before), after: placement(person), leaveEffectiveFrom: impact.leaveEffectiveFrom, leaveNote: impact.leaveNote };
  });
  const rules = (await ports.rules(actor.companyId)).filter(rule => positionGpoMatches(rule, position) || positionGpoMatches(rule, candidate));
  const policies = new Map<string, PositionMovePolicyImpact>();
  for (const rule of rules) {
    const key = JSON.stringify([rule.connection_id, rule.gpo_id, rule.user_filter_group_id, rule.computer_target_ou_id]);
    const policy = policies.get(key) ?? { connectionId: rule.connection_id, gpoId: rule.gpo_id, userFilterGroupId: rule.user_filter_group_id, computerTargetOuId: rule.computer_target_ou_id, beforeSources: [], afterSources: [] };
    const source = { ruleId: rule.id, kind: rule.target_kind, id: rule.target_id };
    if (positionGpoMatches(rule, position)) policy.beforeSources.push(source);
    if (positionGpoMatches(rule, candidate)) policy.afterSources.push(source);
    policies.set(key, policy);
  }
  const policyChanges = { added: [...policies.values()].filter(policy => !policy.beforeSources.length), removed: [...policies.values()].filter(policy => !policy.afterSources.length), retained: [...policies.values()].filter(policy => policy.beforeSources.length && policy.afterSources.length) };
  // Reconciliation status can advance without invalidating review; rule semantics cannot.
  const policySnapshot = rules.map(rule => [rule.id, rule.target_kind, rule.target_id, rule.version, rule.revision, rule.connection_id, rule.gpo_id, rule.user_filter_group_id, rule.computer_target_ou_id, rule.active]);
  const snapshotHash = ports.hash({ people, positions, nodes, levels, members, policySnapshot, defaults: input.targetKind === "level" ? defaults : [], leave: input.targetKind === "level" ? leave : [] });
  const preview: PositionMovePreview = { previewToken: ports.hash({ input, snapshotHash, expiresAt }), expiresAt, ready: blockers.length === 0,
    blockers: [...new Set(blockers)], before: positionPlacement(position), after: positionPlacement(candidate), people: impacts, policyChanges, affectedCount: impacts.length, effectiveAt: input.effectiveAt, timezone: input.timezone };
  return { preview, snapshotHash, members, position, candidate };
}
export function createPositionMoveApplication(ports:PositionMovePorts){const visibleRows=async(p:PositionMovePorts,actor:WorkContext,id?:string)=>{p.authorize(actor,'organization.manage');const rows=(await p.rows(actor,id)).map(row=>({...row,definition:decode(row.definition),preview:decode(row.preview)})).filter(row=>!row.preview.affectedCount||p.canReadPeople(actor));if(id){if(!rows[0])throw new BusinessRuleError(404,'POSITION_MOVE_NOT_FOUND','Position move not found.');return rows[0];}return rows;};return {prepare:(actor:WorkContext,input:PositionMoveInput,expiresAt:number,moveId:string|null=null)=>prepareMove(ports,actor,input,expiresAt,moveId),preview:(actor:WorkContext,input:PositionMoveInput)=>ports.transaction(async p=>(await prepareMove(p,actor,input,p.clock.now()+15*60_000)).preview),async submit(actor:WorkContext,input:PositionMoveInput,previewToken:string,expiresAt:number,key:string){ports.authorize(actor,'organization.manage');const id=`position-move-${key}`,operation='organization.position-move.submit',payload={input,previewToken,expiresAt};return ports.transaction(async p=>{const prior=await p.replay<Awaited<ReturnType<PositionMovePorts['schedule']>>>(actor,key,operation,id,payload);if(prior)return prior;if(expiresAt<=p.clock.now()||expiresAt>p.clock.now()+15*60_000)throw new BusinessRuleError(409,'POSITION_MOVE_PREVIEW_EXPIRED','Preview the move again before confirming.');if(input.effectiveAt<p.clock.now()-60_000)throw new BusinessRuleError(422,'EFFECTIVE_DATE_PASSED','Choose an effective time now or in the future.');const prepared=await prepareMove(p,actor,input,expiresAt);if(prepared.preview.previewToken!==previewToken)throw new BusinessRuleError(409,'POSITION_MOVE_CHANGED','The reviewed organization changed. Preview the entire move again.');if(!prepared.preview.ready)throw new BusinessRuleError(422,'POSITION_MOVE_NOT_READY',prepared.preview.blockers.join(' '));return p.schedule(actor,input,key,payload,prepared,p.clock.now());});},rows:(actor:WorkContext,id?:string)=>visibleRows(ports,actor,id),async cancel(actor:WorkContext,id:string,expectedVersion:number,key:string){ports.authorize(actor,'organization.manage');return ports.transaction(async p=>{const current=await visibleRows(p,actor,id) as PositionMoveRecord,prior=await p.replay<Awaited<ReturnType<PositionMovePorts['cancel']>>>(actor,key,'organization.position-move.cancel',id,{expectedVersion});if(prior)return prior;if(current.status!=='scheduled'||current.effective_at<=p.clock.now())throw new BusinessRuleError(409,'POSITION_MOVE_LOCKED','Only a scheduled move before its effective time can be cancelled.');return p.cancel(actor,id,expectedVersion,key,p.clock.now());});},async drain(now=ports.clock.now()){let processed=0;for(let count=0;count<50;count++){let currentId:string|null=null;try{const applied=await ports.transaction(async p=>{const row=await p.claim(now);if(!row)return false;currentId=row.id;const actor=decode<WorkContext>(row.actor_json),input=decode<PositionMoveInput>(row.definition),saved=decode<PositionMovePreview>(row.preview);if(!await p.assignmentValid(row,actor,saved.affectedCount,now))throw new BusinessRuleError(409,'POSITION_MOVE_AUTHORITY_CHANGED','The authorizing HR role changed. Review the move again.');const prepared=await prepareMove(p,actor,input,saved.expiresAt,row.id);if(!prepared.preview.ready||prepared.snapshotHash!==row.snapshot_hash)throw new BusinessRuleError(409,'POSITION_MOVE_REVIEW_CHANGED',prepared.preview.blockers.join(' ')||'Organization, holders or leave defaults changed. Preview the entire move again.');await p.setPayrollDate(positionMoveDate(row.effective_at,input.timezone));await p.applyPosition(row.company_id,prepared.candidate,now);for(const holder of prepared.preview.people){const original=prepared.members.find(item=>item.id===holder.engagementId)!;await p.applyHolder(row,input,holder,original,now);if(holder.before.levelId!==holder.after.levelId)await p.recommendLeave(actor,row,input,prepared.candidate,holder,original);}await p.complete(row,input,saved.affectedCount,now);return true;});if(!applied)break;processed++;}catch(error){if(!currentId)throw error;const message=error instanceof BusinessRuleError?`${error.code}: ${error.message}`:'POSITION_MOVE_RETRY_REQUIRED: No changes applied. Review and submit the move again.';await ports.needsReview(currentId,message,now);}}return processed;}};}
