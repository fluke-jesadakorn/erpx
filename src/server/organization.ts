import { canReadOrganizationChanges, organizationChangeNotice } from "./organization-work";
import { requirePermission } from "@/server/iam-permissions";
import { hasPermission } from "@/lib/permissions";
import { samePerson } from "@/lib/iam";
import { organizationMemberSql } from "./organization-channels";
import { spendingTemplate } from "./spending-templates";
import { z } from "zod";
import type { WorkContext } from "./authorization";
import type { SqlDatabase } from "./runtime/types";
import { ApiError } from "./http";
import { assertion, executeCommand, replayCommand } from "./commands";
import { reportingIssues, staffLevelTitles, type OrganizationOverview, type OrganizationPerson, type OrganizationPosition, type OrganizationTemplatePosition, type OrganizationNode, type OrganizationNodeChange } from "@/lib/organization";
import { canSeeDefaultPay, defaultCatalog } from "./staff-defaults";
import { prepareAttachmentBinding } from "./attachments/service";
import { rosterPositionExitStatements } from "./roster-position-exit";
import { sha256 } from "./crypto";

export const positionSchema = z.object({
  requirements: z.string().trim().max(3000).default(""),
  title: z.string().trim().min(2).max(160), branchId: z.string().nullable(), departmentId: z.string().nullable(),
  levelId: z.string().min(1), reportsToPositionId: z.string().nullable(), isCeo: z.boolean().default(false),
  rosterRequired: z.boolean().optional(),
  active: z.boolean().default(true), expectedVersion: z.number().int().min(0), reason: z.string().trim().min(3).max(1000),
});
const templatePositionSchema = z.object({key:z.string().min(1).max(100),title:z.string().trim().min(2).max(160),levelNumber:z.number().int().min(1).max(6),reportsToKey:z.string().nullable(),isCeo:z.boolean(),rosterRequired:z.boolean().default(false)});
export const organizationTemplateSchema = z.object({name:z.string().trim().min(2).max(120),positions:z.array(templatePositionSchema).min(1).max(100),expectedVersion:z.number().int().min(0)});
export const applyOrganizationTemplateSchema = z.object({expectedVersion:z.number().int().positive(),branchId:z.string().nullable(),departmentId:z.string().nullable(),reason:z.string().trim().min(3).max(1000)});
export const organizationNodeChangeSchema=z.object({
  action:z.enum(["create","rename","suspend","reactivate","archive","relocate"]),targetNodeId:z.string().min(1).nullable().default(null),
  kind:z.enum(["branch","department"]),slug:z.string().trim().regex(/^[a-z0-9]+(?:-[a-z0-9]+)*$/).max(80).nullable().default(null),
  displayName:z.string().trim().min(2).max(120),branchId:z.string().min(1).nullable().default(null),
  expectedNodeVersion:z.number().int().positive().nullable().default(null),expectedVersion:z.number().int().min(0),reason:z.string().trim().min(3).max(1000),
  previewSnapshot:z.string().regex(/^[a-f0-9]{64}$/).optional(),
});
export const organizationNodeDecisionSchema=z.object({expectedVersion:z.number().int().positive(),decision:z.enum(["approve","reject"]),reason:z.string().trim().min(3).max(1000),attachmentSetIds:z.array(z.string().min(1)).max(20).optional()});
const organizationChangeFiltersSchema=z.object({
  changeId:z.string().trim().min(1).max(200).nullable().catch(null),
  changeState:z.enum(["open","all","draft","submitted","rejected","applied","cancelled"]).catch("open"),
  changeQuery:z.string().trim().max(120).catch(""),
  changePage:z.coerce.number().int().positive().max(100000).catch(1),
});

