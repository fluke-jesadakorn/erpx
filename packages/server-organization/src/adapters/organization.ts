
import { platformOwnerProjectionSources as commercePlatformSources } from "@erpx/server-commerce/public/platform-owner-projection-sources";
import { platformOwnerProjectionSources as identity_accessPlatformSources } from "@erpx/server-identity_access/public/platform-owner-projection-sources";
import { platformOwnerProjectionSources as people_payrollPlatformSources } from "@erpx/server-people_payroll/public/platform-owner-projection-sources";

import "server-only";
import {hasPermission} from "@erpx/feature-identity_access/lib/permissions";
import {samePerson} from "@erpx/feature-identity_access/lib/iam";
import type {WorkContext} from '@erpx/server-identity_access/public/authorization';
import {requirePermission} from '@erpx/server-identity_access/public/iam-permissions';
import {stageOrganizationDepartmentAssignment} from '@erpx/server-identity_access/public/organization-department-relocation';
import type {SqlDatabase,SqlStatement} from '@erpx/server-runtime/public/runtime/types';
import {ApiError} from '@erpx/server-runtime/public/http';
import {sha256} from '@erpx/server-runtime/public/crypto';
import {assertion} from "@erpx/server-workflow/public/command-assertions";
import {executeCommand,replayCommand} from '@erpx/server-workflow/public/commands';
import {spendingTemplate} from '@erpx/server-people_payroll/public/spending-template-read';
import {rosterPositionExitStatements} from '@erpx/server-people_payroll/public/roster-position-exit';
import {stageOrganizationDepartmentPeople} from '@erpx/server-people_payroll/public/organization-department-relocation';
import { organizationRosterExitEngagementIds, organizationMonthlyPayGroups } from '@erpx/server-people_payroll/public/organization-roster-facts';
import {stageOrganizationNodeChannels} from '@erpx/server-collaboration/public/organization-node-channels';
import {prepareAttachmentBinding} from '@erpx/server-files/public/attachments/service';
import {attachmentPurposeFacts} from '@erpx/server-files/public/attachment-purpose-facts';
import type { OrganizationPerson, OrganizationNode, OrganizationNodeChange } from "@erpx/feature-organization/lib/organization";
import {BusinessRuleError} from '@erpx/server-shared/index';
import {canReadOrganizationChanges,organizationChangeNotice} from '@erpx/server-organization/adapters/organization-work';
import {organizationMemberSql} from '@erpx/server-organization/adapters/organization-channels';
import {canSeeDefaultPay,defaultCatalog} from '@erpx/server-organization/adapters/staff-defaults';
import {createOrganizationApplication,type OrganizationPorts,type RelocationAuthority} from '@erpx/server-organization/application/organization';
import type {ChangeFilters,NodeChangeInput,NodeDecisionInput,PositionInput,TemplateInput,ApplyTemplateInput} from '@erpx/server-organization/domain/organization';
export {positionSchema,organizationTemplateSchema,applyOrganizationTemplateSchema,organizationNodeChangeSchema,organizationNodeDecisionSchema} from '@erpx/server-organization/domain/organization';
import {organizationGuard}from '@erpx/server-organization/public/organization-graph-guard';
import {organizationRoster}from '@erpx/server-organization/public/organization-roster-read';
const nodeImpactSql=`SELECT
 (SELECT count(*) FROM organization_nodes WHERE company_id=? AND parent_id=? AND status='active') child_nodes,
 (SELECT count(*) FROM (${people_payrollPlatformSources.hr_engagements}) hr_engagements WHERE company_id=? AND status='active' AND (branch_id=? OR department_id=?)) staff,
 (SELECT count(*) FROM organization_positions WHERE company_id=? AND active=1 AND (branch_id=? OR department_id=?)) positions,
 (SELECT count(*) FROM (${identity_accessPlatformSources.assignments}) assignments WHERE company_id=? AND revoked_at IS NULL AND scope_node_id=?) assignments,
 (SELECT count(*) FROM (${commercePlatformSources.commerce_warehouses}) commerce_warehouses WHERE company_id=? AND branch_id=?) warehouses`;
