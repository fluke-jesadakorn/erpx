import { requirePermission } from "@/server/iam-permissions";
import { hasPermission } from "@/lib/permissions";
import { organizationMemberSql } from "./organization-channels";
import { spendingTemplate } from "./spending-templates";
import { z } from "zod";
import type { WorkContext } from "./authorization";
import type { SqlDatabase } from "./runtime/types";
import { ApiError } from "./http";
import { assertion, executeCommand, replayCommand } from "./commands";
import { reportingIssues, staffLevelTitles, type OrganizationOverview, type OrganizationPerson, type OrganizationPosition, type OrganizationTemplatePosition, type OrganizationNode } from "@/lib/organization";

export const positionSchema = z.object({
  title: z.string().trim().min(2).max(160), branchId: z.string().nullable(), departmentId: z.string().nullable(),
  levelId: z.string().min(1), reportsToPositionId: z.string().nullable(), isCeo: z.boolean().default(false),
  active: z.boolean().default(true), expectedVersion: z.number().int().min(0), reason: z.string().trim().min(3).max(1000),
});
const templatePositionSchema = z.object({key:z.string().min(1).max(100),title:z.string().trim().min(2).max(160),levelNumber:z.number().int().min(1).max(6),reportsToKey:z.string().nullable(),isCeo:z.boolean()});
export const organizationTemplateSchema = z.object({name:z.string().trim().min(2).max(120),positions:z.array(templatePositionSchema).min(1).max(100),expectedVersion:z.number().int().min(0)});
export const applyOrganizationTemplateSchema = z.object({expectedVersion:z.number().int().positive(),branchId:z.string().nullable(),departmentId:z.string().nullable(),reason:z.string().trim().min(3).max(1000)});

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
export async function organizationOverview(db: SqlDatabase, actor: WorkContext): Promise<OrganizationOverview> {
  const {people,positions} = await organizationRoster(db,actor.companyId);
  const visible = (p:{branch_id:string|null;department_id:string|null}) => (!actor.branchId || p.branch_id === actor.branchId) && (!actor.departmentId || p.department_id === actor.departmentId);
  const canManage = hasPermission(actor,"organization.manage");
  const nodes = (await db.prepare("SELECT id,kind,display_name,branch_id FROM organization_nodes WHERE company_id=? AND status='active' AND kind IN ('company','branch','department') AND (?::text IS NULL OR id=? OR branch_id=?) AND (?::text IS NULL OR kind!='department' OR id=?) ORDER BY display_name").bind(actor.companyId,actor.branchId,actor.branchId,actor.branchId,actor.departmentId,actor.departmentId).all<OrganizationNode>()).results;
  const levels = (await db.prepare("SELECT id,name,level_number FROM staff_levels WHERE company_id=? AND active=1 AND level_number IS NOT NULL ORDER BY level_number").bind(actor.companyId).all<{id:string;name:string;level_number:number}>()).results;
  const templates = canManage ? (await db.prepare("SELECT id,name,version,positions_json FROM organization_templates WHERE company_id=? ORDER BY name").bind(actor.companyId).all<{id:string;name:string;version:number;positions_json:string}>()).results.map(t=>({id:t.id,name:t.name,version:t.version,positions:JSON.parse(t.positions_json) as OrganizationTemplatePosition[]})) : [];
  const authorizedChannelNodes=(await db.prepare(`SELECT n.id FROM organization_nodes n WHERE n.company_id=? AND n.status='active' AND n.kind IN ('company','branch','department') AND ${organizationMemberSql("n","?")}`).bind(actor.companyId,actor.userId).all<{id:string}>()).results.map(n=>n.id);
  const managerGrants = canManage ? (await db.prepare("SELECT user_id,scope_kind,scope_node_id FROM assignments WHERE company_id=? AND iam_role_allows(role,company_id,'spending.approve') AND revoked_at IS NULL AND iam_assignment_active(id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) AND iam_principal_active(user_id,company_id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) AND valid_from<=? AND valid_until>?").bind(actor.companyId,Date.now(),Date.now()).all<{user_id:string;scope_kind:string;scope_node_id:string|null}>()).results : [];
  const managerIssues = (person:OrganizationPerson) => canManage && people.filter(p=>p.manager_person_id===person.person_id && p.status==="active" && visible(p)).some(report=>!managerGrants.some(a=>a.user_id===person.user_id && (a.scope_kind==="holding" || a.scope_kind==="branch" && a.scope_node_id===report.branch_id || a.scope_kind==="department" && a.scope_node_id===report.department_id))) ? ["People Manager access is missing for one or more reporting teams. Open the profile and review scoped role assignments."] : [];
  return {authorizedChannelNodes,spendingTemplate:canManage?await spendingTemplate(db,actor.companyId):undefined,canManageSpending:canManage&&actor.assignmentScope==="holding",positions:positions.filter(p=>(!actor.branchId||!p.branch_id||p.branch_id===actor.branchId)&&(!actor.departmentId||!p.department_id||p.department_id===actor.departmentId)),people:people.filter(visible).map(p=>({...p,issues:[...reportingIssues(p,people,positions),...managerIssues(p)]})),nodes,levels,templates,canManage};
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
  return executeCommand(db,actor,key,operation,input,recordId,version,{id:recordId,version},[guard,
    ...(id?[assertion(db,key,"position-version","EXISTS(SELECT 1 FROM organization_positions WHERE id=? AND company_id=? AND version=?)",[id,actor.companyId,input.expectedVersion])]:[]),
    db.prepare(`INSERT INTO organization_positions(id,company_id,branch_id,department_id,title,level_id,reports_to_position_id,is_ceo,active,version,created_by,created_at,updated_at) VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?)
      ON CONFLICT(id) DO UPDATE SET branch_id=excluded.branch_id,department_id=excluded.department_id,title=excluded.title,level_id=excluded.level_id,reports_to_position_id=excluded.reports_to_position_id,is_ceo=excluded.is_ceo,active=excluded.active,version=excluded.version,updated_at=excluded.updated_at`).bind(recordId,actor.companyId,input.branchId,input.departmentId,input.title,input.levelId,input.reportsToPositionId,input.isCeo?1:0,input.active?1:0,version,actor.userId,now,now),
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
  for(const p of positions){if(p.isCeo&&existingCeo)continue;const level=levels.find(l=>l.level_number===p.levelNumber);if(!level)throw new ApiError(422,"LEVEL_REQUIRED","Install the six staff levels before applying this template.");statements.push(db.prepare("INSERT INTO organization_positions(id,company_id,branch_id,department_id,title,level_id,reports_to_position_id,is_ceo,created_by,created_at,updated_at) VALUES(?,?,?,?,?,?,?,?,?,?,?)").bind(ids.get(p.key),actor.companyId,input.branchId,input.departmentId,p.title,level.id,p.reportsToKey?ids.get(p.reportsToKey):null,p.isCeo?1:0,actor.userId,Date.now(),Date.now()));}
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