// One snapshot protects graph validation against concurrent edits in a command batch.
const graphSql = `SELECT jsonb_build_object(
 'people',(SELECT jsonb_agg(to_jsonb(p) ORDER BY p.id) FROM hr_people p WHERE p.company_id=?),
 'engagements',(SELECT jsonb_agg(to_jsonb(e) ORDER BY e.id) FROM hr_engagements e WHERE e.company_id=?),
 'positions',(SELECT jsonb_agg(to_jsonb(p) ORDER BY p.id) FROM organization_positions p WHERE p.company_id=?),
 'levels',(SELECT jsonb_agg(to_jsonb(l) ORDER BY l.id) FROM staff_levels l WHERE l.company_id=?),
 'nodes',(SELECT jsonb_agg(to_jsonb(n) ORDER BY n.id) FROM organization_nodes n WHERE n.company_id=?)
)::text snapshot`;
export async function organizationGuard(db: SqlDatabase, companyId: string, key: string) {
  const args = Array(5).fill(companyId) as string[];
  const row = await db.prepare(graphSql).bind(...args).first<{snapshot:string}>();
  return assertion(db,key,"organization-snapshot",`((${graphSql})::jsonb = ?::jsonb)`,[...args,row!.snapshot]);
}
export async function organizationRoster(db: SqlDatabase, companyId: string) {
  const [people, positions] = await Promise.all([
    db.prepare(`SELECT p.id person_id,p.user_id,p.legal_name,p.email,p.version person_version,p.status person_status,e.id engagement_id,e.version engagement_version,
      e.company_id,e.branch_id,e.department_id,e.position_id,e.manager_person_id,e.level_id,l.level_number,l.name level_name,e.status,e.start_date,e.end_date,e.job_title
      FROM hr_people p JOIN hr_engagements e ON e.person_id=p.id AND e.company_id=p.company_id
      LEFT JOIN staff_levels l ON l.id=e.level_id AND l.company_id=e.company_id AND l.active=1
      WHERE p.company_id=? AND p.merged_into_id IS NULL AND p.status!='archived' AND e.status!='ended' ORDER BY p.legal_name,e.id`).bind(companyId).all<OrganizationPerson>(),
    db.prepare("SELECT p.*,l.level_number FROM organization_positions p JOIN staff_levels l ON l.id=p.level_id AND l.company_id=p.company_id WHERE p.company_id=? ORDER BY l.level_number,p.title,p.id").bind(companyId).all<OrganizationPosition>(),
  ]);
  return {people:people.results,positions:positions.results};
}
export async function organizationOverview(db: SqlDatabase, actor: WorkContext, query:Record<string,unknown> = {}): Promise<OrganizationOverview> {
  const {people,positions} = await organizationRoster(db,actor.companyId);
  const visible = (p:{branch_id:string|null;department_id:string|null}) => (!actor.branchId || p.branch_id === actor.branchId) && (!actor.departmentId || p.department_id === actor.departmentId);
  const canManage = hasPermission(actor,"organization.manage");
  const canReadHrDetails = hasPermission(actor,"people.manage");
  const canManageUnits = await canReadOrganizationChanges(db,actor);
  const nodes = (await db.prepare("SELECT id,kind,slug,display_name,branch_id,status,hierarchy_revision FROM organization_nodes WHERE company_id=? AND kind IN ('company','branch','department') AND (?::text IS NULL OR id=? OR branch_id=?) AND (?::text IS NULL OR kind!='department' OR id=?) ORDER BY status,display_name").bind(actor.companyId,actor.branchId,actor.branchId,actor.branchId,actor.departmentId,actor.departmentId).all<OrganizationNode>()).results;
  const levels = (await db.prepare("SELECT id,name,level_number FROM staff_levels WHERE company_id=? AND active=1 AND level_number IS NOT NULL ORDER BY level_number").bind(actor.companyId).all<{id:string;name:string;level_number:number}>()).results;
  const templates = canManage ? (await db.prepare("SELECT id,name,version,positions_json FROM organization_templates WHERE company_id=? ORDER BY name").bind(actor.companyId).all<{id:string;name:string;version:number;positions_json:string}>()).results.map(t=>({id:t.id,name:t.name,version:t.version,positions:JSON.parse(t.positions_json) as OrganizationTemplatePosition[]})) : [];
  const authorizedChannelNodes=(await db.prepare(`SELECT n.id FROM organization_nodes n WHERE n.company_id=? AND n.status='active' AND n.kind IN ('company','branch','department') AND ${organizationMemberSql("n","?")}`).bind(actor.companyId,actor.userId).all<{id:string}>()).results.map(n=>n.id);
  const managerGrants = canManage ? (await db.prepare("SELECT user_id,scope_kind,scope_node_id FROM assignments WHERE company_id=? AND iam_role_allows(role,company_id,'spending.approve') AND revoked_at IS NULL AND iam_assignment_active(id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) AND iam_principal_active(user_id,company_id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) AND valid_from<=? AND valid_until>?").bind(actor.companyId,Date.now(),Date.now()).all<{user_id:string;scope_kind:string;scope_node_id:string|null}>()).results : [];
  const managerIssues = (person:OrganizationPerson) => canManage && people.filter(p=>p.manager_person_id===person.person_id && p.status==="active" && visible(p)).some(report=>!managerGrants.some(a=>a.user_id===person.user_id && (a.scope_kind==="holding" || a.scope_kind==="branch" && a.scope_node_id===report.branch_id || a.scope_kind==="department" && a.scope_node_id===report.department_id))) ? ["People Manager access is missing for one or more reporting teams. Open the profile and review scoped role assignments."] : [];
  const filters=organizationChangeFiltersSchema.parse(query),pageSize=20;
  if(filters.changeId){filters.changeState="all";filters.changeQuery="";}
  const stateFilter=filters.changeState==="open"?"state IN ('draft','submitted','rejected')":filters.changeState==="all"?"TRUE":"state=?";
  const predicate=`company_id=? ${filters.changeId?"AND id=?":""} AND ${stateFilter} AND (display_name ILIKE ? OR reason ILIKE ?)`;
  const search=`%${filters.changeQuery.replace(/[\\%_]/g,"\\$&")}%`;
  const args=[actor.companyId,...(filters.changeId?[filters.changeId]:[]),...(["open","all"].includes(filters.changeState)?[]:[filters.changeState]),search,search];
  const total=canManageUnits?Number((await db.prepare(`SELECT count(*) total FROM organization_node_changes WHERE ${predicate}`).bind(...args).first<{total:number}>())?.total??0):0;
  const pages=Math.max(1,Math.ceil(total/pageSize)),page=Math.min(filters.changePage,pages);
  const nodeChanges=canManageUnits?(await db.prepare(`SELECT *,NOT iam_same_person(requested_by,?,company_id) independent_reviewer FROM organization_node_changes WHERE ${predicate} ORDER BY updated_at DESC,id LIMIT ? OFFSET ?`).bind(actor.userId,...args,pageSize,(page-1)*pageSize).all<OrganizationNodeChange & {independent_reviewer:boolean}>()).results:[];
  if(nodeChanges.length){
    const events=(await db.prepare(`SELECT e.change_id,e.action,e.version,e.created_at,e.snapshot_json,COALESCE(p.legal_name,e.actor_id) actor_name
      FROM organization_node_change_events e LEFT JOIN hr_people p ON p.user_id=e.actor_id AND p.company_id=e.company_id AND p.merged_into_id IS NULL
      WHERE e.company_id=? AND e.change_id IN (${nodeChanges.map(()=>"?").join(",")}) ORDER BY e.version,e.created_at,e.id`)
      .bind(actor.companyId,...nodeChanges.map(c=>c.id)).all<{change_id:string;action:string;version:number;created_at:number;snapshot_json:string;actor_name:string}>()).results;
    for(const change of nodeChanges){change.canReview=change.state==="submitted"&&change.independent_reviewer&&!samePerson(actor,change.requested_by);change.events=events.filter(e=>e.change_id===change.id).map(e=>{
      const snapshot=JSON.parse(e.snapshot_json) as {reason?:string};
      return {action:e.action,actor_name:e.actor_name,version:e.version,created_at:e.created_at,reason:snapshot.reason??null};
    });}
  }
  return {authorizedChannelNodes,spendingTemplate:canManage?await spendingTemplate(db,actor.companyId):undefined,canManageSpending:canManage&&actor.assignmentScope==="holding",positions:positions.filter(p=>(!actor.branchId||!p.branch_id||p.branch_id===actor.branchId)&&(!actor.departmentId||!p.department_id||p.department_id===actor.departmentId)),people:people.filter(visible).map(p=>({...p,email:canReadHrDetails?p.email:null,start_date:canReadHrDetails?p.start_date:"",end_date:canReadHrDetails?p.end_date:null,issues:[...reportingIssues(p,people,positions),...managerIssues(p)]})),nodes,levels,templates,nodeChanges,changeQueue:{recordId:filters.changeId,state:filters.changeState,query:filters.changeQuery,page,pageSize,total,pages},actorId:actor.userId,canManage,canManageUnits,
    defaults:await defaultCatalog(db,actor),
    monthlyPayGroups:canSeeDefaultPay(actor)?(await db.prepare("SELECT id,name,currency FROM hr_pay_groups WHERE company_id=? AND status='active' AND frequency='monthly' ORDER BY name").bind(actor.companyId).all<{id:string;name:string;currency:string}>()).results:[],
  };
}

