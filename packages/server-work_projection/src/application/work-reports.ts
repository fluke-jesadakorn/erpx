import type {WorkContext} from "@erpx/server-identity_access/public/authorization";
import type {FlowKind,FlowView} from "@erpx/feature-shared/lib/flow";
import {decimalToMinor} from "@erpx/feature-finance/lib/finance-money";
import {reportQuery,type WorkReportCandidate} from "@erpx/server-work_projection/domain/work-reports";
export {reportQuery} from "@erpx/server-work_projection/domain/work-reports";
type WorkReportRows<Row>={results:Row[]};
export type WorkReportProjection={mode:"period"|"open"|"drafts";scoped:string;start:number;end:number;page:number};
export type WorkReportUnit={candidates(companyId:string):Promise<WorkReportCandidate[]>;access(actor:WorkContext,kind:FlowKind,id:string):Promise<unknown>;flow(actor:WorkContext,kind:FlowKind,id:string):Promise<FlowView>;people(companyId:string):Promise<WorkReportRows<{ user_id: string; legal_name: string; }>>;nodes(companyId:string):Promise<WorkReportRows<{ id: string; display_name: string; }>>;decisions(companyId:string,ids:string):Promise<WorkReportRows<{ record_id: string; id: string; action: string; actor: string; at: number; reason: string; step: string; cycle: number; policy: string | null; }>>;events(companyId:string,ids:string):Promise<WorkReportRows<{ record_id: string; action: string; actor: string; at: number; reason: string; }>>;savedRoutes(companyId:string,ids:string):Promise<WorkReportRows<{ record_id: string; cycle: number; ordinal: number; approver_user_id: string; stage: string | null; authority: string; route_snapshot_json: string | null; }>>;counts(projection:WorkReportProjection):Promise<{ total: number; open: number; drafts: number; unknown: number; blocked: number; units: number; } | null>;money(projection:WorkReportProjection):Promise<WorkReportRows<{ currency: string; amount: number; }>>;estimates(projection:WorkReportProjection):Promise<WorkReportRows<{ currency: string; amount: number; }>>;stageCounts(projection:WorkReportProjection):Promise<WorkReportRows<{ stage: string; count: number; oldest: number | null; }>>;page<Row>(projection:WorkReportProjection):Promise<WorkReportRows<{row:Row}>>;allRows<Row>(projection:WorkReportProjection):Promise<WorkReportRows<{row:Row}>>;};
export function createWorkReports(ports:{resource(holdingId:string):Promise<WorkReportUnit>;now():number;error(status:number,code:string,message:string):Error;errorStatus(error:unknown):number|undefined}){
async function readWorkReports(actor:WorkContext,raw:Record<string,unknown>,exportAll:boolean){
 const query=reportQuery.parse(raw),write=await ports.resource(actor.holdingId);
 const today=new Intl.DateTimeFormat("en-CA",{timeZone:"Asia/Bangkok",year:"numeric",month:"2-digit",day:"2-digit"}).format(new Date(ports.now()));
 const from=query.from??today.slice(0,7)+"-01",to=query.to??today;
 if(from>to)throw ports.error(422,"REPORT_DATES","End date must be after start date.");
 const start=Date.parse(from+"T00:00:00+07:00"),end=Date.parse(to+"T23:59:59.999+07:00"),now=ports.now();
 const candidates=await write.candidates(actor.companyId);
 const people=(await write.people(actor.companyId)).results;
 const names=new Map(people.map(p=>[p.user_id,p.legal_name]));
 const nodes=(await write.nodes(actor.companyId)).results;
 const nodeNames=new Map(nodes.map(n=>[n.id,n.display_name]));
 const rows=[];
 type Audit={source:"flow"|"domain";action:string;actor:string;at:number;reason:string;step:string;cycle:number|null;policy:string|null};
 for(const candidate of candidates){
  if(query.type!=="all"&&query.type!==candidate.type||query.status&&query.status!==candidate.status||query.branch&&query.branch!==candidate.branch||query.department&&query.department!==candidate.department||query.requester&&query.requester!==candidate.requester)continue;
  try{await write.access(actor,candidate.kind,candidate.id);}catch(error){if([403,404].includes(ports.errorStatus(error)??0))continue;throw error;}
  const flow=await write.flow(actor,candidate.kind,candidate.id),owner=flow.current?.primary;
  if(query.owner&&query.owner!==owner?.userId)continue;
  const draft=candidate.status==="draft",open=!!flow.current&&!draft;
  const submitted=candidate.submitted_at===null?null:Number(candidate.submitted_at);
  const requestApproval=JSON.parse(candidate.approval_json??"{}") as {route?:{steps:{userId:string;stage?:string;snapshot?:unknown}[]};decisions?:{action?:string;userId:string;at?:number;reason?:string;note?:string;stage?:string}[]};
  const estimate=candidate.type==="purchase"?JSON.parse(candidate.items_json??"[]").reduce((sum:number,item:{estimatedCost?:string})=>sum+(item.estimatedCost?Number(decimalToMinor(item.estimatedCost)):0),0):null;
  rows.push({...candidate,approval_json:undefined,items_json:undefined,estimate,savedRequestRoute:requestApproval.route?.steps.map(s=>({person:names.get(s.userId)??"Recorded reviewer",step:s.stage??"Approval",policy:s.snapshot??null}))??[],stageEnteredAt:null as number|null,stageWaitDays:null as number|null,completionDays:null as number|null,amount:candidate.amount===null?null:Number(candidate.amount),units:candidate.units===null?null:Number(candidate.units),requesterName:names.get(candidate.requester)??"Company member",branchName:nodeNames.get(candidate.branch??"")??"Company",departmentName:nodeNames.get(candidate.department??"")??"—",ownerId:owner?.userId??null,owner:owner?.name??flow.current?.teamName??"—",stage:flow.current?.action??"Complete",blocker:flow.blocker,audit:flow.history.map(h=>({source:"flow",action:h.action,actor:h.actorName,at:h.at,reason:h.reason,step:h.step,cycle:null,policy:null}) as Audit),submittedAt:submitted,ageDays:submitted&&open?Math.floor((now-submitted)/86400000):null,open,draft,href:candidate.kind==="expense"?`expenses/${candidate.id}`:candidate.kind==="leave"?`tasks/people_payroll/${candidate.id}?from=leave`:`requests/${candidate.id}`});
 }
 // Route snapshots and domain decisions are read only after the owner has allowed each record.
 const ids=JSON.stringify(rows.map(r=>r.id));
 const audit=(await write.decisions(actor.companyId,ids)).results;
 const events=(await write.events(actor.companyId,ids)).results;
 const savedRoutes=(await write.savedRoutes(actor.companyId,ids)).results;
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
 const projection:WorkReportProjection={mode:query.mode,scoped:JSON.stringify(rows),start,end,page:query.page};
 const counts=await write.counts(projection);
 const moneyRows=(await write.money(projection)).results;
 const estimateRows=(await write.estimates(projection)).results;
 const stageCounts=(await write.stageCounts(projection)).results;
 const page=(await write.page<Row>(projection)).results.map(r=>typeof r.row==="string"?JSON.parse(r.row) as Row:r.row);
 const allRows=exportAll?(await write.allRows<Row>(projection)).results.map(r=>typeof r.row==="string"?JSON.parse(r.row) as Row:r.row):[];
 return {query:{...query,from,to},total:Number(counts?.total??0),open:Number(counts?.open??0),drafts:Number(counts?.drafts??0),unknownSubmissionDates:Number(counts?.unknown??0),blocked:Number(counts?.blocked??0),leaveUnits:Number(counts?.units??0),estimates:Object.fromEntries(estimateRows.map(r=>[r.currency,Number(r.amount)])),money:Object.fromEntries(moneyRows.map(r=>[r.currency,Number(r.amount)])),stageCounts:stageCounts.map(r=>({...r,count:Number(r.count)})),rows:page,allRows,savedRoutes:savedRoutes.filter(r=>page.some(p=>p.id===r.record_id)),options:{branches:[...new Map(rows.filter(r=>r.branch).map(r=>[r.branch,{id:r.branch!,name:r.branchName}])).values()],departments:[...new Map(rows.filter(r=>r.department).map(r=>[r.department,{id:r.department!,name:r.departmentName}])).values()],requesters:[...new Map(rows.map(r=>[r.requester,{id:r.requester,name:r.requesterName}])).values()],owners:[...new Map(rows.filter(r=>r.ownerId).map(r=>[r.ownerId,{id:r.ownerId!,name:r.owner}])).values()],statuses:[...new Set(rows.map(r=>r.status))]}};
}

return {workReports:readWorkReports};}
export function workReportCsv(rows:Awaited<ReturnType<ReturnType<typeof createWorkReports>["workReports"]>>["rows"]){const cell=(v:unknown)=>{let s=v==null?"":String(v);if(/^[=+@\-\t\r]/.test(s))s="'"+s;return '"'+s.replaceAll('"','""')+'"';};return '\ufeff'+[["Type","ID","Title","Status","Current step","Requester","Branch","Department","Owner","Submitted at","Age days","Currency","Amount minor","Leave units","Blocker","Recorded audit"],...rows.map(r=>[r.type,r.id,r.title,r.status,r.stage,r.requesterName,r.branchName,r.departmentName,r.owner,r.submittedAt?new Date(r.submittedAt).toISOString():null,r.ageDays,r.currency,r.amount,r.units,r.blocker,JSON.stringify(r.audit)])].map(r=>r.map(cell).join(",")).join("\r\n");}