export async function organizationNodeImpact(db:SqlDatabase,companyId:string,nodeId:string){
  return (await db.prepare(nodeImpactSql).bind(companyId,nodeId,companyId,nodeId,nodeId,companyId,nodeId,nodeId,companyId,nodeId,companyId,nodeId).first<{child_nodes:number;staff:number;positions:number;assignments:number;warehouses:number}>())!;
}
async function relocationSnapshot(db:SqlDatabase,companyId:string,departmentId:string){
  const [engagements,positions,assignments,rules,head,legacyHead,flow]=await Promise.all([
    db.prepare(`SELECT id,version,status FROM (${people_payrollPlatformSources.hr_engagements}) hr_engagements WHERE company_id=? AND department_id=? AND status IN ('draft','onboarding','active','suspended') ORDER BY id`).bind(companyId,departmentId).all(),
    db.prepare("SELECT id,version FROM organization_positions WHERE company_id=? AND department_id=? AND active=1 ORDER BY id").bind(companyId,departmentId).all(),
    db.prepare(`SELECT id,version FROM (${identity_accessPlatformSources.assignments}) assignments WHERE company_id=? AND scope_kind='department' AND scope_node_id=? AND revoked_at IS NULL ORDER BY id`).bind(companyId,departmentId).all(),
    db.prepare(`SELECT id,version FROM (${identity_accessPlatformSources.organization_gpo_rules}) organization_gpo_rules WHERE company_id=? AND target_kind='department' AND target_id=? ORDER BY id`).bind(companyId,departmentId).all(),
    db.prepare("SELECT version,position_id,person_id,effective_at FROM authority_seats WHERE company_id=? AND kind='head' AND scope_id=?").bind(companyId,departmentId).first(),
    db.prepare("SELECT version,person_id FROM department_heads WHERE company_id=? AND department_id=?").bind(companyId,departmentId).first(),
    db.prepare("SELECT id,version,steps_json FROM department_flow_versions WHERE company_id=? AND department_id=? ORDER BY version DESC LIMIT 1").bind(companyId,departmentId).first(),
  ]);
  return sha256(JSON.stringify({engagements:engagements.results,positions:positions.results,assignments:assignments.results,rules:rules.results,head,legacyHead,flow}));
}
function changeSelection(companyId:string,filters:ChangeFilters){const state=filters.changeState==='open'?"state IN ('draft','submitted','rejected')":filters.changeState==='all'?'TRUE':'state=?',predicate=`company_id=? ${filters.changeId?'AND id=?':''} AND ${state} AND (display_name ILIKE ? OR reason ILIKE ?)`,search=`%${filters.changeQuery.replace(/[\\%_]/g,"\\$&")}%`;return {predicate,args:[companyId,...(filters.changeId?[filters.changeId]:[]),...(['open','all'].includes(filters.changeState)?[]:[filters.changeState]),search,search]};}
function portsFor(db:SqlDatabase):OrganizationPorts<SqlStatement>{return {
clock:{now:()=>Date.now()},authorize:actor=>requirePermission(actor,'organization.manage'),hasPermission,samePerson,canManageUnits:actor=>canReadOrganizationChanges(db,actor),canSeeDefaultPay,defaults:actor=>defaultCatalog(db,actor),spendingTemplate:companyId=>spendingTemplate(db,companyId),monthlyPayGroups:companyId=>organizationMonthlyPayGroups(db,companyId),roster:companyId=>organizationRoster(db,companyId),
 independentReviewer:async(actor,id)=>!(await db.prepare('SELECT iam_same_person(?,?,?) same_person').bind(actor.userId,id,actor.companyId).first<{same_person:boolean}>())?.same_person,
 nodes:async(actor)=>(await db.prepare("SELECT id,kind,slug,display_name,branch_id,status,hierarchy_revision FROM organization_nodes WHERE company_id=? AND kind IN ('company','branch','department') AND (?::text IS NULL OR id=? OR branch_id=?) AND (?::text IS NULL OR kind!='department' OR id=?) ORDER BY status,display_name").bind(actor.companyId,actor.branchId,actor.branchId,actor.branchId,actor.departmentId,actor.departmentId).all<OrganizationNode>()).results,
 levels:async(companyId)=>(await db.prepare('SELECT id,name,level_number FROM staff_levels WHERE company_id=? AND active=1 AND level_number IS NOT NULL ORDER BY level_number').bind(companyId).all<{id:string;name:string;level_number:number}>()).results,
 templates:async(companyId)=>(await db.prepare('SELECT id,name,version,positions_json FROM organization_templates WHERE company_id=? ORDER BY name').bind(companyId).all<{id:string;name:string;version:number;positions_json:string}>()).results,
 channelNodes:async(actor)=>(await db.prepare(`SELECT n.id FROM organization_nodes n WHERE n.company_id=? AND n.status='active' AND n.kind IN ('company','branch','department') AND ${organizationMemberSql('n','?')}`).bind(actor.companyId,actor.userId).all<{id:string}>()).results.map(n=>n.id),
 managerGrants:async(companyId,from,to)=>(await db.prepare(`SELECT user_id,scope_kind,scope_node_id FROM (${identity_accessPlatformSources.assignments}) assignments WHERE company_id=? AND iam_role_allows(role,company_id,'spending.approve') AND revoked_at IS NULL AND iam_assignment_active(id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) AND iam_principal_active(user_id,company_id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) AND valid_from<=? AND valid_until>?`).bind(companyId,from,to).all<{user_id:string;scope_kind:string;scope_node_id:string|null}>()).results,
 changeTotal:async(companyId,filters)=>{const {predicate,args}=changeSelection(companyId,filters);return Number((await db.prepare(`SELECT count(*) total FROM organization_node_changes WHERE ${predicate}`).bind(...args).first<{total:number}>())?.total??0);},
 changes:async(actor,filters,page,pageSize)=>{const {predicate,args}=changeSelection(actor.companyId,filters);return (await db.prepare(`SELECT *,NOT iam_same_person(requested_by,?,company_id) independent_reviewer FROM organization_node_changes WHERE ${predicate} ORDER BY updated_at DESC,id LIMIT ? OFFSET ?`).bind(actor.userId,...args,pageSize,(page-1)*pageSize).all<OrganizationNodeChange&{independent_reviewer:boolean}>()).results;},
 events:async(companyId,ids)=>(await db.prepare(`SELECT e.change_id,e.action,e.version,e.created_at,e.snapshot_json,COALESCE(p.legal_name,e.actor_id) actor_name FROM organization_node_change_events e LEFT JOIN (${people_payrollPlatformSources.hr_people}) p ON p.user_id=e.actor_id AND p.company_id=e.company_id AND p.merged_into_id IS NULL WHERE e.company_id=? AND e.change_id IN (${ids.map(()=>'?').join(',')}) ORDER BY e.version,e.created_at,e.id`).bind(companyId,...ids).all<{change_id:string;action:string;version:number;created_at:number;snapshot_json:string;actor_name:string}>()).results,
 graphGuard:(companyId,key)=>organizationGuard(db,companyId,key),replay:(actor,key,operation,id,input)=>replayCommand(db,actor,key,operation,id,input),
 branchActive:async(companyId,id)=>!!await db.prepare("SELECT 1 FROM organization_nodes WHERE id=? AND company_id=? AND kind='branch' AND status='active'").bind(id,companyId).first(),
 departmentActive:async(companyId,id,branchId)=>!!await db.prepare("SELECT 1 FROM organization_nodes WHERE company_id=? AND id=? AND kind='department' AND status='active' AND branch_id IS NOT DISTINCT FROM ?").bind(companyId,id,branchId).first(),
 node:(companyId,id)=>db.prepare("SELECT id,kind,slug,display_name,branch_id,status,hierarchy_revision FROM organization_nodes WHERE id=? AND company_id=? AND kind IN ('branch','department')").bind(id,companyId).first<OrganizationNode>(),
 reservedDepartment:async(companyId,id)=>!!await db.prepare(`SELECT 1 FROM organization_draft_reservations r WHERE r.company_id=? AND ((r.kind='position' AND EXISTS(SELECT 1 FROM organization_positions p WHERE p.id=r.entity_id AND p.company_id=r.company_id AND p.department_id=?)) OR (r.kind='person' AND EXISTS(SELECT 1 FROM (${people_payrollPlatformSources.hr_engagements}) e WHERE e.id=r.entity_id AND e.company_id=r.company_id AND e.department_id=?))) LIMIT 1`).bind(companyId,id,id).first(),
 hasChildren:async(companyId,id)=>!!await db.prepare("SELECT 1 FROM organization_nodes WHERE company_id=? AND parent_id=? AND status<>'archived' LIMIT 1").bind(companyId,id).first(),
 slugConflict:async(companyId,branchId,slug)=>!!await db.prepare('SELECT 1 FROM organization_nodes WHERE company_id=? AND parent_id=? AND slug=? LIMIT 1').bind(companyId,branchId,slug).first(),
 change:(companyId,id)=>db.prepare('SELECT * FROM organization_node_changes WHERE id=? AND company_id=?').bind(id,companyId).first<OrganizationNodeChange>(),
 target:(companyId,id)=>db.prepare('SELECT branch_id,slug FROM organization_nodes WHERE id=? AND company_id=?').bind(id,companyId).first<{branch_id:string|null;slug:string}>(),
 relocationSnapshot:(companyId,id)=>relocationSnapshot(db,companyId,id),impact:(companyId,id)=>organizationNodeImpact(db,companyId,id),submittedSnapshot:async(id)=>(await db.prepare("SELECT snapshot_json FROM organization_node_change_events WHERE change_id=? AND action='submitted' ORDER BY created_at DESC,id DESC LIMIT 1").bind(id).first<{snapshot_json:string}>())?.snapshot_json??null,
 attachmentSets:(companyId,ids)=>attachmentPurposeFacts(db,companyId,ids),prepareReviewBinding:(actor,id,attachmentSetIds,version,key,decision)=>prepareAttachmentBinding(db,actor,{attachmentSetIds,bindingContract:{targetType:'organization_change',bindingType:'organization_change_review',contexts:[{purposeKey:'org_change_proof',componentKind:'action',componentRef:'organization-change-review'}]},targetId:id,expectedTargetVersion:version,commandId:key,bindingId:`${id}:${decision}:${version+1}`}),
 reviewGuards:(actor,id,version,key,requesterId)=>[assertion(db,key,'change-version',"EXISTS(SELECT 1 FROM organization_node_changes WHERE id=? AND company_id=? AND version=? AND state='submitted')",[id,actor.companyId,version]),assertion(db,key,'independent-review','NOT iam_same_person(?,?,?)',[actor.userId,requesterId,actor.companyId])],
 stageCreateNode:(actor,change,nodeId)=>[db.prepare("INSERT INTO organization_nodes(id,kind,slug,display_name,parent_id,company_id,branch_id,status,hierarchy_revision) VALUES(?,?,?,?,?,?,?,'active',1)").bind(nodeId,change.node_kind,change.slug,change.display_name,change.node_kind==='branch'?actor.companyId:change.branch_id,actor.companyId,change.branch_id)],
 stageNodeVersion:(actor,change,key)=>[assertion(db,key,'node-version','EXISTS(SELECT 1 FROM organization_nodes WHERE id=? AND company_id=? AND hierarchy_revision=? AND kind=? AND branch_id IS NOT DISTINCT FROM ?)',[change.target_node_id,actor.companyId,change.expected_node_version!,change.node_kind,change.action==='relocate'?change.source_branch_id:change.branch_id])],
 relocationAuthority:async(companyId,id)=>{const [head,legacyHead,flow]=await Promise.all([db.prepare("SELECT version,position_id,person_id,effective_at FROM authority_seats WHERE company_id=? AND kind='head' AND scope_id=?").bind(companyId,id).first<RelocationAuthority['head']&{}>(),db.prepare('SELECT version,person_id FROM department_heads WHERE company_id=? AND department_id=?').bind(companyId,id).first<RelocationAuthority['legacyHead']&{}>(),db.prepare('SELECT id,version,steps_json FROM department_flow_versions WHERE company_id=? AND department_id=? ORDER BY version DESC LIMIT 1').bind(companyId,id).first<RelocationAuthority['flow']&{}>()]);return {head,legacyHead,flow};},
stageRelocate:(actor,change,id,appliedNodeId,key,{head,legacyHead,flow},now)=>{
        return [
          assertion(db,key,"relocation-destination-active","EXISTS(SELECT 1 FROM organization_nodes WHERE id=? AND company_id=? AND kind='branch' AND status='active')",[change.branch_id,actor.companyId]),
          assertion(db,key,"relocation-no-children","NOT EXISTS(SELECT 1 FROM organization_nodes WHERE company_id=? AND parent_id=? AND status<>'archived')",[actor.companyId,change.target_node_id]),
          assertion(db,key,"relocation-slug-available","NOT EXISTS(SELECT 1 FROM organization_nodes destination JOIN organization_nodes source ON source.id=? AND source.company_id=destination.company_id WHERE destination.company_id=? AND destination.parent_id=? AND destination.slug=source.slug)",[change.target_node_id,actor.companyId,change.branch_id]),
          assertion(db,key,"relocation-head-unchanged",head
            ? "EXISTS(SELECT 1 FROM authority_seats WHERE company_id=? AND kind='head' AND scope_id=? AND version=? AND position_id IS NOT DISTINCT FROM ? AND person_id IS NOT DISTINCT FROM ? AND effective_at IS NOT DISTINCT FROM ?)"
            : "NOT EXISTS(SELECT 1 FROM authority_seats WHERE company_id=? AND kind='head' AND scope_id=?)",
          head?[actor.companyId,change.target_node_id,head.version,head.position_id,head.person_id,head.effective_at]:[actor.companyId,change.target_node_id]),
          assertion(db,key,"relocation-legacy-head-unchanged",legacyHead
            ? "EXISTS(SELECT 1 FROM department_heads WHERE company_id=? AND department_id=? AND version=? AND person_id=?)"
            : "NOT EXISTS(SELECT 1 FROM department_heads WHERE company_id=? AND department_id=?)",
          legacyHead?[actor.companyId,change.target_node_id,legacyHead.version,legacyHead.person_id]:[actor.companyId,change.target_node_id]),
          assertion(db,key,"relocation-flow-unchanged",flow
            ? "(SELECT id FROM department_flow_versions WHERE company_id=? AND department_id=? ORDER BY version DESC LIMIT 1)=? AND EXISTS(SELECT 1 FROM department_flow_versions WHERE id=? AND company_id=? AND department_id=? AND version=? AND steps_json=?::jsonb)"
            : "NOT EXISTS(SELECT 1 FROM department_flow_versions WHERE company_id=? AND department_id=?)",
          flow?[actor.companyId,change.target_node_id,flow.id,flow.id,actor.companyId,change.target_node_id,flow.version,typeof flow.steps_json==="string"?flow.steps_json:JSON.stringify(flow.steps_json)]:[actor.companyId,change.target_node_id]),
          db.prepare("INSERT INTO organization_nodes(id,kind,slug,display_name,parent_id,company_id,branch_id,status,hierarchy_revision) SELECT ?,kind,slug,?,?,company_id,?,'active',1 FROM organization_nodes WHERE id=? AND company_id=?").bind(appliedNodeId,change.display_name,change.branch_id,change.branch_id,change.target_node_id,actor.companyId),
          db.prepare("UPDATE organization_nodes SET status='archived',hierarchy_revision=hierarchy_revision+1 WHERE id=? AND company_id=?").bind(change.target_node_id,actor.companyId),
...stageOrganizationDepartmentPeople(db,actor.companyId,change.target_node_id,change.branch_id,appliedNodeId,now),
          db.prepare("UPDATE organization_positions SET branch_id=?,department_id=?,updated_at=? WHERE company_id=? AND department_id=? AND active=1").bind(change.branch_id,appliedNodeId,now,actor.companyId,change.target_node_id),
stageOrganizationDepartmentAssignment(db,actor.companyId,change.target_node_id,change.branch_id,appliedNodeId),
          db.prepare("INSERT INTO authority_seats(company_id,kind,scope_id,version,position_id,person_id,effective_at) SELECT company_id,kind,?,1,position_id,person_id,effective_at FROM authority_seats WHERE company_id=? AND kind='head' AND scope_id=?").bind(appliedNodeId,actor.companyId,change.target_node_id),
          db.prepare("INSERT INTO authority_appointments(id,company_id,kind,scope_id,version,position_id,person_id,effective_at,appointed_by,reason,provenance,source_reference,command_id,created_at) SELECT ?,company_id,'head',?,1,position_id,person_id,COALESCE(effective_at,?::bigint),?,'Carried forward during department relocation','explicit',?,?,? FROM authority_seats WHERE company_id=? AND kind='head' AND scope_id=?").bind(`appointment-relocated-${id}`,appliedNodeId,now,actor.userId,change.target_node_id,`appointment-relocated-${id}`,now,actor.companyId,change.target_node_id),
          db.prepare("INSERT INTO department_heads(department_id,company_id,person_id,version,updated_by,updated_at) SELECT ?,company_id,person_id,1,?,?::bigint FROM authority_seats WHERE company_id=? AND kind='head' AND scope_id=? AND person_id IS NOT NULL UNION ALL SELECT ?,legacy.company_id,legacy.person_id,1,?,?::bigint FROM department_heads legacy WHERE legacy.company_id=? AND legacy.department_id=? AND NOT EXISTS(SELECT 1 FROM authority_seats s WHERE s.company_id=legacy.company_id AND s.kind='head' AND s.scope_id=legacy.department_id)").bind(appliedNodeId,actor.userId,now,actor.companyId,change.target_node_id,appliedNodeId,actor.userId,now,actor.companyId,change.target_node_id),
          db.prepare("INSERT INTO department_flow_versions(id,company_id,department_id,version,steps_json,published_by,reason,command_id,created_at) SELECT ?,company_id,?,1,steps_json,?,'Carried forward during department relocation',?,? FROM department_flow_versions WHERE company_id=? AND department_id=? ORDER BY version DESC LIMIT 1").bind(`department-flow-relocated-${id}`,appliedNodeId,actor.userId,`department-flow-relocated-${id}`,now,actor.companyId,change.target_node_id),
        ];
},stageUpdateNode:(actor,change)=>{const status=change.action==='suspend'?'suspended':change.action==='archive'?'archived':change.action==='reactivate'?'active':null;return [db.prepare('UPDATE organization_nodes SET display_name=?,status=COALESCE(?,status),hierarchy_revision=hierarchy_revision+1 WHERE id=? AND company_id=?').bind(change.display_name,status,change.target_node_id,actor.companyId)];},stageChannels:(actor,change,nodeId,now)=>stageOrganizationNodeChannels(db,actor.companyId,change.display_name,nodeId,now),
commitReview:async(actor,id,input,key,_change,state,version,appliedNodeId,stages,bindingSteps,now)=>{const operation='organization.node-change.review';
  stages.push(
    db.prepare("UPDATE organization_node_changes SET state=?,version=?,reviewed_by=?,review_reason=?,reviewed_at=?,applied_at=?,updated_at=? WHERE id=? AND company_id=?").bind(state,version,actor.userId,input.reason,now,input.decision==="approve"?now:null,now,id,actor.companyId),
    db.prepare("INSERT INTO organization_node_change_events(id,change_id,company_id,action,actor_id,version,snapshot_json,created_at) VALUES(?,?,?,?,?,?,?,?)").bind(`org-change-event-${key}`,id,actor.companyId,input.decision==="approve"?"applied":"rejected",actor.userId,version,JSON.stringify({decision:input.decision,reason:input.reason,appliedNodeId}),now),
    ...bindingSteps,
    organizationChangeNotice(db,actor.companyId,id,key,state),
  );
  return executeCommand(db,actor,key,operation,input,id,version,{id,version,state,nodeId:appliedNodeId},stages);
},saveChange:async(actor,id,input,key,submit,sourceBranchId,slug,now)=>{const recordId=id??`org-change-${key}`,operation=`organization.node-change.${submit?'submit':'save'}`,version=input.expectedVersion+1,state=submit?'submitted':'draft',result={id:recordId,version,state};
  return executeCommand(db,actor,key,operation,input,recordId,version,result,[
    ...(id?[assertion(db,key,"change-version","EXISTS(SELECT 1 FROM organization_node_changes WHERE id=? AND company_id=? AND version=? AND state IN ('draft','rejected'))",[id,actor.companyId,input.expectedVersion])]:[]),
    db.prepare(`INSERT INTO organization_node_changes(id,company_id,target_node_id,action,node_kind,slug,display_name,branch_id,source_branch_id,expected_node_version,state,version,requested_by,reason,created_at,updated_at)
      VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?) ON CONFLICT(id) DO UPDATE SET target_node_id=excluded.target_node_id,action=excluded.action,node_kind=excluded.node_kind,slug=excluded.slug,display_name=excluded.display_name,branch_id=excluded.branch_id,source_branch_id=excluded.source_branch_id,expected_node_version=excluded.expected_node_version,state=excluded.state,version=excluded.version,reason=excluded.reason,reviewed_by=NULL,review_reason=NULL,reviewed_at=NULL,updated_at=excluded.updated_at`).bind(recordId,actor.companyId,input.targetNodeId,input.action,input.kind,slug,input.displayName,input.branchId,sourceBranchId,input.expectedNodeVersion,state,version,actor.userId,input.reason,now,now),
    db.prepare("INSERT INTO organization_node_change_events(id,change_id,company_id,action,actor_id,version,snapshot_json,created_at) VALUES(?,?,?,?,?,?,?,?)").bind(`org-change-event-${key}`,recordId,actor.companyId,submit?"submitted":"drafted",actor.userId,version,JSON.stringify({...input,state}),now),
    ...(submit?[organizationChangeNotice(db,actor.companyId,recordId,key,"submitted")]:[]),
  ]);
},cancelChange:async(actor,id,expectedVersion,key,now)=>{const operation='organization.node-change.cancel',input={expectedVersion};
  const version=expectedVersion+1;return executeCommand(db,actor,key,operation,input,id,version,{id,version,state:"cancelled"},[
    assertion(db,key,"change-version","EXISTS(SELECT 1 FROM organization_node_changes WHERE id=? AND company_id=? AND version=? AND state IN ('draft','submitted','rejected'))",[id,actor.companyId,expectedVersion]),
    db.prepare("UPDATE organization_node_changes SET state='cancelled',version=?,updated_at=? WHERE id=? AND company_id=?").bind(version,now,id,actor.companyId),
    db.prepare("INSERT INTO organization_node_change_events(id,change_id,company_id,action,actor_id,version,snapshot_json,created_at) VALUES(?,?,?,?,?,?,?,?)").bind(`org-change-event-${key}`,id,actor.companyId,"cancelled",actor.userId,version,JSON.stringify(input),now),
    organizationChangeNotice(db,actor.companyId,id,key,"cancelled"),
  ]);
},
level:(companyId,id)=>db.prepare('SELECT level_number FROM staff_levels WHERE company_id=? AND id=? AND active=1').bind(companyId,id).first<{level_number:number|null}>(),positionLevel:(companyId,id)=>db.prepare('SELECT level_number FROM staff_levels WHERE id=? AND company_id=? AND active=1 AND level_number IS NOT NULL').bind(id,companyId).first<{level_number:number}>(),exitingEngagements:(companyId,id)=>organizationRosterExitEngagementIds(db,companyId,id),
savePosition:async(actor,id,input,key,guard,rosterRequired,exiting,now)=>{const recordId=id??`position-${key}`,operation='organization.position.save',version=input.expectedVersion+1;
  return executeCommand(db,actor,key,operation,input,recordId,version,{id:recordId,version},[guard,
    ...(id?[assertion(db,key,"position-version","EXISTS(SELECT 1 FROM organization_positions WHERE id=? AND company_id=? AND version=?)",[id,actor.companyId,input.expectedVersion])]:[]),
    db.prepare(`INSERT INTO organization_positions(id,company_id,branch_id,department_id,title,level_id,reports_to_position_id,is_ceo,roster_required,active,version,created_by,created_at,updated_at) VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?,?)
      ON CONFLICT(id) DO UPDATE SET branch_id=excluded.branch_id,department_id=excluded.department_id,title=excluded.title,level_id=excluded.level_id,reports_to_position_id=excluded.reports_to_position_id,is_ceo=excluded.is_ceo,roster_required=excluded.roster_required,active=excluded.active,version=excluded.version,updated_at=excluded.updated_at`).bind(recordId,actor.companyId,input.branchId,input.departmentId,input.title,input.levelId,input.reportsToPositionId,input.isCeo?1:0,rosterRequired?1:0,input.active?1:0,version,actor.userId,now,now),
    db.prepare("UPDATE organization_positions SET requirements=? WHERE id=? AND company_id=?").bind(input.requirements,recordId,actor.companyId),
    ...rosterPositionExitStatements(db,actor,exiting,key,input.reason),
  ]);
},installLevels:async(actor,key,titles)=>{
  return executeCommand(db,actor,key,"organization.levels.install",{},actor.companyId,1,{id:actor.companyId},titles.map((name,index)=>db.prepare("INSERT INTO staff_levels(id,company_id,code,name,rank,level_number,active,created_at,updated_at) VALUES(?,?,?,?,?,?,1,?,?) ON CONFLICT DO NOTHING").bind(`org-level-${actor.companyId}-${index+1}`,actor.companyId,`ORG_L${index+1}`,name,-index-1,index+1,Date.now(),Date.now())));
},saveTemplate:async(actor,id,input,key)=>{const recordId=id??`org-template-${key}`,version=input.expectedVersion+1;
  return executeCommand(db,actor,key,"organization.template.save",input,recordId,version,{id:recordId,version},[
    ...(id?[assertion(db,key,"template-version","EXISTS(SELECT 1 FROM organization_templates WHERE id=? AND company_id=? AND version=?)",[id,actor.companyId,input.expectedVersion])]:[]),
    db.prepare("INSERT INTO organization_templates(id,company_id,name,positions_json,version,created_by,created_at,updated_at) VALUES(?,?,?,?,?,?,?,?) ON CONFLICT(id) DO UPDATE SET name=excluded.name,positions_json=excluded.positions_json,version=excluded.version,updated_at=excluded.updated_at").bind(recordId,actor.companyId,input.name,JSON.stringify(input.positions),version,actor.userId,Date.now(),Date.now()),
  ]);
},template:(companyId,id)=>db.prepare('SELECT positions_json,version FROM organization_templates WHERE id=? AND company_id=?').bind(id,companyId).first<{positions_json:string;version:number}>(),ceoPosition:companyId=>db.prepare('SELECT id FROM organization_positions WHERE company_id=? AND is_ceo=1 AND active=1').bind(companyId).first<{id:string}>(),stageTemplatePosition:(actor,input,p,id,levelId,parentId)=>db.prepare('INSERT INTO organization_positions(id,company_id,branch_id,department_id,title,level_id,reports_to_position_id,is_ceo,roster_required,created_by,created_at,updated_at) VALUES(?,?,?,?,?,?,?,?,?,?,?,?)').bind(id,actor.companyId,input.branchId,input.departmentId,p.title,levelId,parentId,p.isCeo?1:0,p.rosterRequired?1:0,actor.userId,Date.now(),Date.now()),applyTemplate:(actor,id,input,key,guard,positionIds,stages)=>executeCommand(db,actor,key,'organization.template.apply',input,id,input.expectedVersion,{id,positionIds},[guard,assertion(db,key,'template-version','EXISTS(SELECT 1 FROM organization_templates WHERE id=? AND company_id=? AND version=?)',[id,actor.companyId,input.expectedVersion]),...stages])};}
async function invoke<T>(call:()=>Promise<T>):Promise<T>{try{return await call();}catch(error){if(error instanceof BusinessRuleError)throw new ApiError(error.status,error.code,error.message);throw error;}}
const application=(db:SqlDatabase)=>createOrganizationApplication(portsFor(db));
export const organizationOverview=(db:SqlDatabase,actor:WorkContext,query:Record<string,unknown>={})=>invoke(()=>application(db).overview(actor,query));
export const saveOrganizationNodeChange=(db:SqlDatabase,actor:WorkContext,id:string|null,input:NodeChangeInput,key:string,submit:boolean)=>invoke(()=>application(db).saveChange(actor,id,input,key,submit));
export const previewOrganizationNodeChange=(db:SqlDatabase,actor:WorkContext,input:NodeChangeInput)=>invoke(()=>application(db).previewChange(actor,input));
export const decideOrganizationNodeChange=(db:SqlDatabase,actor:WorkContext,id:string,input:NodeDecisionInput,key:string)=>invoke(()=>application(db).reviewChange(actor,id,input,key));
export const cancelOrganizationNodeChange=(db:SqlDatabase,actor:WorkContext,id:string,version:number,key:string)=>invoke(()=>application(db).cancelChange(actor,id,version,key));
export const installStaffLevels=(db:SqlDatabase,actor:WorkContext,key:string)=>invoke(()=>application(db).installLevels(actor,key));
export const savePosition=(db:SqlDatabase,actor:WorkContext,id:string|null,input:PositionInput,key:string)=>invoke(()=>application(db).savePosition(actor,id,input,key));
export const saveOrganizationTemplate=(db:SqlDatabase,actor:WorkContext,id:string|null,input:TemplateInput,key:string)=>invoke(()=>application(db).saveTemplate(actor,id,input,key));
export const applyOrganizationTemplate=(db:SqlDatabase,actor:WorkContext,id:string,input:ApplyTemplateInput,key:string)=>invoke(()=>application(db).applyTemplate(actor,id,input,key));
export const validateStaffOrganization=(db:SqlDatabase,companyId:string,person:Pick<OrganizationPerson,'person_id'|'branch_id'|'department_id'|'position_id'|'level_id'|'manager_person_id'>,required:boolean)=>invoke(()=>application(db).validateStaff(companyId,person,required));