function requireCompanyOrganizationAuthority(actor:WorkContext){
  actor=requirePermission(actor,"organization.manage");
  if(actor.assignmentScope!=="holding"||actor.scope.kind!=="holding")throw new ApiError(403,"COMPANY_AUTHORITY_REQUIRED","Open the Company workspace with Company HR authority to manage branches and departments.");
  return actor;
}
async function validateNodeChange(db:SqlDatabase,actor:WorkContext,input:z.infer<typeof organizationNodeChangeSchema>){
  const relocation=input.action==="relocate";
  if(input.action==="create"){
    if(input.targetNodeId||input.expectedNodeVersion)throw new ApiError(422,"NEW_NODE_INVALID","A new unit cannot reference an existing node version.");
    if(!input.slug)throw new ApiError(422,"SLUG_REQUIRED","Choose a stable URL code for the new unit.");
  }else if(!input.targetNodeId||!input.expectedNodeVersion)throw new ApiError(422,"NODE_VERSION_REQUIRED","Reload and choose the current organization unit.");
  if(input.kind==="branch"&&input.branchId)throw new ApiError(422,"BRANCH_PARENT_INVALID","A branch belongs directly to the company.");
  if(input.kind==="department"){
    if(!input.branchId)throw new ApiError(422,"DEPARTMENT_BRANCH_REQUIRED","Choose the department's branch.");
    if(!await db.prepare("SELECT 1 FROM organization_nodes WHERE id=? AND company_id=? AND kind='branch' AND status='active'").bind(input.branchId,actor.companyId).first())throw new ApiError(422,"BRANCH_INVALID","Choose an active branch in this company.");
  }
  if(input.action!=="create"){
    const node=await db.prepare("SELECT id,kind,slug,display_name,branch_id,status,hierarchy_revision FROM organization_nodes WHERE id=? AND company_id=? AND kind IN ('branch','department')").bind(input.targetNodeId,actor.companyId).first<OrganizationNode>();
    if(!node)throw new ApiError(404,"ORGANIZATION_NODE_NOT_FOUND","Organization unit not found.");
    if(node.kind!==input.kind||(!relocation&&node.branch_id!==input.branchId))throw new ApiError(422,"NODE_PLACEMENT_CHANGED","Reload the organization unit and review its placement.");
    if(relocation){
      if(input.expectedVersion===0&&await db.prepare("SELECT 1 FROM organization_draft_reservations r WHERE r.company_id=? AND ((r.kind='position' AND EXISTS(SELECT 1 FROM organization_positions p WHERE p.id=r.entity_id AND p.company_id=r.company_id AND p.department_id=?)) OR (r.kind='person' AND EXISTS(SELECT 1 FROM hr_engagements e WHERE e.id=r.entity_id AND e.company_id=r.company_id AND e.department_id=?))) LIMIT 1").bind(actor.companyId,node.id,node.id).first())throw new ApiError(409,"ORGANIZATION_DRAFT_RESERVED","This department contains changes reserved by a confirmed organization draft.");
      if(node.kind!=="department")throw new ApiError(422,"RELOCATION_REQUIRES_DEPARTMENT","Only a department can be relocated between branches.");
      if(node.status!=="active")throw new ApiError(409,"NODE_STATE_CHANGED","Only an active department can be relocated.");
      if(node.hierarchy_revision!==input.expectedNodeVersion)throw new ApiError(409,"NODE_VERSION_CHANGED","The department changed. Preview the relocation again.");
      if(node.branch_id===input.branchId)throw new ApiError(409,"RELOCATION_UNCHANGED","Choose a different active branch for relocation.");
      if(await db.prepare("SELECT 1 FROM organization_nodes WHERE company_id=? AND parent_id=? AND status<>'archived' LIMIT 1").bind(actor.companyId,node.id).first())
        throw new ApiError(409,"RELOCATION_HAS_CHILDREN","Move or archive the department's child units before relocating it.");
      if(await db.prepare("SELECT 1 FROM organization_nodes WHERE company_id=? AND parent_id=? AND slug=? LIMIT 1").bind(actor.companyId,input.branchId,node.slug).first())
        throw new ApiError(409,"RELOCATION_SLUG_CONFLICT","A department with this URL code already exists in the destination branch.");
    }else if(input.action==="suspend"&&node.status!=="active"||input.action==="archive"&&node.status!=="active"||input.action==="reactivate"&&!(["suspended","archived"] as string[]).includes(node.status))throw new ApiError(409,"NODE_STATE_CHANGED","The organization unit is already in the requested state.");
  }
}
export async function saveOrganizationNodeChange(db:SqlDatabase,actor:WorkContext,id:string|null,input:z.infer<typeof organizationNodeChangeSchema>,key:string,submit:boolean){
  actor=requireCompanyOrganizationAuthority(actor);const recordId=id??`org-change-${key}`,operation=`organization.node-change.${submit?"submit":"save"}`;
  const prior=await replayCommand(db,actor,key,operation,recordId,input);if(prior)return prior;
  await validateNodeChange(db,actor,input);
  if(input.action==="relocate" && input.previewSnapshot){
    const currentSnapshot=await relocationSnapshot(db,actor.companyId,input.targetNodeId!);
    if(currentSnapshot!==input.previewSnapshot)throw new ApiError(409,"ORGANIZATION_RELOCATION_CHANGED","The department's people, positions, or access assignments changed. Preview the move again.");
  }
  const current=id?await db.prepare("SELECT * FROM organization_node_changes WHERE id=? AND company_id=?").bind(id,actor.companyId).first<OrganizationNodeChange>():null;
  if(id&&!current)throw new ApiError(404,"ORGANIZATION_CHANGE_NOT_FOUND","Organization change not found.");
  if(current&&current.requested_by!==actor.userId)throw new ApiError(403,"CHANGE_OWNER_REQUIRED","Only the requester can correct this organization change.");
  if(current&&!['draft','rejected'].includes(current.state))throw new ApiError(409,"CHANGE_LOCKED","Only draft or rejected changes can be edited.");
  const target=input.targetNodeId?await db.prepare("SELECT branch_id,slug FROM organization_nodes WHERE id=? AND company_id=?").bind(input.targetNodeId,actor.companyId).first<{branch_id:string|null;slug:string}>():null;
  const sourceBranchId=input.action==="relocate"?(target?.branch_id??current?.source_branch_id??null):null;
  const slug=input.action==="relocate"?(target?.slug??current?.slug??null):input.slug;
  const version=input.expectedVersion+1,state=submit?"submitted":"draft",now=Date.now();
  const result={id:recordId,version,state};
  return executeCommand(db,actor,key,operation,input,recordId,version,result,[
    ...(id?[assertion(db,key,"change-version","EXISTS(SELECT 1 FROM organization_node_changes WHERE id=? AND company_id=? AND version=? AND state IN ('draft','rejected'))",[id,actor.companyId,input.expectedVersion])]:[]),
    db.prepare(`INSERT INTO organization_node_changes(id,company_id,target_node_id,action,node_kind,slug,display_name,branch_id,source_branch_id,expected_node_version,state,version,requested_by,reason,created_at,updated_at)
      VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?) ON CONFLICT(id) DO UPDATE SET target_node_id=excluded.target_node_id,action=excluded.action,node_kind=excluded.node_kind,slug=excluded.slug,display_name=excluded.display_name,branch_id=excluded.branch_id,source_branch_id=excluded.source_branch_id,expected_node_version=excluded.expected_node_version,state=excluded.state,version=excluded.version,reason=excluded.reason,reviewed_by=NULL,review_reason=NULL,reviewed_at=NULL,updated_at=excluded.updated_at`).bind(recordId,actor.companyId,input.targetNodeId,input.action,input.kind,slug,input.displayName,input.branchId,sourceBranchId,input.expectedNodeVersion,state,version,actor.userId,input.reason,now,now),
    db.prepare("INSERT INTO organization_node_change_events(id,change_id,company_id,action,actor_id,version,snapshot_json,created_at) VALUES(?,?,?,?,?,?,?,?)").bind(`org-change-event-${key}`,recordId,actor.companyId,submit?"submitted":"drafted",actor.userId,version,JSON.stringify({...input,state}),now),
    ...(submit?[organizationChangeNotice(db,actor.companyId,recordId,key,"submitted")]:[]),
  ]);
}

