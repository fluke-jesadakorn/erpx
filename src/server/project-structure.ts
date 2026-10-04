import 'server-only';
import {z} from 'zod';
import type {WorkContext} from './authorization';
import type {SqlDatabase,SqlStatement} from './runtime/types';
import {isTransactionalSqlDatabase} from './runtime/types';
import {holdingResources} from './runtime';
import {requirePermission} from './iam-permissions';
import {hasPermission} from '@/lib/permissions';
import {hasModuleAccess} from '@/data/module-access';
import {samePerson} from '@/lib/iam';
import {ApiError} from './http';
import {assertion,executeCommand,replayCommand} from './commands';
import {activeStaff,staffGuard} from './staff-policy';
import {getProject,type ProjectRow} from './staff-tasks';
import {listProjectMilestones,projectConstraint,type ProjectMilestoneRow} from './project-milestones';
import type {WorkItem} from '@/lib/work-item';

type Project=ProjectRow&{lifecycle_status:'active'|'handoff_pending'|'completed'|'cancelled'};
const version=z.number().int().positive(),id=z.string().min(1).max(200),reason=z.string().trim().min(3).max(2000);
export const projectStructureCommandSchema=z.discriminatedUnion('action',[
 z.object({action:z.literal('assign-task'),expectedVersion:version,taskId:id,taskVersion:version,milestoneId:id,milestoneVersion:version,bindingVersion:z.number().int().nonnegative().default(0),reason}),
 z.object({action:z.literal('unassign-task'),expectedVersion:version,taskId:id,taskVersion:version,bindingVersion:version,reason}),
 z.object({action:z.literal('link-actual'),expectedVersion:version,milestoneId:id,milestoneVersion:version,sourceKind:z.enum(['work_log','cost_allocation']),sourceId:id,sourceVersion:version,reason}),
 z.object({action:z.literal('prepare-handoff'),expectedVersion:version,recipientId:id,evidence:z.string().trim().min(3).max(10000),reason}),
 z.object({action:z.enum(['accept-handoff','reject-handoff','cancel-handoff','reopen']),expectedVersion:version,handoffId:id,handoffVersion:version,reason}),
]);
type GraphTask={id:string;title:string;state:string;version:number;assignee_user_id:string;milestone_id:string|null;binding_version:number|null};
type Actual={id:string;milestone_id:string;source_kind:'work_log'|'cost_allocation';source_id:string;source_version:number;minutes:number|null;amount_minor:number|null;currency:string|null;user_id:string|null;current_source_version:number;source_state:string;journal_status:string|null;journal_id:string|null};
type GraphPackage=ProjectMilestoneRow&{wbs:string;depth:number;tasks:GraphTask[];approvedMinutes:number;costByCurrency:Record<string,string>};
export type ProjectGraph={project:Project;packages:GraphPackage[];tasks:GraphTask[];actuals:Actual[];edges:{from:string;to:string;kind:'contains'|'requires'|'task'}[];unassignedTasks:GraphTask[];canReadCosts:boolean;isProjectOwner:boolean;totals:{approvedMinutes:number;costByCurrency:Record<string,string>}};
type HandoffRow={id:string;company_id:string;project_id:string;source_project_version:number;recipient_id:string;status:'pending'|'accepted'|'rejected'|'cancelled';version:number;snapshot_json:string;snapshot_sha256:string;evidence:string;prepared_by:string;prepared_at:number;decided_by:string|null;decided_at:number|null;decision_reason:string|null};
export type ProjectHandoffView=Omit<HandoffRow,'snapshot_json'>&{snapshot:ProjectGraph};
function access(a:WorkContext,write=false){requirePermission(a,write?'records.write':'records.read');if(!hasModuleAccess(a.modules,'projects'))throw new ApiError(403,'MODULE_FORBIDDEN','Your current role cannot access Projects.');}
function costAccess(a:WorkContext,p:Project){return hasPermission(a,'finance.read')&&(!a.branchId||a.branchId===p.branch_id)&&(!a.departmentId||a.departmentId===p.department_id);}
async function project(db:SqlDatabase,a:WorkContext,projectId:string){
 access(a);
 const current=await db.prepare("SELECT 1 FROM assignments WHERE id=? AND company_id=? AND user_id=? AND role=? AND iam_assignment_active(id,?) AND iam_principal_active(user_id,company_id,?) AND iam_role_allows(role,company_id,'records.read')").bind(a.assignmentId,a.companyId,a.userId,a.role,Date.now(),Date.now()).first();
 if(!current)throw new ApiError(404,'PROJECT_NOT_FOUND','Project not found in the current role.');
 await activeStaff(db,a.companyId,a.userId);return await getProject(db,a,projectId) as Project;
}
function owner(a:WorkContext,p:Project){if(!samePerson(a,p.owner_user_id))throw new ApiError(403,'PROJECT_OWNER_REQUIRED','The project owner manages work structure and handoff.');}
function sourceCurrent(actual:Actual){return actual.source_version===actual.current_source_version&&actual.source_state==='approved'&&(actual.source_kind==='work_log'||actual.journal_status==='posted');}
function addCost(result:Record<string,string>,currency:string,minor:number){result[currency]=(BigInt(result[currency]??'0')+BigInt(minor)).toString();}
async function graphRows(db:SqlDatabase,a:WorkContext,projectId:string){
 const p=await project(db,a,projectId),milestones=await listProjectMilestones(db,a,projectId),tasks=(await db.prepare(`SELECT t.id,t.title,t.state,t.version,t.assignee_user_id,b.milestone_id,b.version binding_version
  FROM staff_tasks t LEFT JOIN staff_project_task_packages b ON b.task_id=t.id WHERE t.project_id=? AND t.company_id=? ORDER BY t.id LIMIT 1001`).bind(projectId,a.companyId).all<GraphTask>()).results;
 if(milestones.length>1000||tasks.length>1000)throw new ApiError(422,'PROJECT_GRAPH_TOO_LARGE','Divide this project into bounded work packages before creating a complete handoff snapshot.');
 const actuals=(await db.prepare(`SELECT a.*,l.user_id,l.version current_source_version,l.state source_state,NULL::text journal_status,NULL::text journal_id
  FROM staff_project_package_actuals a JOIN staff_project_work_logs l ON a.source_kind='work_log' AND l.id=a.source_id WHERE a.project_id=? AND a.company_id=?
  UNION ALL SELECT a.*,NULL,c.version,c.state,j.status,j.id FROM staff_project_package_actuals a JOIN staff_project_cost_allocations c ON a.source_kind='cost_allocation' AND c.id=a.source_id
  JOIN finance_journal_lines line ON line.id=c.journal_line_id JOIN finance_journals j ON j.id=line.journal_id WHERE a.project_id=? AND a.company_id=? ORDER BY id`).bind(projectId,a.companyId,projectId,a.companyId).all<Actual>()).results;
 return {project:p,milestones,tasks,actuals};
}
function buildGraph(rows:Awaited<ReturnType<typeof graphRows>>,a:WorkContext,includeAll=false):ProjectGraph{
 const {project:p,milestones,tasks}=rows,isOwner=samePerson(a,p.owner_user_id),canReadCosts=includeAll||costAccess(a,p),actuals=rows.actuals.filter(source=>includeAll||(source.source_kind==='cost_allocation'?canReadCosts:isOwner||samePerson(a,source.user_id))),
  packages:GraphPackage[]=[],seen=new Set<string>(),edges:ProjectGraph['edges']=[],totalCosts:Record<string,string>={};let totalMinutes=0;
 const walk=(parent:string|null,prefix:string,depth:number)=>{
  const children=milestones.filter(m=>(m.parent_milestone_id??null)===parent).sort((left,right)=>(left.ordinal_position??0)-(right.ordinal_position??0)||left.id.localeCompare(right.id));
  for(const [index,m] of children.entries()){
   if(seen.has(m.id))throw new ApiError(409,'PROJECT_PACKAGE_CYCLE','The project structure requires recovery before it can be read.');seen.add(m.id);
   const wbs=prefix?`${prefix}.${index+1}`:String(index+1),costs:Record<string,string>={};let minutes=0;
   for(const source of actuals.filter(s=>s.milestone_id===m.id&&sourceCurrent(s))){if(source.source_kind==='work_log')minutes+=Number(source.minutes);else addCost(costs,source.currency!,Number(source.amount_minor));}
   totalMinutes+=minutes;for(const [currency,amount] of Object.entries(costs))totalCosts[currency]=(BigInt(totalCosts[currency]??'0')+BigInt(amount)).toString();
   packages.push({...m,wbs,depth,tasks:tasks.filter(task=>task.milestone_id===m.id),approvedMinutes:minutes,costByCurrency:costs});
   if(parent)edges.push({from:parent,to:m.id,kind:'contains'});for(const predecessor of m.depends_on_ids)edges.push({from:predecessor,to:m.id,kind:'requires'});
   for(const task of tasks.filter(t=>t.milestone_id===m.id))edges.push({from:m.id,to:task.id,kind:'task'});
   walk(m.id,wbs,depth+1);
  }
 };
 walk(null,'',0);if(seen.size!==milestones.length)throw new ApiError(409,'PROJECT_PACKAGE_CYCLE','The project structure requires recovery before it can be read.');
 return {project:p,packages,tasks,actuals,edges,unassignedTasks:tasks.filter(t=>!t.milestone_id),canReadCosts,isProjectOwner:isOwner,totals:{approvedMinutes:totalMinutes,costByCurrency:totalCosts}};
}
export async function projectGraph(a:WorkContext,projectId:string,providedWrite?:SqlDatabase){return buildGraph(await graphRows(providedWrite??(await holdingResources(a.holdingId)).write,a,projectId),a);}
function event(db:SqlDatabase,key:string,p:Project,packageId:string|null,action:string,a:WorkContext,note:string,snapshot:unknown,now:number){return db.prepare('INSERT INTO staff_project_package_events VALUES(?,?,?,?,?,?,?,?,?)').bind(`project-package-event-${key}`,p.company_id,p.id,packageId,action,a.userId,note,JSON.stringify(snapshot),now);}
async function sha256(value:string){return Array.from(new Uint8Array(await crypto.subtle.digest('SHA-256',new TextEncoder().encode(value))),b=>b.toString(16).padStart(2,'0')).join('');}
const projectMember=(alias:string)=>`(iam_same_person(${alias}.owner_user_id,?,${alias}.company_id) OR EXISTS(SELECT 1 FROM staff_project_members member WHERE member.project_id=${alias}.id AND iam_same_person(member.user_id,?,${alias}.company_id)))`;

