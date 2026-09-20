import "server-only";
import { z } from "zod";
import type { WorkContext } from "./authorization";
import type { SqlDatabase } from "./runtime/types";
import type { PostgresDatabase } from "./runtime/postgres";
import { holdingResources } from "./runtime";
import { activeStaff, requestRoute, notification, requireCompanyAuthority } from "./staff-policy";
import { approvalView, openApproval, prepareDecision, closeApprovals, participantSql } from "./approvals";
import { executeCommand, replayCommand } from "./commands";
import { ApiError } from "./http";
import { overtimeDetailsSchema, overtimeCommandSchema, overtimeMinutes, overtimeLabels, type OvertimeDetails } from "@/lib/overtime";
import type { WorkItem } from "@/lib/work-item";

const fail = (code: string, message: string, status=409): never => { throw new ApiError(status,code,message); };
export type OvertimeRow = {
 id:string; company_id:string; engagement_id:string; person_id:string; branch_id:string|null; department_id:string|null;
 created_by:string; mode:"advance"|"retrospective"; state:string; timezone:string; start_local:string; end_local:string;
 starts_at:number; ends_at:number; break_minutes:number; minutes:number; reason:string; late_explanation:string;
 roster_shift_id:string|null; planned_snapshot:string|null; actuals_snapshot:string|null; decision_note:string;
 version:number; created_at:number; updated_at:number;
};
export async function overtimeSetup(db:SqlDatabase, actor:WorkContext) {
 const staff=await activeStaff(db,actor.companyId,actor.userId);
 const eligibility=await db.prepare("SELECT eligible FROM overtime_eligibility WHERE company_id=? AND engagement_id=?").bind(actor.companyId,staff.engagement_id).first<{eligible:boolean}>();
 const route=await requestRoute(db,actor.companyId,actor.userId,"leave");
 const blockers=[...(eligibility?.eligible===false || staff.worker_type==='contractor' && !eligibility?.eligible ? ["Ask HR to configure explicit overtime eligibility for your engagement in People settings."] : []),...(route.blockedReason?[route.blockedReason]:[])];
 return {staff,route,blockers};
}
export async function previewOvertime(actor:WorkContext, raw:OvertimeDetails, actual=false, excludeId:string|null=null, database?:SqlDatabase) {
 const input=overtimeDetailsSchema.parse(raw), db=database??(await holdingResources(actor.holdingId)).write;
 const {staff,route,blockers}=await overtimeSetup(db,actor);
 const time=await db.prepare(`SELECT (extract(epoch FROM (?::timestamp AT TIME ZONE ?))*1000)::bigint starts_at,(extract(epoch FROM (?::timestamp AT TIME ZONE ?))*1000)::bigint ends_at,
 to_char((?::timestamp AT TIME ZONE ?) AT TIME ZONE ?,'YYYY-MM-DD"T"HH24:MI') local_start,to_char((?::timestamp AT TIME ZONE ?) AT TIME ZONE ?,'YYYY-MM-DD"T"HH24:MI') local_end`)
 .bind(input.startLocal,input.timezone,input.endLocal,input.timezone,input.startLocal,input.timezone,input.timezone,input.endLocal,input.timezone,input.timezone).first<{starts_at:number;ends_at:number;local_start:string;local_end:string}>();
 if(!time||time.local_start!==input.startLocal||time.local_end!==input.endLocal) fail("OVERTIME_TIME","This local time does not exist in the selected timezone.",422);
 let minutes:number;
 try { minutes=overtimeMinutes(time!.starts_at,time!.ends_at,input.breakMinutes); } catch(e) { return fail("OVERTIME_TIME",(e as Error).message,422); }
 if(input.startLocal.slice(0,10)<staff.start_date || staff.end_date && input.endLocal.slice(0,10)>staff.end_date) blockers.push("The interval must fall within your engagement dates.");
 if(actual||input.mode==='retrospective') {
  if(time!.ends_at>Date.now()) blockers.push("Actual hours cannot be in the future.");
  if(input.mode==='retrospective'&&!input.lateExplanation.trim()) blockers.push("Explain why these hours are being submitted retrospectively.");
 } else if(time!.starts_at<Date.now()) blockers.push("Use a retrospective claim for work that has already started.");
 const overlap=await db.prepare(`SELECT id FROM overtime_requests WHERE company_id=? AND engagement_id=? AND id IS DISTINCT FROM ? AND state NOT IN ('draft','withdrawn','rejected') AND starts_at<? AND ends_at>? LIMIT 1`).bind(actor.companyId,staff.engagement_id,excludeId,time!.ends_at,time!.starts_at).first();
 if(overlap) blockers.push("Another overtime request overlaps this interval.");
 if(input.rosterShiftId&&!await db.prepare(`SELECT 1 FROM people_roster_shifts s JOIN people_roster_weeks w ON w.id=s.week_id JOIN people_roster_assignments a ON a.shift_id=s.id WHERE s.id=? AND w.company_id=? AND w.state='published' AND s.state='active' AND a.state='assigned' AND a.engagement_id=?`).bind(input.rosterShiftId,actor.companyId,staff.engagement_id).first()) blockers.push("The roster reference must be one of your published assigned shifts.");
 const reviewers=await Promise.all(route.steps.map(async step=>({stage:step.authority==='hr_admin'?'Independent HR approval':'Reporting manager',userId:step.userId,name:(await db.prepare('SELECT legal_name FROM hr_people WHERE company_id=? AND user_id=?').bind(actor.companyId,step.userId).first<{legal_name:string}>())?.legal_name??'Assigned reviewer'})));
 return { ...input, startsAt:time!.starts_at,endsAt:time!.ends_at,minutes,hours:minutes/60,blockers,canSubmit:!blockers.length,reviewers };
}
const detailsOf=(r:OvertimeRow):OvertimeDetails=>({mode:r.mode,startLocal:r.start_local,endLocal:r.end_local,timezone:r.timezone,breakMinutes:r.break_minutes,reason:r.reason,lateExplanation:r.late_explanation,rosterShiftId:r.roster_shift_id});
export async function getOvertime(actor:WorkContext,id:string,database?:SqlDatabase) {
 const db=database??(await holdingResources(actor.holdingId)).write;
 const row=await db.prepare(`SELECT o.* FROM overtime_requests o WHERE o.id=? AND o.company_id=? AND (o.created_by=? OR ${participantSql('overtime','o','?','?')})`).bind(id,actor.companyId,actor.userId,actor.userId,actor.assignmentId).first<OvertimeRow>();
 if(!row) return fail('NOT_FOUND','Overtime request not found.',404);
 const approval=await approvalView(db,'overtime',id,actor), own=row.created_by===actor.userId;
 const actions=[...(own&&['draft','rejected'].includes(row.state)?['save','submit']:[]),...(own&&['authorized','returned'].includes(row.state)?['save','actuals']:[]),...(own&&!['approved','withdrawn'].includes(row.state)?['withdraw']:[]),...(approval.canDecide?row.state==='pending_authorization'?['authorize','reject']:row.state==='pending_verification'?['verify','return']:row.state==='pending_hr'?['approve','return']:[]:[])];
 const handoff=await db.prepare('SELECT * FROM overtime_payroll_handoffs WHERE request_id=? AND company_id=?').bind(id,actor.companyId).first();
 return {...row,approval,permittedActions:actions,handoff,details:detailsOf(row)};
}
export type OvertimeDetail=Awaited<ReturnType<typeof getOvertime>>;
export async function listOvertime(actor:WorkContext) {
 const db=(await holdingResources(actor.holdingId)).write;
 const rows=(await db.prepare(`SELECT o.id FROM overtime_requests o WHERE o.company_id=? AND (o.created_by=? OR ${participantSql('overtime','o','?','?')}) ORDER BY o.updated_at DESC LIMIT 100`).bind(actor.companyId,actor.userId,actor.userId,actor.assignmentId).all<{id:string}>()).results;
 return Promise.all(rows.map(r=>getOvertime(actor,r.id,db)));
}
export async function createOvertime(actor:WorkContext,raw:OvertimeDetails,key:string) {
 const input=overtimeDetailsSchema.parse(raw), db=(await holdingResources(actor.holdingId)).write as PostgresDatabase, id=`ot-${key}`;
 return db.transaction(async tx=>{
  const prior=await replayCommand(tx,actor,key,'overtime.create',id,input);if(prior)return prior;
  const p=await previewOvertime(actor,input,false,null,tx), {staff}=await overtimeSetup(tx,actor), now=Date.now();
  return executeCommand(tx,actor,key,'overtime.create',input,id,1,{id,version:1,state:'draft'},[
   tx.prepare(`INSERT INTO overtime_requests(id,company_id,engagement_id,person_id,branch_id,department_id,created_by,mode,timezone,start_local,end_local,starts_at,ends_at,break_minutes,minutes,reason,late_explanation,roster_shift_id,created_at,updated_at) VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?)`).bind(id,actor.companyId,staff.engagement_id,staff.person_id,staff.branch_id,staff.department_id,actor.userId,input.mode,input.timezone,input.startLocal,input.endLocal,p.startsAt,p.endsAt,input.breakMinutes,p.minutes,input.reason,input.lateExplanation,input.rosterShiftId,now,now)
  ],{companyId:actor.companyId,branchId:staff.branch_id,departmentId:staff.department_id});
 });
}
export async function commandOvertime(actor:WorkContext,id:string,raw:z.infer<typeof overtimeCommandSchema>,key:string) {
 const input=overtimeCommandSchema.parse(raw), db=(await holdingResources(actor.holdingId)).write as PostgresDatabase, operation=`overtime.${input.action}`;
 return db.transaction(async tx=>{
  const prior=await replayCommand(tx,actor,key,operation,id,input);if(prior)return prior;
  await tx.prepare('SELECT id FROM overtime_requests WHERE id=? AND company_id=? FOR UPDATE').bind(id,actor.companyId).first();
  const row=await getOvertime(actor,id,tx);
  if(row.version!==input.expectedVersion) fail('STALE_VERSION','This request changed. Reload to review the current version; your entered values are retained.');
  if(!row.permittedActions.includes(input.action)) fail('OVERTIME_ACTION','This action is not available to you at the current stage.',403);
  const version=row.version+1,now=Date.now(),statements=[];let state=row.state;
  if(['save','submit','actuals'].includes(input.action)) {
   const details=input.details??row.details;
   if(details.mode!==row.mode) fail('OVERTIME_MODE','Start a new request to change between advance and retrospective.');
   const actual=input.action==='actuals'||['authorized','returned'].includes(row.state);
   const p=await previewOvertime(actor,details,actual,id,tx), setup=await overtimeSetup(tx,actor);
   if(setup.staff.engagement_id!==row.engagement_id) fail('ENGAGEMENT_CHANGED','This request belongs to a previous engagement. Start a new request.');
   if(input.action!=='save'&&p.blockers.length) fail('OVERTIME_BLOCKED',p.blockers.join(' '));
   statements.push(tx.prepare('UPDATE overtime_requests SET start_local=?,end_local=?,timezone=?,starts_at=?,ends_at=?,break_minutes=?,minutes=?,reason=?,late_explanation=?,roster_shift_id=? WHERE id=?').bind(details.startLocal,details.endLocal,details.timezone,p.startsAt,p.endsAt,details.breakMinutes,p.minutes,details.reason,details.lateExplanation,details.rosterShiftId,id));
   if(input.action!=='save') {
    const authorization=row.mode==='advance'&&!actual;
    state=authorization?'pending_authorization':'pending_verification';
    const route={...setup.route,steps:authorization?setup.route.steps.slice(0,1):setup.route.steps};
    statements.push(...(await openApproval(tx,key,'overtime',id,authorization?'start':'request',row.created_by,null,route,version)).statements);
    statements.push(tx.prepare(`UPDATE overtime_requests SET ${authorization?'planned_snapshot':'actuals_snapshot'}=?::jsonb,decision_note='' WHERE id=?`).bind(JSON.stringify(p),id));
   }
  } else if(input.action==='withdraw') {
   state='withdrawn';statements.push(...closeApprovals(tx,key,'overtime',id,actor.userId,'cancel',input.note,row.version));
  } else {
   const approve=!['return','reject'].includes(input.action), decision=await prepareDecision(tx,actor,key,'overtime',id,approve,input.note,row.version);
   statements.push(...decision.statements);
   state=!approve?(row.state==='pending_authorization'?'rejected':'returned'):row.state==='pending_authorization'?'authorized':row.state==='pending_verification'?'pending_hr':'approved';
   if(state==='approved') {
    if(!decision.final||!row.actuals_snapshot||row.ends_at>now) fail('OVERTIME_REVIEW','Complete independent actual-hours reviews before handoff.');
    const conflict=await tx.prepare("SELECT id FROM hr_time_entries WHERE company_id=? AND engagement_id=? AND work_date BETWEEN ? AND ? AND overtime_hours>0 AND status IN ('draft','submitted','approved') LIMIT 1").bind(actor.companyId,row.engagement_id,row.start_local.slice(0,10),row.end_local.slice(0,10)).first();
    if(conflict) fail('OVERTIME_RECONCILIATION','Manual overtime exists on these dates. Ask Payroll to reconcile that entry before final approval; an approved time entry cannot be overwritten.');
    statements.push(tx.prepare('INSERT INTO overtime_payroll_handoffs(id,request_id,company_id,engagement_id,request_version,work_date,minutes,timezone,approved_by,created_at) VALUES(?,?,?,?,?,?,?,?,?,?)').bind(`handoff-${id}`,id,actor.companyId,row.engagement_id,version,row.start_local.slice(0,10),row.minutes,row.timezone,actor.userId,now));
   }
   statements.push(tx.prepare('UPDATE overtime_requests SET decision_note=? WHERE id=?').bind(input.note,id));
  }
  statements.push(tx.prepare('UPDATE overtime_requests SET state=?,version=?,updated_at=? WHERE id=?').bind(state,version,now,id),notification(tx,row,row.created_by,'overtime',id,version,overtimeLabels[state]));
  return executeCommand(tx,actor,key,operation,input,id,version,{id,state,version},statements,{companyId:row.company_id,branchId:row.branch_id,departmentId:row.department_id});
 });
}
export async function overtimeWork(actor:WorkContext):Promise<WorkItem[]> {
 return (await listOvertime(actor)).map(row=>({id:row.id,type:'overtime',title:'Overtime request',detail:`${row.start_local.slice(0,10)} · ${(row.minutes/60).toFixed(2)} hours`,href:`/overtime/${row.id}`,status:row.state,updatedAt:row.updated_at,queue:row.permittedActions.some(a=>a!=='withdraw')?'todo':['approved','withdrawn','rejected'].includes(row.state)?'completed':'waiting',canAct:row.permittedActions.some(a=>a!=='withdraw'),nextAction:overtimeLabels[row.state],responsibleQueue:row.approval.currentApprover?.approver_name??'Requester',blocker:row.approval.blockedReason,relationships:[...(row.created_by===actor.userId?['requested' as const]:[]),...(row.approval.canDecide?['approval' as const]:[])]}));
}
export async function setOvertimeEligibility(actor:WorkContext,raw:unknown,key:string) {
 const input=z.object({engagementId:z.string().min(1),expectedVersion:z.number().int().nonnegative().default(0),eligible:z.boolean(),topLevel:z.boolean().default(false),reason:z.string().trim().min(3).max(500)}).parse(raw),db=(await holdingResources(actor.holdingId)).write as PostgresDatabase;
 return db.transaction(async tx=>{
  const prior=await replayCommand(tx,actor,key,'overtime.eligibility',input.engagementId,input);if(prior)return prior;
  await requireCompanyAuthority(tx,actor,'leave.policy.manage');
  if(!await tx.prepare('SELECT id FROM hr_engagements WHERE id=? AND company_id=? FOR UPDATE').bind(input.engagementId,actor.companyId).first())fail('NOT_FOUND','Engagement not found.',404);
  const current=await tx.prepare('SELECT version FROM overtime_eligibility WHERE company_id=? AND engagement_id=?').bind(actor.companyId,input.engagementId).first<{version:number}>();
  if((current?.version??0)!==input.expectedVersion)fail('STALE_VERSION','Eligibility changed. Reload settings and review before saving.');
  const version=input.expectedVersion+1;
  return executeCommand(tx,actor,key,'overtime.eligibility',input,input.engagementId,version,{id:input.engagementId,version},[
   tx.prepare('UPDATE hr_engagements SET self_service_top_level=?,version=version+1 WHERE id=? AND company_id=?').bind(input.topLevel,input.engagementId,actor.companyId),
   tx.prepare('INSERT INTO overtime_eligibility(company_id,engagement_id,eligible,reason,updated_by,updated_at,version) VALUES(?,?,?,?,?,?,?) ON CONFLICT(company_id,engagement_id) DO UPDATE SET eligible=excluded.eligible,reason=excluded.reason,updated_by=excluded.updated_by,updated_at=excluded.updated_at,version=excluded.version').bind(actor.companyId,input.engagementId,input.eligible,input.reason,actor.userId,Date.now(),version)
  ]);
 });
}