const nodeImpactSql=`SELECT
 (SELECT count(*) FROM organization_nodes WHERE company_id=? AND parent_id=? AND status='active') child_nodes,
 (SELECT count(*) FROM hr_engagements WHERE company_id=? AND status='active' AND (branch_id=? OR department_id=?)) staff,
 (SELECT count(*) FROM organization_positions WHERE company_id=? AND active=1 AND (branch_id=? OR department_id=?)) positions,
 (SELECT count(*) FROM assignments WHERE company_id=? AND revoked_at IS NULL AND scope_node_id=?) assignments,
 (SELECT count(*) FROM commerce_warehouses WHERE company_id=? AND branch_id=?) warehouses`;
export async function organizationNodeImpact(db:SqlDatabase,companyId:string,nodeId:string){
  return (await db.prepare(nodeImpactSql).bind(companyId,nodeId,companyId,nodeId,nodeId,companyId,nodeId,nodeId,companyId,nodeId,companyId,nodeId).first<{child_nodes:number;staff:number;positions:number;assignments:number;warehouses:number}>())!;
}
async function relocationSnapshot(db:SqlDatabase,companyId:string,departmentId:string){
  const [engagements,positions,assignments,rules,head,legacyHead,flow]=await Promise.all([
    db.prepare("SELECT id,version,status FROM hr_engagements WHERE company_id=? AND department_id=? AND status IN ('draft','onboarding','active','suspended') ORDER BY id").bind(companyId,departmentId).all(),
    db.prepare("SELECT id,version FROM organization_positions WHERE company_id=? AND department_id=? AND active=1 ORDER BY id").bind(companyId,departmentId).all(),
    db.prepare("SELECT id,version FROM assignments WHERE company_id=? AND scope_kind='department' AND scope_node_id=? AND revoked_at IS NULL ORDER BY id").bind(companyId,departmentId).all(),
    db.prepare("SELECT id,version FROM organization_gpo_rules WHERE company_id=? AND target_kind='department' AND target_id=? ORDER BY id").bind(companyId,departmentId).all(),
    db.prepare("SELECT version,position_id,person_id,effective_at FROM authority_seats WHERE company_id=? AND kind='head' AND scope_id=?").bind(companyId,departmentId).first(),
    db.prepare("SELECT version,person_id FROM department_heads WHERE company_id=? AND department_id=?").bind(companyId,departmentId).first(),
    db.prepare("SELECT id,version,steps_json FROM department_flow_versions WHERE company_id=? AND department_id=? ORDER BY version DESC LIMIT 1").bind(companyId,departmentId).first(),
  ]);
  return sha256(JSON.stringify({engagements:engagements.results,positions:positions.results,assignments:assignments.results,rules:rules.results,head,legacyHead,flow}));
}
export async function previewOrganizationNodeChange(db:SqlDatabase,actor:WorkContext,input:z.infer<typeof organizationNodeChangeSchema>){
  actor=requireCompanyOrganizationAuthority(actor);
  if(input.action!=="relocate" || input.kind!=="department")throw new ApiError(422,"RELOCATION_PREVIEW_REQUIRED","Choose a department relocation.");
  await validateNodeChange(db,actor,input);
  const [impact,previewSnapshot]=await Promise.all([
    organizationNodeImpact(db,actor.companyId,input.targetNodeId!),
    relocationSnapshot(db,actor.companyId,input.targetNodeId!),
  ]);
  return {impact,previewSnapshot,sourceBranchId:(await db.prepare("SELECT branch_id FROM organization_nodes WHERE id=? AND company_id=?").bind(input.targetNodeId,actor.companyId).first<{branch_id:string}>())?.branch_id ?? null,destinationBranchId:input.branchId};
}
export async function decideOrganizationNodeChange(db:SqlDatabase,actor:WorkContext,id:string,input:z.infer<typeof organizationNodeDecisionSchema>,key:string){
  actor=requireCompanyOrganizationAuthority(actor);const operation="organization.node-change.review";
  const prior=await replayCommand(db,actor,key,operation,id,input);if(prior)return prior;
  const change=await db.prepare("SELECT * FROM organization_node_changes WHERE id=? AND company_id=?").bind(id,actor.companyId).first<OrganizationNodeChange>();
  if(!change)throw new ApiError(404,"ORGANIZATION_CHANGE_NOT_FOUND","Organization change not found.");
  if(change.state!=="submitted")throw new ApiError(409,"CHANGE_NOT_SUBMITTED","Only submitted organization changes can be reviewed.");
  if(samePerson(actor,change.requested_by)||(await db.prepare("SELECT iam_same_person(?,?,?) same_person").bind(actor.userId,change.requested_by,actor.companyId).first<{same_person:boolean}>())?.same_person)throw new ApiError(403,"INDEPENDENT_REVIEW_REQUIRED","Another company HR administrator must review this change.");
  if(change.version!==input.expectedVersion)throw new ApiError(409,"STALE_VERSION","The organization change was updated. Reload and review it again.");
  if(input.decision==="approve" && change.action==="relocate"){
    const submitted=await db.prepare("SELECT snapshot_json FROM organization_node_change_events WHERE change_id=? AND action='submitted' ORDER BY created_at DESC,id DESC LIMIT 1").bind(id).first<{snapshot_json:string}>();
    const saved=JSON.parse(submitted?.snapshot_json ?? "{}") as {previewSnapshot?:string};
    if(saved.previewSnapshot && saved.previewSnapshot!==await relocationSnapshot(db,actor.companyId,change.target_node_id!))throw new ApiError(409,"ORGANIZATION_RELOCATION_CHANGED","The department's people, positions, or access assignments changed. Ask HR to preview and submit the move again.");
  }
  const now=Date.now(),version=change.version+1,state=input.decision==="approve"?"applied":"rejected";
  const attachmentSetIds=input.attachmentSetIds??[];
  let attachmentBinding:Awaited<ReturnType<typeof prepareAttachmentBinding>>|null=null;
  if(attachmentSetIds.length){
    const placeholders=attachmentSetIds.map(()=>"?").join(","),sets=(await db.prepare(`SELECT id,purpose_key,component_kind,component_ref FROM attachment_sets WHERE company_id=? AND id IN (${placeholders})`).bind(actor.companyId,...attachmentSetIds).all<{id:string;purpose_key:string;component_kind:string;component_ref:string|null}>()).results;
    if(sets.length!==new Set(attachmentSetIds).size||sets.some(set=>set.purpose_key!=="org_change_proof"||set.component_kind!=="action"||set.component_ref!=="organization-change-review"))throw new ApiError(422,"ORGANIZATION_ATTACHMENT_PURPOSE","Choose only organization-change proof prepared for this review.");
    attachmentBinding=await prepareAttachmentBinding(db,actor,{attachmentSetIds,bindingContract:{targetType:"organization_change",bindingType:"organization_change_review",contexts:[{purposeKey:"org_change_proof",componentKind:"action",componentRef:"organization-change-review"}]},targetId:id,expectedTargetVersion:change.version,commandId:key,bindingId:`${id}:${input.decision}:${version}`});
  }
  const statements=[...(attachmentBinding?.guards??[]),assertion(db,key,"change-version","EXISTS(SELECT 1 FROM organization_node_changes WHERE id=? AND company_id=? AND version=? AND state='submitted')",[id,actor.companyId,input.expectedVersion]),assertion(db,key,"independent-review","NOT iam_same_person(?,?,?)",[actor.userId,change.requested_by,actor.companyId])];
  let appliedNodeId=change.target_node_id;
  if(input.decision==="approve"){
    const candidate=organizationNodeChangeSchema.parse({action:change.action,targetNodeId:change.target_node_id,kind:change.node_kind,slug:change.slug,displayName:change.display_name,branchId:change.branch_id,expectedNodeVersion:change.expected_node_version,expectedVersion:change.version,reason:change.reason});
    await validateNodeChange(db,actor,candidate);
    if(change.action==="create"){
      appliedNodeId=`org-node-${id}`;const parent=change.node_kind==="branch"?actor.companyId:change.branch_id;
      statements.push(db.prepare("INSERT INTO organization_nodes(id,kind,slug,display_name,parent_id,company_id,branch_id,status,hierarchy_revision) VALUES(?,?,?,?,?,?,?,'active',1)").bind(appliedNodeId,change.node_kind,change.slug,change.display_name,parent,actor.companyId,change.branch_id));
    }else{
      const expectedBranch=change.action==="relocate"?change.source_branch_id:change.branch_id;
      statements.push(assertion(db,key,"node-version","EXISTS(SELECT 1 FROM organization_nodes WHERE id=? AND company_id=? AND hierarchy_revision=? AND kind=? AND branch_id IS NOT DISTINCT FROM ?)",[change.target_node_id,actor.companyId,change.expected_node_version!,change.node_kind,expectedBranch]));
      if(change.action==="suspend"||change.action==="archive"){
        const impact=await organizationNodeImpact(db,actor.companyId,change.target_node_id!);const blocking=Object.entries(impact).filter(([,count])=>Number(count)>0);
        if(blocking.length)throw new ApiError(409,"ORGANIZATION_NODE_IN_USE",`Move or close active dependencies before ${change.action}: ${blocking.map(([name,count])=>`${name.replace('_',' ')} ${count}`).join(', ')}.`);
      }
      if(change.action==="relocate"){
        appliedNodeId=`org-node-${id}`;
        const [head,legacyHead,flow]=await Promise.all([
          db.prepare("SELECT version,position_id,person_id,effective_at FROM authority_seats WHERE company_id=? AND kind='head' AND scope_id=?").bind(actor.companyId,change.target_node_id).first<{version:number;position_id:string|null;person_id:string|null;effective_at:number|null}>(),
          db.prepare("SELECT version,person_id FROM department_heads WHERE company_id=? AND department_id=?").bind(actor.companyId,change.target_node_id).first<{version:number;person_id:string}>(),
          db.prepare("SELECT id,version,steps_json FROM department_flow_versions WHERE company_id=? AND department_id=? ORDER BY version DESC LIMIT 1").bind(actor.companyId,change.target_node_id).first<{id:string;version:number;steps_json:unknown}>(),
        ]);
        statements.push(
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
          db.prepare("UPDATE hr_engagements SET branch_id=?,department_id=?,updated_at=? WHERE company_id=? AND department_id=? AND status IN ('draft','onboarding','active','suspended')").bind(change.branch_id,appliedNodeId,now,actor.companyId,change.target_node_id),
          db.prepare("UPDATE employees SET branch_id=?,department_id=? WHERE company_id=? AND department_id=? AND status='active'").bind(change.branch_id,appliedNodeId,actor.companyId,change.target_node_id),
          db.prepare("UPDATE organization_positions SET branch_id=?,department_id=?,updated_at=? WHERE company_id=? AND department_id=? AND active=1").bind(change.branch_id,appliedNodeId,now,actor.companyId,change.target_node_id),
          db.prepare("UPDATE assignments SET branch_id=?,scope_node_id=?,version=version+1 WHERE company_id=? AND scope_kind='department' AND scope_node_id=? AND revoked_at IS NULL").bind(change.branch_id,appliedNodeId,actor.companyId,change.target_node_id),
          db.prepare("INSERT INTO authority_seats(company_id,kind,scope_id,version,position_id,person_id,effective_at) SELECT company_id,kind,?,1,position_id,person_id,effective_at FROM authority_seats WHERE company_id=? AND kind='head' AND scope_id=?").bind(appliedNodeId,actor.companyId,change.target_node_id),
          db.prepare("INSERT INTO authority_appointments(id,company_id,kind,scope_id,version,position_id,person_id,effective_at,appointed_by,reason,provenance,source_reference,command_id,created_at) SELECT ?,company_id,'head',?,1,position_id,person_id,COALESCE(effective_at,?::bigint),?,'Carried forward during department relocation','explicit',?,?,? FROM authority_seats WHERE company_id=? AND kind='head' AND scope_id=?").bind(`appointment-relocated-${id}`,appliedNodeId,now,actor.userId,change.target_node_id,`appointment-relocated-${id}`,now,actor.companyId,change.target_node_id),
          db.prepare("INSERT INTO department_heads(department_id,company_id,person_id,version,updated_by,updated_at) SELECT ?,company_id,person_id,1,?,?::bigint FROM authority_seats WHERE company_id=? AND kind='head' AND scope_id=? AND person_id IS NOT NULL UNION ALL SELECT ?,legacy.company_id,legacy.person_id,1,?,?::bigint FROM department_heads legacy WHERE legacy.company_id=? AND legacy.department_id=? AND NOT EXISTS(SELECT 1 FROM authority_seats s WHERE s.company_id=legacy.company_id AND s.kind='head' AND s.scope_id=legacy.department_id)").bind(appliedNodeId,actor.userId,now,actor.companyId,change.target_node_id,appliedNodeId,actor.userId,now,actor.companyId,change.target_node_id),
          db.prepare("INSERT INTO department_flow_versions(id,company_id,department_id,version,steps_json,published_by,reason,command_id,created_at) SELECT ?,company_id,?,1,steps_json,?,'Carried forward during department relocation',?,? FROM department_flow_versions WHERE company_id=? AND department_id=? ORDER BY version DESC LIMIT 1").bind(`department-flow-relocated-${id}`,appliedNodeId,actor.userId,`department-flow-relocated-${id}`,now,actor.companyId,change.target_node_id),
        );
      }else{
        const status=change.action==="suspend"?"suspended":change.action==="archive"?"archived":change.action==="reactivate"?"active":null;
        statements.push(db.prepare("UPDATE organization_nodes SET display_name=?,status=COALESCE(?,status),hierarchy_revision=hierarchy_revision+1 WHERE id=? AND company_id=?").bind(change.display_name,status,change.target_node_id,actor.companyId));
      }
    }
    statements.push(
      db.prepare("INSERT INTO messaging_teams(id,company_id,name,owner_id,organization_managed,created_at) VALUES(?,?,'Organization','organization',true,?) ON CONFLICT DO NOTHING").bind(`org-team-${actor.companyId}`,actor.companyId,now),
      db.prepare("INSERT INTO messaging_rooms(id,company_id,kind,name,owner_id,team_id,organization_node_id,created_at) SELECT ?,?,'channel',?,'organization',?,?,? WHERE EXISTS(SELECT 1 FROM organization_nodes WHERE id=? AND company_id=? AND status='active') ON CONFLICT DO NOTHING").bind(`org-channel-${appliedNodeId}`,actor.companyId,change.display_name,`org-team-${actor.companyId}`,appliedNodeId,now,appliedNodeId,actor.companyId),
      db.prepare("UPDATE messaging_rooms SET name=? WHERE company_id=? AND organization_node_id=?").bind(change.display_name,actor.companyId,appliedNodeId),
    );
  }
  statements.push(
    db.prepare("UPDATE organization_node_changes SET state=?,version=?,reviewed_by=?,review_reason=?,reviewed_at=?,applied_at=?,updated_at=? WHERE id=? AND company_id=?").bind(state,version,actor.userId,input.reason,now,input.decision==="approve"?now:null,now,id,actor.companyId),
    db.prepare("INSERT INTO organization_node_change_events(id,change_id,company_id,action,actor_id,version,snapshot_json,created_at) VALUES(?,?,?,?,?,?,?,?)").bind(`org-change-event-${key}`,id,actor.companyId,input.decision==="approve"?"applied":"rejected",actor.userId,version,JSON.stringify({decision:input.decision,reason:input.reason,appliedNodeId}),now),
    ...(attachmentBinding?.steps??[]),
    organizationChangeNotice(db,actor.companyId,id,key,state),
  );
  return executeCommand(db,actor,key,operation,input,id,version,{id,version,state,nodeId:appliedNodeId},statements);
}
export async function cancelOrganizationNodeChange(db:SqlDatabase,actor:WorkContext,id:string,expectedVersion:number,key:string){
  actor=requireCompanyOrganizationAuthority(actor);const operation="organization.node-change.cancel",input={expectedVersion};
  const prior=await replayCommand(db,actor,key,operation,id,input);if(prior)return prior;
  const current=await db.prepare("SELECT requested_by,state FROM organization_node_changes WHERE id=? AND company_id=?").bind(id,actor.companyId).first<{requested_by:string;state:string}>();
  if(!current)throw new ApiError(404,"ORGANIZATION_CHANGE_NOT_FOUND","Organization change not found.");
  if(current.requested_by!==actor.userId)throw new ApiError(403,"CHANGE_OWNER_REQUIRED","Only the requester can cancel this organization change.");
  if(!['draft','submitted','rejected'].includes(current.state))throw new ApiError(409,"CHANGE_LOCKED","This organization change can no longer be cancelled.");
  const version=expectedVersion+1,now=Date.now();return executeCommand(db,actor,key,operation,input,id,version,{id,version,state:"cancelled"},[
    assertion(db,key,"change-version","EXISTS(SELECT 1 FROM organization_node_changes WHERE id=? AND company_id=? AND version=? AND state IN ('draft','submitted','rejected'))",[id,actor.companyId,expectedVersion]),
    db.prepare("UPDATE organization_node_changes SET state='cancelled',version=?,updated_at=? WHERE id=? AND company_id=?").bind(version,now,id,actor.companyId),
    db.prepare("INSERT INTO organization_node_change_events(id,change_id,company_id,action,actor_id,version,snapshot_json,created_at) VALUES(?,?,?,?,?,?,?,?)").bind(`org-change-event-${key}`,id,actor.companyId,"cancelled",actor.userId,version,JSON.stringify(input),now),
    organizationChangeNotice(db,actor.companyId,id,key,"cancelled"),
  ]);
}
function checkScope(actor: WorkContext, branchId:string|null, departmentId:string|null) {
  if ((actor.branchId && actor.branchId!==branchId) || (actor.departmentId && actor.departmentId!==departmentId)) throw new ApiError(403,"SCOPE_FORBIDDEN","Choose a position in your authorized workspace.");
}
async function checkPlacement(db:SqlDatabase,actor:WorkContext,branchId:string|null,departmentId:string|null) {
  checkScope(actor,branchId,departmentId);
  if (branchId && !await db.prepare("SELECT 1 FROM organization_nodes WHERE company_id=? AND id=? AND kind='branch' AND status='active'").bind(actor.companyId,branchId).first()) throw new ApiError(422,"BRANCH_INVALID","Choose an active branch in this company.");
  if (departmentId && !await db.prepare("SELECT 1 FROM organization_nodes WHERE company_id=? AND id=? AND kind='department' AND status='active' AND branch_id IS NOT DISTINCT FROM ?").bind(actor.companyId,departmentId,branchId).first()) throw new ApiError(422,"DEPARTMENT_INVALID","Choose a department in the selected branch.");
}
export async function installStaffLevels(db:SqlDatabase,actor:WorkContext,key:string) {
  actor=requirePermission(actor,"organization.manage");
  if(actor.assignmentScope!=="holding") throw new ApiError(403,"COMPANY_AUTHORITY_REQUIRED","Company HR manages the level catalog.");
  return executeCommand(db,actor,key,"organization.levels.install",{},actor.companyId,1,{id:actor.companyId},staffLevelTitles.map((name,index)=>db.prepare("INSERT INTO staff_levels(id,company_id,code,name,rank,level_number,active,created_at,updated_at) VALUES(?,?,?,?,?,?,1,?,?) ON CONFLICT DO NOTHING").bind(`org-level-${actor.companyId}-${index+1}`,actor.companyId,`ORG_L${index+1}`,name,-index-1,index+1,Date.now(),Date.now())));
}
export async function savePosition(db:SqlDatabase,actor:WorkContext,id:string|null,input:z.infer<typeof positionSchema>,key:string) {
  actor=requirePermission(actor,"organization.manage");
  const recordId=id??`position-${key}`,operation="organization.position.save";
  const prior=await replayCommand(db,actor,key,operation,recordId,input);if(prior)return prior;
  const guard=await organizationGuard(db,actor.companyId,key);
  const roster=await organizationRoster(db,actor.companyId),current=roster.positions.find(p=>p.id===id);
  if(id&&!current) throw new ApiError(404,"POSITION_NOT_FOUND","Position not found.");
  if(current)checkScope(actor,current.branch_id,current.department_id);
  await checkPlacement(db,actor,input.branchId,input.departmentId);
  const level=await db.prepare("SELECT level_number FROM staff_levels WHERE id=? AND company_id=? AND active=1 AND level_number IS NOT NULL").bind(input.levelId,actor.companyId).first<{level_number:number}>();
  if(!level)throw new ApiError(422,"LEVEL_REQUIRED","Choose one of the six active staff levels.");
  if(input.isCeo && (level.level_number!==1 || input.reportsToPositionId))throw new ApiError(422,"CEO_POSITION_INVALID","The CEO position uses Level 1 and has no reporting position.");
  if(!input.isCeo && !input.reportsToPositionId)throw new ApiError(422,"REPORTING_POSITION_REQUIRED","Choose the expected reporting position.");
  const seen=new Set([recordId]);let parent=input.reportsToPositionId;
  while(parent){if(seen.has(parent))throw new ApiError(422,"REPORTING_CYCLE","A position cannot report to itself or its descendants.");seen.add(parent);const p=roster.positions.find(v=>v.id===parent&&v.active===1);if(!p)throw new ApiError(422,"REPORTING_POSITION_INVALID","Choose an active reporting position in this company.");if(parent===input.reportsToPositionId&&p.level_number>=level.level_number)throw new ApiError(422,"MANAGER_LEVEL_INVALID","The reporting position must be more senior.");parent=p.reports_to_position_id;}
  if(id&&!input.active && (roster.people.some(p=>p.position_id===id&&p.status==="active")||roster.positions.some(p=>p.reports_to_position_id===id&&p.active===1)))throw new ApiError(409,"POSITION_IN_USE","Reassign active staff and reporting positions before archiving this position.");
  if(id&&roster.positions.some(p=>p.active===1&&p.reports_to_position_id===id&&p.level_number<=level.level_number))throw new ApiError(422,"POSITION_LEVEL_INVALID","Reporting positions must remain less senior than this position.");
  if(current){
    const candidate={...current,branch_id:input.branchId,department_id:input.departmentId,level_id:input.levelId,level_number:level.level_number,is_ceo:input.isCeo?1:0,active:input.active?1:0};
    const positions=roster.positions.map(p=>p.id===id?candidate:p);
    for(const person of roster.people.filter(p=>p.position_id===id&&p.status==="active")){
      const priorIssues=reportingIssues(person,roster.people,roster.positions);
      const added=reportingIssues(person,roster.people,positions).filter(i=>!priorIssues.includes(i));
      if(added.length)throw new ApiError(422,"POSITION_IN_USE",added.join(" "));
    }
  }
  const now=Date.now(),version=input.expectedVersion+1;
  const rosterRequired=input.rosterRequired===undefined?(current?.roster_required===1):input.rosterRequired;
  const exiting=current?.roster_required===1&&!rosterRequired?(await db.prepare("SELECT id FROM hr_engagements WHERE company_id=? AND position_id=? AND status='active'").bind(actor.companyId,recordId).all<{id:string}>()).results.map(row=>row.id):[];
  return executeCommand(db,actor,key,operation,input,recordId,version,{id:recordId,version},[guard,
    ...(id?[assertion(db,key,"position-version","EXISTS(SELECT 1 FROM organization_positions WHERE id=? AND company_id=? AND version=?)",[id,actor.companyId,input.expectedVersion])]:[]),
    db.prepare(`INSERT INTO organization_positions(id,company_id,branch_id,department_id,title,level_id,reports_to_position_id,is_ceo,roster_required,active,version,created_by,created_at,updated_at) VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?,?)
      ON CONFLICT(id) DO UPDATE SET branch_id=excluded.branch_id,department_id=excluded.department_id,title=excluded.title,level_id=excluded.level_id,reports_to_position_id=excluded.reports_to_position_id,is_ceo=excluded.is_ceo,roster_required=excluded.roster_required,active=excluded.active,version=excluded.version,updated_at=excluded.updated_at`).bind(recordId,actor.companyId,input.branchId,input.departmentId,input.title,input.levelId,input.reportsToPositionId,input.isCeo?1:0,rosterRequired?1:0,input.active?1:0,version,actor.userId,now,now),
    db.prepare("UPDATE organization_positions SET requirements=? WHERE id=? AND company_id=?").bind(input.requirements,recordId,actor.companyId),
    ...rosterPositionExitStatements(db,actor,exiting,key,input.reason),
  ]);
}
function validateTemplate(positions:OrganizationTemplatePosition[]) {
  if(new Set(positions.map(p=>p.key)).size!==positions.length)throw new ApiError(422,"DUPLICATE_POSITION","Template position keys must be unique.");
  for(const p of positions){if(p.isCeo&&(p.levelNumber!==1||p.reportsToKey))throw new ApiError(422,"CEO_POSITION_INVALID","The CEO uses Level 1 with no reporting position.");if(!p.isCeo&&!p.reportsToKey)throw new ApiError(422,"REPORTING_POSITION_REQUIRED","Each non-CEO position needs a reporting position.");const seen=new Set([p.key]);let parent=p.reportsToKey;while(parent){if(seen.has(parent))throw new ApiError(422,"REPORTING_CYCLE","Correct the template reporting cycle.");seen.add(parent);const next=positions.find(v=>v.key===parent);if(!next||next.levelNumber>=p.levelNumber)throw new ApiError(422,"REPORTING_POSITION_INVALID","Template managers must exist and be more senior.");parent=next.reportsToKey;}}
  if(positions.filter(p=>p.isCeo).length!==1)throw new ApiError(422,"CEO_POSITION_REQUIRED","The organization template needs one CEO position.");
}
export async function saveOrganizationTemplate(db:SqlDatabase,actor:WorkContext,id:string|null,input:z.infer<typeof organizationTemplateSchema>,key:string) {
  actor=requirePermission(actor,"organization.manage");validateTemplate(input.positions);
  const recordId=id??`org-template-${key}`,version=input.expectedVersion+1;
  return executeCommand(db,actor,key,"organization.template.save",input,recordId,version,{id:recordId,version},[
    ...(id?[assertion(db,key,"template-version","EXISTS(SELECT 1 FROM organization_templates WHERE id=? AND company_id=? AND version=?)",[id,actor.companyId,input.expectedVersion])]:[]),
    db.prepare("INSERT INTO organization_templates(id,company_id,name,positions_json,version,created_by,created_at,updated_at) VALUES(?,?,?,?,?,?,?,?) ON CONFLICT(id) DO UPDATE SET name=excluded.name,positions_json=excluded.positions_json,version=excluded.version,updated_at=excluded.updated_at").bind(recordId,actor.companyId,input.name,JSON.stringify(input.positions),version,actor.userId,Date.now(),Date.now()),
  ]);
}
export async function applyOrganizationTemplate(db:SqlDatabase,actor:WorkContext,id:string,input:z.infer<typeof applyOrganizationTemplateSchema>,key:string) {
  actor=requirePermission(actor,"organization.manage");
  const prior=await replayCommand(db,actor,key,"organization.template.apply",id,input);if(prior)return prior;
  const guard=await organizationGuard(db,actor.companyId,key);
  await checkPlacement(db,actor,input.branchId,input.departmentId);
  const template=await db.prepare("SELECT positions_json,version FROM organization_templates WHERE id=? AND company_id=?").bind(id,actor.companyId).first<{positions_json:string;version:number}>();
  if(!template)throw new ApiError(404,"TEMPLATE_NOT_FOUND","Organization template not found.");
  const positions=z.array(templatePositionSchema).parse(JSON.parse(template.positions_json));validateTemplate(positions);
  const existingCeo=await db.prepare("SELECT id FROM organization_positions WHERE company_id=? AND is_ceo=1 AND active=1").bind(actor.companyId).first<{id:string}>();
  const levels=(await db.prepare("SELECT id,level_number FROM staff_levels WHERE company_id=? AND active=1 AND level_number IS NOT NULL").bind(actor.companyId).all<{id:string;level_number:number}>()).results;
  const ids=new Map(positions.map((p,i)=>[p.key,p.isCeo&&existingCeo?existingCeo.id:`position-${key}-${i}`]));
  const statements=[guard,assertion(db,key,"template-version","EXISTS(SELECT 1 FROM organization_templates WHERE id=? AND company_id=? AND version=?)",[id,actor.companyId,input.expectedVersion])];
  for(const p of positions){if(p.isCeo&&existingCeo)continue;const level=levels.find(l=>l.level_number===p.levelNumber);if(!level)throw new ApiError(422,"LEVEL_REQUIRED","Install the six staff levels before applying this template.");statements.push(db.prepare("INSERT INTO organization_positions(id,company_id,branch_id,department_id,title,level_id,reports_to_position_id,is_ceo,roster_required,created_by,created_at,updated_at) VALUES(?,?,?,?,?,?,?,?,?,?,?,?)").bind(ids.get(p.key),actor.companyId,input.branchId,input.departmentId,p.title,level.id,p.reportsToKey?ids.get(p.reportsToKey):null,p.isCeo?1:0,p.rosterRequired?1:0,actor.userId,Date.now(),Date.now()));}
  return executeCommand(db,actor,key,"organization.template.apply",input,id,input.expectedVersion,{id,positionIds:[...ids.values()]},statements);
}

