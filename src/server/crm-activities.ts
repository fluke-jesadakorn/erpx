import 'server-only';
import {z} from 'zod';
import type {WorkContext} from './authorization';
import type {SqlDatabase} from './runtime/types';
import type {Row} from './commerce';
import type {WorkItem} from '@/lib/work-item';
import {holdingResources} from './runtime';
import {requirePermission} from './iam-permissions';
import {hasPermission} from '@/lib/permissions';
import {samePerson} from '@/lib/iam';
import {assertion,executeCommand,replayCommand} from './commands';
import {ApiError} from './http';
import {salesRecipientEligibility} from './sales-recipient';

const text=z.string().trim().min(3).max(2000),version=z.number().int().positive(),id=z.string().min(1);
export const crmActivityCreateSchema=z.object({leadId:id,expectedVersion:version,activityType:z.enum(['call','email','meeting','task']),subject:z.string().trim().min(3).max(200),details:text,ownerId:id,dueDate:z.iso.date()});
export const crmActivityChangeSchema=z.discriminatedUnion('action',[
 z.object({action:z.literal('reassign'),expectedVersion:version,ownerId:id,dueDate:z.iso.date(),reason:text}),
 z.object({action:z.enum(['complete','cancel']),expectedVersion:version,reason:text}),
]);
const scope="l.company_id=? AND (?::text IS NULL OR l.branch_id=?) AND (?::text IS NULL OR l.department_id=?)";
const args=(a:WorkContext)=>[a.companyId,a.branchId,a.branchId,a.departmentId,a.departmentId];
const eligible=salesRecipientEligibility('l','l.owner_id');
type Activity={id:string;company_id:string;lead_id:string;activity_type:string;subject:string;details:string;owner_id:string;due_date:string;status:string;outcome:string|null;version:number;created_by:string;created_at:number;updated_at:number;lead_title:string;lead_status:string;lead_stage:string|null;branch_id:string;department_id:string|null;owner_name?:string;owner_eligible?:number};

async function lead(db:SqlDatabase,a:WorkContext,leadId:string){
 const row=await db.prepare(`SELECT l.id,l.company_id,l.branch_id,l.department_id,l.title,l.status,l.version,v.stage FROM crm_leads l LEFT JOIN crm_opportunities v ON v.lead_id=l.id WHERE l.id=? AND ${scope}`).bind(leadId,...args(a)).first<Row>();
 if(!row)throw new ApiError(404,'CRM_LEAD_NOT_FOUND','Lead not found in this scope.');
 return row;
}
async function rows(db:SqlDatabase,a:WorkContext,leadId:string){
 return (await db.prepare(`SELECT x.*,l.title lead_title,l.status lead_status,v.stage lead_stage,l.branch_id,l.department_id,(SELECT legal_name FROM hr_people WHERE company_id=x.company_id AND user_id=x.owner_id AND merged_into_id IS NULL LIMIT 1) owner_name,CASE WHEN ${eligible} THEN 1 ELSE 0 END owner_eligible FROM crm_lead_activities x JOIN crm_leads l ON l.id=x.lead_id LEFT JOIN crm_opportunities v ON v.lead_id=l.id WHERE x.company_id=? AND x.lead_id=? AND ${scope} ORDER BY x.due_date,x.created_at,x.id`).bind(a.companyId,leadId,...args(a)).all<Activity>()).results;
}
async function recipients(db:SqlDatabase,a:WorkContext,leadId:string){
 return (await db.prepare(`SELECT DISTINCT a.user_id id,p.legal_name name FROM assignments a JOIN hr_people p ON p.user_id=a.user_id AND p.company_id=a.company_id JOIN crm_leads l ON l.id=? WHERE a.company_id=? AND ${salesRecipientEligibility('l','a.user_id')} ORDER BY p.legal_name`).bind(leadId,a.companyId).all<{id:string;name:string}>()).results;
}
function canChange(a:WorkContext,row:Activity){return hasPermission(a,'commerce.sales')&&(samePerson(a,row.created_by)||samePerson(a,row.owner_id)||hasPermission(a,'commerce.approve'));}
function notify(db:SqlDatabase,id:string,key:string){return db.prepare("INSERT INTO staff_notifications(id,company_id,user_id,record_type,record_id,record_version,message,created_at) SELECT ?,company_id,owner_id,'crm_activity',id,version,CASE WHEN status='open' THEN 'CRM activity assigned: review the due date and next action' ELSE 'CRM activity outcome recorded' END,updated_at FROM crm_lead_activities WHERE id=? ON CONFLICT DO NOTHING").bind(`crm-activity-notice-${key}`,id);}