export async function projectStructureCommand(a:WorkContext,projectId:string,raw:unknown,key:string,providedWrite?:SqlDatabase):Promise<Record<string,unknown>>{
 access(a,true);const input=projectStructureCommandSchema.parse(raw),db=providedWrite??(await holdingResources(a.holdingId)).write;
 if(!providedWrite){if(!isTransactionalSqlDatabase(db))throw new ApiError(503,'PROJECT_TRANSACTION_REQUIRED','The transaction runtime is unavailable. Retry when it is ready.');return db.transaction(tx=>projectStructureCommand(a,projectId,input,key,tx)).catch(projectConstraint);}
 await project(db,a,projectId);
 const op=`project.structure.${input.action}`,prior=await replayCommand(db,a,key,op,projectId,input);if(prior)return prior;
 await db.prepare('SELECT id FROM staff_projects WHERE id=? AND company_id=? FOR UPDATE').bind(projectId,a.companyId).run();
 const p=await project(db,a,projectId),now=Date.now(),nextVersion=input.expectedVersion+1;
 if(p.version!==input.expectedVersion)throw new ApiError(409,'CONFLICT','The project changed. Reload its current structure before continuing.');
 const steps:SqlStatement[]=[staffGuard(db,key,a.companyId,a.userId),assertion(db,key,'project-member',`EXISTS(SELECT 1 FROM staff_projects p WHERE p.id=? AND p.company_id=? AND p.version=? AND ${projectMember('p')})`,[p.id,a.companyId,input.expectedVersion,a.userId,a.userId])];
 let lifecycle=p.lifecycle_status,packageId:string|null=null,result:Record<string,unknown>={id:projectId,version:nextVersion};
 if(input.action==='assign-task'||input.action==='unassign-task'){
  owner(a,p);if(p.lifecycle_status!=='active')throw new ApiError(409,'PROJECT_STRUCTURE_FROZEN','Reopen the project before changing work structure.');
  const task=await db.prepare('SELECT id,version,project_id FROM staff_tasks WHERE id=? AND company_id=? AND project_id=? FOR UPDATE').bind(input.taskId,a.companyId,p.id).first<{id:string;version:number;project_id:string}>(),binding=await db.prepare('SELECT * FROM staff_project_task_packages WHERE task_id=?').bind(input.taskId).first<{milestone_id:string;version:number}>();
  if(!task||task.version!==input.taskVersion||(binding?.version??0)!==input.bindingVersion)throw new ApiError(409,'PROJECT_TASK_BINDING_CHANGED','Reload the task and its current work package.');
  packageId=input.action==='assign-task'?input.milestoneId:binding?.milestone_id??null;
  if(input.action==='assign-task')steps.push(assertion(db,key,'package-version',"EXISTS(SELECT 1 FROM staff_project_milestones WHERE id=? AND company_id=? AND project_id=? AND version=? AND state IN ('planned','in_progress'))",[input.milestoneId,a.companyId,p.id,input.milestoneVersion]),
   db.prepare('INSERT INTO staff_project_task_packages VALUES(?,?,?,?,?,?,?,?) ON CONFLICT(task_id) DO UPDATE SET milestone_id=excluded.milestone_id,source_task_version=excluded.source_task_version,version=staff_project_task_packages.version+1,linked_by=excluded.linked_by,linked_at=excluded.linked_at').bind(input.taskId,a.companyId,p.id,input.milestoneId,input.taskVersion,1,a.userId,now));
  else steps.push(assertion(db,key,'package-editable',"EXISTS(SELECT 1 FROM staff_project_milestones WHERE id=? AND state IN ('planned','in_progress'))",[packageId]),db.prepare('DELETE FROM staff_project_task_packages WHERE task_id=? AND version=?').bind(input.taskId,input.bindingVersion));
  result={...result,taskId:input.taskId,bindingVersion:input.action==='assign-task'?input.bindingVersion+1:0};
 }else if(input.action==='link-actual'){
  if(p.lifecycle_status!=='active')throw new ApiError(409,'PROJECT_STRUCTURE_FROZEN','Reopen the project before assigning approved actuals.');
  packageId=input.milestoneId;let minutes:number|null=null,amount:number|null=null,currency:string|null=null;
  if(input.sourceKind==='work_log'){
   owner(a,p);const source=await db.prepare("SELECT * FROM staff_project_work_logs WHERE id=? AND company_id=? AND project_id=? AND state='approved' AND version=? FOR SHARE").bind(input.sourceId,a.companyId,p.id,input.sourceVersion).first<{minutes:number}>();if(!source)throw new ApiError(409,'PROJECT_TIME_SOURCE_CHANGED','Choose the current independently approved project time.');minutes=source.minutes;
  }else{
   requirePermission(a,'finance.write');if(!costAccess(a,p))throw new ApiError(404,'PROJECT_NOT_FOUND','Project not found in the current Finance scope.');
   const source=await db.prepare("SELECT c.amount_minor,j.currency FROM staff_project_cost_allocations c JOIN finance_journal_lines l ON l.id=c.journal_line_id JOIN finance_journals j ON j.id=l.journal_id WHERE c.id=? AND c.company_id=? AND c.project_id=? AND c.state='approved' AND c.version=? AND j.status='posted' FOR SHARE OF c,j").bind(input.sourceId,a.companyId,p.id,input.sourceVersion).first<{amount_minor:number;currency:string}>();if(!source)throw new ApiError(409,'PROJECT_COST_SOURCE_CHANGED','Choose the current approved allocation from a posted Finance journal.');amount=source.amount_minor;currency=source.currency;
  }
  steps.push(assertion(db,key,'actual-package-version','EXISTS(SELECT 1 FROM staff_project_milestones WHERE id=? AND company_id=? AND project_id=? AND version=?)',[packageId,a.companyId,p.id,input.milestoneVersion]),
   db.prepare('INSERT INTO staff_project_package_actuals VALUES(?,?,?,?,?,?,?,?,?,?,?,?)').bind(`project-package-actual-${key}`,a.companyId,p.id,packageId,input.sourceKind,input.sourceId,input.sourceVersion,minutes,amount,currency,a.userId,now));
  result={...result,actualId:`project-package-actual-${key}`};
 }else if(input.action==='prepare-handoff'){
  owner(a,p);if(p.lifecycle_status!=='active')throw new ApiError(409,'PROJECT_HANDOFF_NOT_READY','The project must be active before a new handoff.');if(samePerson(a,input.recipientId))throw new ApiError(409,'INDEPENDENT_HANDOFF_REQUIRED','A different project member must receive and review this handoff.');
  await activeStaff(db,a.companyId,input.recipientId);
  const ready=await db.prepare(`SELECT NOT EXISTS(SELECT 1 FROM staff_project_milestones WHERE project_id=? AND state NOT IN ('completed','cancelled'))
   AND NOT EXISTS(SELECT 1 FROM staff_tasks WHERE project_id=? AND state NOT IN ('completed','cancelled'))
   AND NOT EXISTS(SELECT 1 FROM staff_tasks t WHERE t.project_id=? AND t.state<>'cancelled' AND NOT EXISTS(SELECT 1 FROM staff_project_task_packages b WHERE b.task_id=t.id))
   AND NOT EXISTS(SELECT 1 FROM staff_project_work_logs WHERE project_id=? AND state='submitted')
   AND NOT EXISTS(SELECT 1 FROM staff_project_cost_allocations WHERE project_id=? AND state='draft')
   AND NOT EXISTS(SELECT 1 FROM staff_project_work_logs l WHERE l.project_id=? AND l.state='approved' AND NOT EXISTS(SELECT 1 FROM staff_project_package_actuals a WHERE a.source_kind='work_log' AND a.source_id=l.id))
   AND NOT EXISTS(SELECT 1 FROM staff_project_cost_allocations c JOIN finance_journal_lines line ON line.id=c.journal_line_id JOIN finance_journals j ON j.id=line.journal_id
    WHERE c.project_id=? AND c.state='approved' AND j.status='posted' AND NOT EXISTS(SELECT 1 FROM staff_project_package_actuals a WHERE a.source_kind='cost_allocation' AND a.source_id=c.id)) ready`).bind(p.id,p.id,p.id,p.id,p.id,p.id,p.id).first<{ready:boolean}>();
  if(!ready?.ready)throw new ApiError(409,'PROJECT_HANDOFF_INCOMPLETE','Close all work and reviews, and assign completed tasks and approved time/cost sources to work packages before handoff.');
  const graph=buildGraph(await graphRows(db,a,p.id),a,true),snapshot=JSON.stringify(graph),digest=await sha256(snapshot),handoffId=`project-handoff-${key}`;
  steps.push(staffGuard(db,key,a.companyId,input.recipientId,'handoff-recipient'),assertion(db,key,'recipient-member','EXISTS(SELECT 1 FROM staff_project_members WHERE project_id=? AND user_id=?)',[p.id,input.recipientId]),
   db.prepare('INSERT INTO staff_project_handoffs(id,company_id,project_id,source_project_version,recipient_id,snapshot_json,snapshot_sha256,evidence,prepared_by,prepared_at) VALUES(?,?,?,?,?,?,?,?,?,?)').bind(handoffId,a.companyId,p.id,p.version,input.recipientId,snapshot,digest,input.evidence,a.userId,now),
   db.prepare("INSERT INTO staff_notifications(id,company_id,user_id,record_type,record_id,record_version,message,created_at) VALUES(?,?,?,'project_handoff',?,1,'Review the completed project and frozen handoff evidence',?)").bind(`project-handoff-notice-${key}`,a.companyId,input.recipientId,handoffId,now));
  lifecycle='handoff_pending';result={...result,handoffId,snapshotSha256:digest};
 }else{
  const h=await db.prepare('SELECT * FROM staff_project_handoffs WHERE id=? AND company_id=? AND project_id=? FOR UPDATE').bind(input.handoffId,a.companyId,p.id).first<{id:string;status:string;version:number;recipient_id:string;prepared_by:string;snapshot_sha256:string}>();
  if(!h||h.version!==input.handoffVersion)throw new ApiError(409,'PROJECT_HANDOFF_CHANGED','Reload the current handoff before reviewing it.');
  if(input.action==='reopen'){
   if(p.lifecycle_status!=='completed'||h.status!=='accepted'||!samePerson(a,h.recipient_id)||samePerson(a,p.owner_user_id))throw new ApiError(403,'PROJECT_RECIPIENT_REOPEN_REQUIRED','The independent receiving member must review reopening an accepted project.');
   lifecycle='active';
  }else{
   if(h.status!=='pending'||p.lifecycle_status!=='handoff_pending')throw new ApiError(409,'PROJECT_HANDOFF_DECIDED','This handoff already has a recorded decision.');
   if(input.action==='cancel-handoff'){if(!samePerson(a,h.prepared_by))throw new ApiError(403,'PROJECT_HANDOFF_PREPARER_REQUIRED','The preparing owner cancels the pending handoff.');}
   else if(!samePerson(a,h.recipient_id)||samePerson(a,h.prepared_by))throw new ApiError(403,'PROJECT_RECIPIENT_REQUIRED','The named independent receiving member must decide this handoff.');
   const status=input.action==='accept-handoff'?'accepted':input.action==='reject-handoff'?'rejected':'cancelled';
   steps.push(db.prepare('UPDATE staff_project_handoffs SET status=?,version=version+1,decided_by=?,decided_at=?,decision_reason=? WHERE id=?').bind(status,a.userId,now,input.reason,h.id));
   if(input.action!=='cancel-handoff')steps.push(db.prepare("INSERT INTO staff_notifications(id,company_id,user_id,record_type,record_id,record_version,message,created_at) VALUES(?,?,?,'project_handoff',?,?,?,?)").bind(`project-handoff-decision-${key}`,a.companyId,h.prepared_by,h.id,h.version+1,`Project handoff ${status}; retained review evidence is available`,now));
   lifecycle=status==='accepted'?'completed':'active';
  }
  result={...result,handoffId:h.id,snapshotSha256:h.snapshot_sha256};
 }
 steps.push(db.prepare('UPDATE staff_projects SET lifecycle_status=?,version=version+1,updated_at=? WHERE id=?').bind(lifecycle,now,p.id),event(db,key,p,packageId,input.action,a,input.reason,{...input,...result,status:lifecycle},now));
 return executeCommand(db,a,key,op,input,p.id,nextVersion,{...result,status:lifecycle},steps,{companyId:p.company_id,branchId:p.branch_id,departmentId:p.department_id});
}

