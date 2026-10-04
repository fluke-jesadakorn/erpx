import "server-only";
import { z } from "zod";
import type { WorkContext } from "./authorization";
import { holdingResources } from "./runtime";
import { requireFlowAccess } from "./flow-access";
import { resolveFlow } from "./flow-routing";
import { withPolicyReadSnapshot } from "./iam-read-snapshot";
import { ApiError } from "./http";
import { decimalToMinor } from "@/lib/finance-money";
import type { FlowKind } from "@/lib/flow";
export const reportQuery=z.object({from:z.iso.date().optional(),to:z.iso.date().optional(),type:z.enum(["all","expense","leave","purchase","service"]).default("all"),status:z.string().max(80).default(""),branch:z.string().max(180).default(""),department:z.string().max(180).default(""),requester:z.string().max(180).default(""),owner:z.string().max(180).default(""),page:z.coerce.number().int().min(1).default(1),mode:z.enum(["period","open","drafts"]).default("period")});
type Candidate={approval_json:string|null;items_json:string|null;id:string;kind:FlowKind;type:string;title:string;status:string;requester:string;branch:string|null;department:string|null;currency:string|null;amount:number|null;units:number|null;submitted_at:number|null;created_at:number;updated_at:number};
export function workReports(actor:WorkContext,raw:Record<string,unknown>,exportAll=false){return withPolicyReadSnapshot(()=>readWorkReports(actor,raw,exportAll));}
async function readWorkReports(actor:WorkContext,raw:Record<string,unknown>,exportAll:boolean){
 const query=reportQuery.parse(raw),{write}=await holdingResources(actor.holdingId);
 const today=new Intl.DateTimeFormat("en-CA",{timeZone:"Asia/Bangkok",year:"numeric",month:"2-digit",day:"2-digit"}).format(new Date());
 const from=query.from??today.slice(0,7)+"-01",to=query.to??today;
 if(from>to)throw new ApiError(422,"REPORT_DATES","End date must be after start date.");
 const start=Date.parse(from+"T00:00:00+07:00"),end=Date.parse(to+"T23:59:59.999+07:00"),now=Date.now();
 const sources=[
  `SELECT NULL::text approval_json,NULL::text items_json,id,'expense' kind,'expense' type,COALESCE(merchant,'Expense claim') title,status,COALESCE(claimant_user_id,created_by) requester,branch_id branch,department_id department,currency,(amount_minor+tax_minor)::bigint amount,NULL::numeric units,submitted_at,created_at,updated_at FROM finance_expenses WHERE company_id=?`,
  `SELECT NULL::text approval_json,NULL::text items_json,id,'leave' kind,'leave' type,number title,state status,created_by requester,branch_id branch,department_id department,NULL::text currency,NULL::bigint amount,COALESCE(calculated_units,days)::numeric units,submitted_at,created_at,updated_at FROM leave_requests WHERE company_id=?`,
  `SELECT approval_json,items_json,r.id,'request' kind,kind type,title,state status,requester_user_id requester,branch_id branch,department_id department,currency,NULL::bigint amount,NULL::numeric units,(SELECT MIN(created_at) FROM employee_request_events e WHERE e.request_id=r.id AND e.company_id=r.company_id AND e.action='submit') submitted_at,created_at,updated_at FROM employee_requests r WHERE company_id=? AND kind IN ('purchase','service')`
 ];
 const candidates=(await Promise.all(sources.map(sql=>write.prepare(sql).bind(actor.companyId).all<Candidate>()))).flatMap(r=>r.results);
 const people=(await write.prepare("SELECT user_id,legal_name FROM hr_people WHERE company_id=? AND merged_into_id IS NULL").bind(actor.companyId).all<{user_id:string;legal_name:string}>()).results;
 const names=new Map(people.map(p=>[p.user_id,p.legal_name]));
 const nodes=(await write.prepare("SELECT id,display_name FROM organization_nodes WHERE company_id=?").bind(actor.companyId).all<{id:string;display_name:string}>()).results;
 const nodeNames=new Map(nodes.map(n=>[n.id,n.display_name]));
 const rows=[];
 type Audit={source:"flow"|"domain";action:string;actor:string;at:number;reason:string;step:string;cycle:number|null;policy:string|null};
 for(const candidate of candidates){
  if(query.type!=="all"&&query.type!==candidate.type||query.status&&query.status!==candidate.status||query.branch&&query.branch!==candidate.branch||query.department&&query.department!==candidate.department||query.requester&&query.requester!==candidate.requester)continue;
  try{await requireFlowAccess(actor,candidate.kind,candidate.id);}catch(error){if(error instanceof ApiError&&[403,404].includes(error.status))continue;throw error;}
  const flow=await resolveFlow(write,actor,candidate.kind,candidate.id),owner=flow.current?.primary;
  if(query.owner&&query.owner!==owner?.userId)continue;
  const draft=candidate.status==="draft",open=!!flow.current&&!draft;
  const submitted=candidate.submitted_at===null?null:Number(candidate.submitted_at);
  const requestApproval=JSON.parse(candidate.approval_json??"{}") as {route?:{steps:{userId:string;stage?:string;snapshot?:unknown}[]};decisions?:{action?:string;userId:string;at?:number;reason?:string;note?:string;stage?:string}[]};
  const estimate=candidate.type==="purchase"?JSON.parse(candidate.items_json??"[]").reduce((sum:number,item:{estimatedCost?:string})=>sum+(item.estimatedCost?Number(decimalToMinor(item.estimatedCost)):0),0):null;
  rows.push({...candidate,approval_json:undefined,items_json:undefined,estimate,savedRequestRoute:requestApproval.route?.steps.map(s=>({person:names.get(s.userId)??"Recorded reviewer",step:s.stage??"Approval",policy:s.snapshot??null}))??[],stageEnteredAt:null as number|null,stageWaitDays:null as number|null,completionDays:null as number|null,amount:candidate.amount===null?null:Number(candidate.amount),units:candidate.units===null?null:Number(candidate.units),requesterName:names.get(candidate.requester)??"Company member",branchName:nodeNames.get(candidate.branch??"")??"Company",departmentName:nodeNames.get(candidate.department??"")??"—",ownerId:owner?.userId??null,owner:owner?.name??flow.current?.teamName??"—",stage:flow.current?.action??"Complete",blocker:flow.blocker,audit:flow.history.map(h=>({source:"flow",action:h.action,actor:h.actorName,at:h.at,reason:h.reason,step:h.step,cycle:null,policy:null}) as Audit),submittedAt:submitted,ageDays:submitted&&open?Math.floor((now-submitted)/86400000):null,open,draft,href:candidate.kind==="expense"?`expenses/${candidate.id}`:candidate.kind==="leave"?`tasks/people_payroll/${candidate.id}?from=leave`:`requests/${candidate.id}`});
 }
 // Route snapshots and domain decisions are read only after the owner has allowed each record.
 const ids=JSON.stringify(rows.map(r=>r.id));
 const audit=(await write.prepare(`SELECT i.record_id,d.id,d.action,d.actor_user_id actor,d.created_at at,d.reason,COALESCE(s.stage,s.authority,'') step,i.cycle,s.route_snapshot_json policy
 FROM approval_instances i JOIN approval_decisions d ON d.instance_id=i.id LEFT JOIN approval_steps s ON s.id=d.step_id
 WHERE i.company_id=? AND i.record_id IN (SELECT jsonb_array_elements_text(?::jsonb)) ORDER BY d.created_at,d.id`).bind(actor.companyId,ids).all<{record_id:string;id:string;action:string;actor:string;at:number;reason:string;step:string;cycle:number;policy:string|null}>()).results;
 const events=(await write.prepare(`SELECT request_id record_id,action,actor_id actor,created_at at,note reason FROM employee_request_events WHERE company_id=? AND request_id IN (SELECT jsonb_array_elements_text(?::jsonb)) ORDER BY created_at,id`).bind(actor.companyId,ids).all<{record_id:string;action:string;actor:string;at:number;reason:string}>()).results;
 const savedRoutes=(await write.prepare(`SELECT i.record_id,i.cycle,s.ordinal,s.approver_user_id,s.stage,s.authority,s.route_snapshot_json FROM approval_instances i JOIN approval_steps s ON s.instance_id=i.id WHERE i.company_id=? AND i.record_id IN (SELECT jsonb_array_elements_text(?::jsonb)) ORDER BY i.cycle,s.ordinal`).bind(actor.companyId,ids).all<{record_id:string;cycle:number;ordinal:number;approver_user_id:string;stage:string|null;authority:string;route_snapshot_json:string|null}>()).results;
 for(const row of rows){
   row.audit.push(...audit.filter(e=>e.record_id===row.id).map(e=>({source:"domain" as const,action:e.action,actor:names.get(e.actor)??"Recorded actor",at:Number(e.at),reason:e.reason,step:e.step,cycle:Number(e.cycle),policy:e.policy})),...events.filter(e=>e.record_id===row.id).map(e=>({source:"domain" as const,action:e.action,actor:names.get(e.actor)??"Recorded actor",at:Number(e.at),reason:e.reason,step:"",cycle:null,policy:null})));
   row.audit.sort((a,b)=>a.at-b.at);
   const transition=row.audit.filter(e=>(e.action==="assign"?e.source==="domain":["submit","approve","reject","return","start","resolve","reopen","confirm","close","request-info","reply","fulfill","post","reimburse","prepare","accounting-prepare","accounts-approve","gl-post"].includes(e.action))).at(-1);
   row.stageEnteredAt=transition?.at??null;
   row.stageWaitDays=row.open&&transition?Math.floor((now-transition.at)/86400000):null;
   const completion=row.audit.filter(e=>["confirm","close","reimburse","settle","complete"].includes(e.action)).at(-1);
   row.completionDays=!row.open&&!row.draft&&row.submittedAt&&completion?Math.max(0,(completion.at-row.submittedAt)/86400000):null;
 }
 type Row=typeof rows[number];
 // PostgreSQL performs matching, aggregation and pagination on the policy-scoped projection.
 // No 100/150-row source cap or client aggregation can truncate the report.
 const scoped=JSON.stringify(rows),predicate=query.mode==="open"?"(row->>'open')::boolean":query.mode==="drafts"?"(row->>'draft')::boolean":"(row->>'submittedAt')::bigint BETWEEN ? AND ?";
 const bind=()=>query.mode==="period"?[scoped,start,end]:[scoped];
 const cte=`WITH visible AS (SELECT value row FROM jsonb_array_elements(?::jsonb)), matching AS (SELECT row FROM visible WHERE ${predicate}) `;
 const counts=await write.prepare(cte+`SELECT (SELECT COUNT(*) FROM matching) total,(SELECT COUNT(*) FROM visible WHERE (row->>'open')::boolean) open,(SELECT COUNT(*) FROM visible WHERE (row->>'draft')::boolean) drafts,(SELECT COUNT(*) FROM visible WHERE NOT (row->>'draft')::boolean AND row->>'submittedAt' IS NULL) unknown,(SELECT COUNT(*) FROM matching WHERE row->>'blocker' IS NOT NULL) blocked,(SELECT COALESCE(SUM((row->>'units')::numeric),0) FROM matching) units`).bind(...bind()).first<{total:number;open:number;drafts:number;unknown:number;blocked:number;units:number}>();
 const moneyRows=(await write.prepare(cte+`SELECT row->>'currency' currency,SUM((row->>'amount')::bigint) amount FROM matching WHERE row->>'currency' IS NOT NULL AND row->>'amount' IS NOT NULL GROUP BY row->>'currency'`).bind(...bind()).all<{currency:string;amount:number}>()).results;
 const estimateRows=(await write.prepare(cte+`SELECT row->>'currency' currency,SUM((row->>'estimate')::bigint) amount FROM matching WHERE row->>'currency' IS NOT NULL AND row->>'estimate' IS NOT NULL GROUP BY row->>'currency'`).bind(...bind()).all<{currency:string;amount:number}>()).results;
 const stageCounts=(await write.prepare(cte+`SELECT row->>'stage' stage,COUNT(*) count,MAX((row->>'stageWaitDays')::int) oldest FROM matching GROUP BY row->>'stage' ORDER BY COUNT(*) DESC`).bind(...bind()).all<{stage:string;count:number;oldest:number|null}>()).results;
 const page=(await write.prepare(cte+`SELECT row FROM matching ORDER BY (row->>'updated_at')::bigint DESC,row->>'id' LIMIT ? OFFSET ?`).bind(...bind(),50,(query.page-1)*50).all<{row:Row}>()).results.map(r=>typeof r.row==="string"?JSON.parse(r.row) as Row:r.row);
 const allRows=exportAll?(await write.prepare(cte+`SELECT row FROM matching ORDER BY (row->>'updated_at')::bigint DESC,row->>'id'`).bind(...bind()).all<{row:Row}>()).results.map(r=>typeof r.row==="string"?JSON.parse(r.row) as Row:r.row):[];
 return {query:{...query,from,to},total:Number(counts?.total??0),open:Number(counts?.open??0),drafts:Number(counts?.drafts??0),unknownSubmissionDates:Number(counts?.unknown??0),blocked:Number(counts?.blocked??0),leaveUnits:Number(counts?.units??0),estimates:Object.fromEntries(estimateRows.map(r=>[r.currency,Number(r.amount)])),money:Object.fromEntries(moneyRows.map(r=>[r.currency,Number(r.amount)])),stageCounts:stageCounts.map(r=>({...r,count:Number(r.count)})),rows:page,allRows,savedRoutes:savedRoutes.filter(r=>page.some(p=>p.id===r.record_id)),options:{branches:[...new Map(rows.filter(r=>r.branch).map(r=>[r.branch,{id:r.branch!,name:r.branchName}])).values()],departments:[...new Map(rows.filter(r=>r.department).map(r=>[r.department,{id:r.department!,name:r.departmentName}])).values()],requesters:[...new Map(rows.map(r=>[r.requester,{id:r.requester,name:r.requesterName}])).values()],owners:[...new Map(rows.filter(r=>r.ownerId).map(r=>[r.ownerId,{id:r.ownerId!,name:r.owner}])).values()],statuses:[...new Set(rows.map(r=>r.status))]}};
}
export function workReportCsv(rows:Awaited<ReturnType<typeof workReports>>["rows"]){const cell=(v:unknown)=>{let s=v==null?"":String(v);if(/^[=+@\-\t\r]/.test(s))s="'"+s;return '"'+s.replaceAll('"','""')+'"';};return '\ufeff'+[["Type","ID","Title","Status","Current step","Requester","Branch","Department","Owner","Submitted at","Age days","Currency","Amount minor","Leave units","Blocker","Recorded audit"],...rows.map(r=>[r.type,r.id,r.title,r.status,r.stage,r.requesterName,r.branchName,r.departmentName,r.owner,r.submittedAt?new Date(r.submittedAt).toISOString():null,r.ageDays,r.currency,r.amount,r.units,r.blocker,JSON.stringify(r.audit)])].map(r=>r.map(cell).join(",")).join("\r\n");}