export async function crmActivityWorkspace(a:WorkContext,leadId:string){
 requirePermission(a,'commerce.read');const {write}=await holdingResources(a.holdingId),l=await lead(write,a,leadId),items=await rows(write,a,leadId),history=(await write.prepare('SELECT e.* FROM crm_lead_activity_events e JOIN crm_lead_activities x ON x.id=e.activity_id WHERE x.company_id=? AND x.lead_id=? ORDER BY e.created_at DESC,e.version DESC').bind(a.companyId,leadId).all<Row>()).results;
 const manager=hasPermission(a,'commerce.sales')&&hasPermission(a,'commerce.approve'),sales=hasPermission(a,'commerce.sales');
 return {lead:{id:String(l.id),title:String(l.title),status:String(l.status),stage:l.stage?String(l.stage):null,version:Number(l.version)},items:items.map(x=>({...x,ownerEligible:Number(x.owner_eligible)>0,canComplete:x.status==='open'&&sales&&x.owner_id===a.userId&&Number(x.owner_eligible)>0,canManage:x.status==='open'&&manager,canChange:x.status==='open'&&canChange(a,x)})),history,recipients:await recipients(write,a,leadId),canCreate:sales&&!['disqualified'].includes(String(l.status))&&!['won','lost'].includes(String(l.stage??'')),canManage:manager};
}

export async function createCRMActivity(a:WorkContext,raw:unknown,key:string){
 const input=crmActivityCreateSchema.parse(raw);requirePermission(a,'commerce.sales');requirePermission(a,'commerce.read');const {write}=await holdingResources(a.holdingId),id=`crm-activity-${key}`,operation='crm.activity.create',prior=await replayCommand(write,a,key,operation,id,input);if(prior)return prior;const l=await lead(write,a,input.leadId),now=Date.now();
 if(!['open','qualified'].includes(String(l.status))||['won','lost'].includes(String(l.stage??'')))throw new ApiError(409,'CRM_ACTIVITY_SOURCE_CLOSED','Only an open or active opportunity can receive a new activity.');
 return executeCommand(write,a,key,operation,input,id,1,{id,version:1},[
  assertion(write,key,'lead-source',`EXISTS(SELECT 1 FROM crm_leads l LEFT JOIN crm_opportunities v ON v.lead_id=l.id WHERE l.id=? AND l.version=? AND l.status IN ('open','qualified') AND COALESCE(v.stage,'discovery') NOT IN ('won','lost') AND ${scope})`,[input.leadId,input.expectedVersion,...args(a)]),
  write.prepare('INSERT INTO crm_lead_activities(id,company_id,lead_id,activity_type,subject,details,owner_id,due_date,created_by,created_at,updated_at) VALUES(?,?,?,?,?,?,?,?,?,?,?)').bind(id,a.companyId,input.leadId,input.activityType,input.subject,input.details,input.ownerId,input.dueDate,a.userId,now,now),
  assertion(write,key,'recipient',`EXISTS(SELECT 1 FROM crm_lead_activities x JOIN crm_leads l ON l.id=x.lead_id WHERE x.id=? AND ${eligible})`,[id]),
  write.prepare("INSERT INTO crm_lead_activity_events VALUES(?,?,1,'create',?,?,?,?,?,?)").bind(`crm-activity-event-${key}`,id,a.userId,input.ownerId,input.dueDate,input.details,JSON.stringify(input),now),
  notify(write,id,key)
 ],{companyId:a.companyId,branchId:String(l.branch_id),departmentId:l.department_id?String(l.department_id):null});
}