export async function projectHandoffs(a:WorkContext,projectId:string):Promise<ProjectHandoffView[]>{const db=(await holdingResources(a.holdingId)).write,p=await project(db,a,projectId),isOwner=samePerson(a,p.owner_user_id),canCosts=costAccess(a,p),rows=(await db.prepare('SELECT * FROM staff_project_handoffs WHERE company_id=? AND project_id=? ORDER BY prepared_at DESC,id').bind(a.companyId,p.id).all<HandoffRow>()).results;
 return rows.map(row=>{const graph=JSON.parse(String(row.snapshot_json)) as ProjectGraph;
  const visible=graph.actuals.filter(source=>source.source_kind==='cost_allocation'?canCosts:isOwner||samePerson(a,source.user_id));
  const packages=graph.packages.map(packageRow=>{const costs:Record<string,string>={};let minutes=0;for(const source of visible.filter(s=>s.milestone_id===packageRow.id&&sourceCurrent(s))){if(source.source_kind==='work_log')minutes+=Number(source.minutes);else addCost(costs,source.currency!,Number(source.amount_minor));}return {...packageRow,approvedMinutes:minutes,costByCurrency:costs};});
  const costs:Record<string,string>={};for(const source of visible.filter(source=>source.source_kind==='cost_allocation'&&sourceCurrent(source)))addCost(costs,source.currency!,Number(source.amount_minor));
  return {...row,snapshot_json:undefined,snapshot:{...graph,actuals:visible,packages,canReadCosts:canCosts,isProjectOwner:isOwner,totals:{approvedMinutes:packages.reduce((n,pack)=>n+pack.approvedMinutes,0),costByCurrency:costs}}};
 });
}