export async function validateStaffOrganization(db:SqlDatabase,companyId:string,person:Pick<OrganizationPerson,"person_id"|"branch_id"|"department_id"|"position_id"|"level_id"|"manager_person_id">,required:boolean) {
  const roster=await organizationRoster(db,companyId);
  const level=person.level_id?await db.prepare("SELECT level_number FROM staff_levels WHERE company_id=? AND id=? AND active=1").bind(companyId,person.level_id).first<{level_number:number|null}>():null;
  const current=roster.people.find(p=>p.person_id===person.person_id);
  const candidate={...current,...person,company_id:companyId,level_number:level?.level_number??null} as OrganizationPerson;
  const people=[...roster.people.filter(p=>p.person_id!==person.person_id),candidate];
  const issues=reportingIssues(candidate,people,roster.positions);
  for(const report of people.filter(p=>p.person_id!==person.person_id&&p.manager_person_id===person.person_id&&p.status==="active"&&p.level_number)){
    if(candidate.level_number && report.level_number!<=candidate.level_number)throw new ApiError(422,"MANAGER_LEVEL_INVALID","Reassign direct reports before changing their manager to an equal or less senior level.");
  }
  if(required&&issues.length)throw new ApiError(422,"STAFF_ORGANIZATION_INCOMPLETE",issues.join(" "));
  if(!required){
    if(person.position_id&&!roster.positions.some(p=>p.id===person.position_id&&p.active===1&&(!p.branch_id||p.branch_id===person.branch_id)&&(!p.department_id||p.department_id===person.department_id)))throw new ApiError(422,"POSITION_INVALID","Choose an active position matching the staff placement.");
    if(person.manager_person_id){
      if(issues.some(i=>i.includes("circular")))throw new ApiError(422,"REPORTING_CYCLE","Correct the circular reporting relationship.");
      const managers=people.filter(p=>p.person_id===person.manager_person_id&&p.status==="active"&&p.person_status==="active");
      if(managers.length!==1)throw new ApiError(422,"MANAGER_INVALID","Choose one active manager in this company.");
      if(candidate.level_number&&managers[0].level_number&&managers[0].level_number>=candidate.level_number)throw new ApiError(422,"MANAGER_LEVEL_INVALID","The reporting manager must be more senior.");
    }
  }
  return issues;
}