export async function changeCRMActivity(a:WorkContext,id:string,raw:unknown,key:string){
 const input=crmActivityChangeSchema.parse(raw);requirePermission(a,'commerce.sales');requirePermission(a,'commerce.read');const {write}=await holdingResources(a.holdingId),operation=`crm.activity.${input.action}`,prior=await replayCommand(write,a,key,operation,id,input);if(prior)return prior;
 const r=await write.prepare(`SELECT x.*,l.title lead_title,l.status lead_status,v.stage lead_stage,l.branch_id,l.department_id,(SELECT legal_name FROM hr_people WHERE company_id=x.company_id AND user_id=x.owner_id AND merged_into_id IS NULL LIMIT 1) owner_name,CASE WHEN ${eligible} THEN 1 ELSE 0 END owner_eligible FROM crm_lead_activities x JOIN crm_leads l ON l.id=x.lead_id LEFT JOIN crm_opportunities v ON v.lead_id=l.id WHERE x.id=? AND x.company_id=? AND ${scope}`).bind(id,a.companyId,...args(a)).first<Activity>();
 if(!r)throw new ApiError(404,'CRM_ACTIVITY_NOT_FOUND','CRM activity not found in this scope.');
 const manage=hasPermission(a,'commerce.approve');if(input.action==='reassign'&&!manage||input.action==='cancel'&&!manage&&r.owner_id!==a.userId||input.action==='complete'&&r.owner_id!==a.userId)throw new ApiError(403,'CRM_ACTIVITY_OWNER_REQUIRED','The assigned salesperson completes an activity; a sales manager manages responsibility.');
 if(r.status!=='open')throw new ApiError(409,'CRM_ACTIVITY_STATE_CHANGED','This activity is already closed.');
 const owner=input.action==='reassign'?input.ownerId:String(r.owner_id),due=input.action==='reassign'?input.dueDate:String(r.due_date),status=input.action==='complete'?'completed':input.action==='cancel'?'cancelled':'open',now=Date.now(),steps=[assertion(write,key,'version',`EXISTS(SELECT 1 FROM crm_lead_activities x JOIN crm_leads l ON l.id=x.lead_id WHERE x.id=? AND x.company_id=? AND x.version=? AND x.status='open' AND ${scope})`,[id,a.companyId,input.expectedVersion,...args(a)]),write.prepare('UPDATE crm_lead_activities SET owner_id=?,due_date=?,status=?,outcome=?,version=version+1,updated_at=? WHERE id=?').bind(owner,due,status,input.action==='reassign'?null:input.reason,now,id)];
 if(input.action==='reassign')steps.push(assertion(write,key,'recipient',`EXISTS(SELECT 1 FROM crm_lead_activities x JOIN crm_leads l ON l.id=x.lead_id WHERE x.id=? AND ${eligible})`,[id]));
 steps.push(write.prepare('INSERT INTO crm_lead_activity_events VALUES(?,?,?,?,?,?,?,?,?,?)').bind(`crm-activity-event-${key}`,id,input.expectedVersion+1,input.action,a.userId,owner,due,input.reason,JSON.stringify(input),now),notify(write,id,key));
 return executeCommand(write,a,key,operation,input,id,input.expectedVersion+1,{id,version:input.expectedVersion+1,status},steps,{companyId:a.companyId,branchId:String(r.branch_id),departmentId:r.department_id?String(r.department_id):null});
}

export async function crmActivityWork(a:WorkContext):Promise<WorkItem[]>{if(!hasPermission(a,'commerce.read'))return [];const {write}=await holdingResources(a.holdingId),all=(await write.prepare(`SELECT x.*,l.title lead_title,l.status lead_status,v.stage lead_stage,l.branch_id,l.department_id,(SELECT legal_name FROM hr_people WHERE company_id=x.company_id AND user_id=x.owner_id AND merged_into_id IS NULL LIMIT 1) owner_name,CASE WHEN ${eligible} THEN 1 ELSE 0 END owner_eligible FROM crm_lead_activities x JOIN crm_leads l ON l.id=x.lead_id LEFT JOIN crm_opportunities v ON v.lead_id=l.id WHERE ${scope} ORDER BY x.due_date,x.updated_at DESC`).bind(...args(a)).all<Activity>()).results;return all.map(r=>{const open=r.status==='open',eligibleOwner=Number(r.owner_eligible)>0,canAct=open&&eligibleOwner&&r.owner_id===a.userId&&hasPermission(a,'commerce.sales');return {id:String(r.id),type:'crm_activity',title:`CRM activity · ${r.subject}`,detail:String(r.details),href:`/crm/${r.lead_id}#crm-activities`,status:String(r.status),updatedAt:Number(r.updated_at),queue:!open?'completed':canAct?'todo':'waiting',responsibleQueue:String(r.owner_name??'Assigned salesperson'),nextAction:open?String(r.subject):'Activity outcome recorded',dueAt:Date.parse(String(r.due_date)+'T00:00:00Z'),blocker:open&&!eligibleOwner?'Assigned owner is no longer eligible; a sales manager must reassign with a reason':null,relationships:[r.owner_id===a.userId?'assigned':r.created_by===a.userId?'requested':'team']};});}