export async function projectHandoffTarget(db:SqlDatabase,a:WorkContext,id:string){
 const source=await db.prepare('SELECT project_id FROM staff_project_handoffs WHERE id=? AND company_id=?').bind(id,a.companyId).first<{project_id:string}>();
 if(!source)throw new ApiError(404,'PROJECT_HANDOFF_NOT_FOUND','Project handoff not found.');await project(db,a,source.project_id);return `/projects/${encodeURIComponent(source.project_id)}#project-structure`;
}
export async function projectHandoffWork(a:WorkContext):Promise<WorkItem[]>{
 if(!hasPermission(a,'records.read')||!hasModuleAccess(a.modules,'projects'))return [];
 const db=(await holdingResources(a.holdingId)).write,current=await db.prepare('SELECT 1 FROM assignments WHERE id=? AND company_id=? AND user_id=? AND role=? AND iam_assignment_active(id,?) AND iam_principal_active(user_id,company_id,?)').bind(a.assignmentId,a.companyId,a.userId,a.role,Date.now(),Date.now()).first();if(!current)return [];
 const rows=(await db.prepare(`SELECT h.id,h.version,h.project_id,h.recipient_id,h.prepared_by,h.status,h.prepared_at,h.decided_at,p.title,p.lifecycle_status FROM staff_project_handoffs h JOIN staff_projects p ON p.id=h.project_id AND p.company_id=h.company_id
  WHERE h.company_id=? AND (iam_same_person(h.recipient_id,?,h.company_id) OR iam_same_person(h.prepared_by,?,h.company_id)) AND ${projectMember('p')} ORDER BY COALESCE(h.decided_at,h.prepared_at) DESC,h.id LIMIT 200`).bind(a.companyId,a.userId,a.userId,a.userId,a.userId).all<{id:string;version:number;project_id:string;recipient_id:string;prepared_by:string;status:string;prepared_at:number;decided_at:number|null;title:string;lifecycle_status:string}>()).results;
 return rows.map(row=>{const canAct=row.status==='pending'&&row.lifecycle_status==='handoff_pending'&&samePerson(a,row.recipient_id)&&!samePerson(a,row.prepared_by)&&hasPermission(a,'records.write'),href=`/projects/${encodeURIComponent(row.project_id)}#project-structure`;
  return {id:row.id,type:'project_handoff',source:{kind:'project_handoff',id:row.id,version:row.version,label:'Project handoff',href},title:row.title,detail:`Project handoff · ${row.status}`,href,status:row.status,updatedAt:row.decided_at??row.prepared_at,queue:row.status!=='pending'?'completed':canAct?'todo':'waiting',canAct,actionKind:'approval',nextAction:canAct?'Review completed project and accept or request correction':'Await independent receiving review',responsibleQueue:'Receiving project member',relationships:[samePerson(a,row.recipient_id)?'approval':'requested']};});
}
