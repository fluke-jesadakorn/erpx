import 'server-only';
import {z} from 'zod';
import type {WorkContext} from './authorization';
import type {Row} from './commerce';
import {holdingResources} from './runtime';
import {requirePermission} from './iam-permissions';
import {hasModuleAccess} from '@/data/module-access';
import {ApiError} from './http';

const schema=z.object({from:z.iso.date(),to:z.iso.date(),branchId:z.string().optional(),propertyId:z.string().optional(),campaignId:z.string().optional(),ownerId:z.string().optional()});
function thaiDate(timestamp:number){return new Date(timestamp+7*3600000).toISOString().slice(0,10);}
export function defaultAnalyticsRange(){const now=Date.now();return {from:thaiDate(now-30*86400000),to:thaiDate(now)};}
export type AnalyticsFilters=z.infer<typeof schema>;
function filters(a:WorkContext,raw:unknown){
 const f=schema.parse(raw);
 if(f.from>f.to)throw new ApiError(422,'REPORT_DATE_RANGE','The start date must not follow the end date.');
 if(a.branchId&&f.branchId&&a.branchId!==f.branchId)throw new ApiError(403,'REPORT_SCOPE','Branch is outside your workspace.');
 return f;
}
function bounds(f:AnalyticsFilters){return {start:Date.parse(`${f.from}T00:00:00+07:00`),end:Date.parse(`${f.to}T23:59:59.999+07:00`)};}
function metric(id:string,label:string,value:number|null,unit:string,formula:string,source:string,f:AnalyticsFilters,drilldown:string[]=[]){return {id,label,value,unit,formula,source,from:f.from,to:f.to,timezone:'Asia/Bangkok',drilldown};}
function definition(id:string,unit:string,formula:string,source:string,f:AnalyticsFilters){return {id,unit,formula,source,from:f.from,to:f.to,timezone:'Asia/Bangkok'};}
function n(v:unknown){return Number(v??0);}
function s(v:unknown){return String(v??'');}
function allowed(a:WorkContext,kind:'crm'|'marketing'){
 requirePermission(a,kind==='crm'?'commerce.read':'records.read');
 if(kind==='marketing'&&!hasModuleAccess(a.modules,'marketing'))throw new ApiError(403,'MODULE_FORBIDDEN','Marketing access required.');
}
export async function analyticsOptions(a:WorkContext){
 const {write}=await holdingResources(a.holdingId);
 const [branches,properties,campaigns,owners]=await Promise.all([
  write.prepare("SELECT id,display_name name FROM organization_nodes WHERE company_id=? AND kind IN ('branch','headquarters') AND (?::text IS NULL OR id=?) ORDER BY display_name").bind(a.companyId,a.branchId,a.branchId).all<Row>(),
  write.prepare('SELECT id,name FROM pms_properties WHERE company_id=? AND (?::text IS NULL OR branch_id=?) ORDER BY name').bind(a.companyId,a.branchId,a.branchId).all<Row>(),
  write.prepare('SELECT id,code name FROM marketing_campaigns WHERE company_id=? ORDER BY code').bind(a.companyId).all<Row>(),
  write.prepare("SELECT DISTINCT l.owner_id id,COALESCE(p.legal_name,l.owner_id) name FROM crm_leads l LEFT JOIN hr_people p ON p.user_id=l.owner_id AND p.company_id=l.company_id AND p.merged_into_id IS NULL WHERE l.company_id=? AND (?::text IS NULL OR l.branch_id=?) ORDER BY name").bind(a.companyId,a.branchId,a.branchId).all<Row>()
 ]);
 return {branches:branches.results,properties:properties.results,campaigns:campaigns.results,owners:owners.results};
}

async function baseRows(a:WorkContext,f:AnalyticsFilters){
 const {write}=await holdingResources(a.holdingId);
 const rows=(await write.prepare(`SELECT l.id,l.account_id,l.contact_id,l.created_at,l.owner_id,l.status,l.due_date,
  v.id opportunity_id,v.stage,v.expected_minor,v.currency,v.sales_id,v.reservation_id,v.win_source,v.won_at,
  o.subtotal_minor sales_booked_minor,o.state sales_state,
  r.data_json::jsonb->>'state' reservation_state,p.id property_id,
  (SELECT MIN(COALESCE((st.data_json::jsonb->>'checkedInAt')::bigint,st.updated_at)) FROM pms_records st WHERE st.kind='stay' AND st.property_id=p.id AND st.data_json::jsonb->>'reservationId'=v.reservation_id AND st.data_json::jsonb->>'state' IN ('in_house','checked_out')) checked_in_at
  FROM crm_leads l LEFT JOIN crm_opportunities v ON v.lead_id=l.id
  LEFT JOIN commerce_orders o ON o.id=v.sales_id AND o.company_id=l.company_id
  LEFT JOIN pms_records r ON r.id=v.reservation_id AND r.kind='reservation'
  LEFT JOIN pms_properties p ON p.id=r.property_id AND p.company_id=l.company_id
  WHERE l.company_id=? AND (?::text IS NULL OR l.branch_id=?) AND (?::text IS NULL OR l.department_id=?)
   AND (?::text IS NULL OR l.branch_id=?) AND (?::text IS NULL OR l.owner_id=?)
   AND (?::text IS NULL OR p.id=?)
   AND (?::text IS NULL OR EXISTS(SELECT 1 FROM marketing_touches t WHERE t.lead_id=l.id AND t.campaign_id=?))
  ORDER BY l.created_at,l.id`).bind(a.companyId,a.branchId,a.branchId,a.departmentId,a.departmentId,f.branchId??null,f.branchId??null,f.ownerId??null,f.ownerId??null,f.propertyId??null,f.propertyId??null,f.campaignId??null,f.campaignId??null).all<Row>()).results;
 return {write,rows};
}

type Write=Awaited<ReturnType<typeof holdingResources>>['write'];
async function postedRevenueByLead(write:Write,companyId:string,f:AnalyticsFilters){
 const posted=(await write.prepare(`WITH source_journals AS (
   SELECT v.lead_id,j.id journal_id FROM crm_opportunities v JOIN crm_leads l ON l.id=v.lead_id
    JOIN commerce_fulfillments ful ON ful.order_id=v.sales_id JOIN finance_invoices i ON i.id=ful.invoice_id
    JOIN finance_journals j ON j.id=i.journal_id AND j.company_id=l.company_id AND j.status='posted'
    WHERE l.company_id=? AND j.entry_date BETWEEN ? AND ?
   UNION
   SELECT v.lead_id,j.id FROM crm_opportunities v JOIN crm_leads l ON l.id=v.lead_id
    JOIN commerce_returns ret ON ret.order_id=v.sales_id AND ret.company_id=l.company_id AND ret.status='posted'
    JOIN finance_journals j ON j.id=ret.credit_journal_id AND j.status='posted' AND j.company_id=l.company_id
    WHERE l.company_id=? AND j.entry_date BETWEEN ? AND ?
   UNION
   SELECT v.lead_id,j.id FROM crm_opportunities v JOIN crm_leads l ON l.id=v.lead_id
    JOIN pms_records folio ON folio.kind='folio' AND folio.data_json::jsonb->>'reservationId'=v.reservation_id
    JOIN pms_records entry ON entry.kind='entry' AND entry.property_id=folio.property_id AND entry.data_json::jsonb->>'folioId'=folio.id
    JOIN pms_postings pp ON pp.record_id=entry.id AND pp.property_id=entry.property_id
    JOIN finance_journals j ON j.id=pp.journal_id AND j.company_id=l.company_id AND j.status='posted'
    WHERE l.company_id=? AND j.entry_date BETWEEN ? AND ?
  ) SELECT sj.lead_id,SUM(jl.credit_minor-jl.debit_minor) amount FROM source_journals sj
  JOIN finance_journal_lines jl ON jl.journal_id=sj.journal_id JOIN finance_accounts ac ON ac.id=jl.account_id AND ac.category='revenue'
  GROUP BY sj.lead_id`).bind(companyId,f.from,f.to,companyId,f.from,f.to,companyId,f.from,f.to).all<Row>()).results;
 return new Map(posted.map(r=>[s(r.lead_id),n(r.amount)]));
}

export async function crmAnalytics(a:WorkContext,raw:unknown){
 allowed(a,'crm');const f=filters(a,raw),{start,end}=bounds(f),{write,rows}=await baseRows(a,f);
 const cohort=rows.filter(r=>n(r.created_at)>=start&&n(r.created_at)<=end),ids=cohort.map(r=>s(r.id)),rowIds=new Set(rows.map(r=>s(r.id)));
 const events=(await write.prepare(`SELECT e.lead_id,e.opportunity_id,e.action,e.snapshot_json,e.created_at
  FROM crm_case_events e JOIN crm_leads l ON l.id=e.lead_id
  WHERE l.company_id=? AND e.created_at<=? AND (?::text IS NULL OR l.branch_id=?) AND (?::text IS NULL OR l.department_id=?)
  ORDER BY e.created_at,e.id`).bind(a.companyId,end,a.branchId,a.branchId,a.departmentId,a.departmentId).all<Row>()).results.filter(e=>rowIds.has(s(e.lead_id)));
 const byLead=new Map<string,Row[]>();for(const e of events){const list=byLead.get(s(e.lead_id))??[];list.push(e);byLead.set(s(e.lead_id),list);}
 const qualified=cohort.filter(r=>(byLead.get(s(r.id))??[]).some(e=>e.action==='qualify'));
 const won=cohort.filter(r=>(byLead.get(s(r.id))??[]).some(e=>['win','win-pms'].includes(s(e.action))));
 const lost=cohort.filter(r=>(byLead.get(s(r.id))??[]).some(e=>e.action==='lose'));
 const transitions=new Map<string,string[]>(),lossReasons=new Map<string,string[]>();
 for(const e of events.filter(e=>n(e.created_at)>=start)){
  const action=s(e.action);
  if(!e.opportunity_id&&!['disqualify','reopen'].includes(action))continue;
  transitions.set(action,[...(transitions.get(action)??[]),s(e.lead_id)]);
  if(action==='lose'){
   const reason=typeof e.snapshot_json==='string'?JSON.parse(e.snapshot_json):e.snapshot_json;
   const code=s((reason as {lossReason?:string})?.lossReason)||'unspecified';lossReasons.set(code,[...(lossReasons.get(code)??[]),s(e.lead_id)]);
  }
 }
 const revenue=await postedRevenueByLead(write,a.companyId,f);
 const overdue=(await write.prepare(`SELECT x.id,x.lead_id FROM crm_lead_activities x JOIN crm_leads l ON l.id=x.lead_id WHERE x.company_id=? AND x.status='open' AND x.due_date<? AND (?::text IS NULL OR l.branch_id=?) AND (?::text IS NULL OR x.owner_id=?)`).bind(a.companyId,thaiDate(Math.min(end,Date.now())),a.branchId,a.branchId,f.ownerId??null,f.ownerId??null).all<Row>()).results.filter(x=>rowIds.has(s(x.lead_id)));
 const open=cohort.filter(r=>r.opportunity_id&&!['won','lost'].includes(s(r.stage)));
 const repeat=new Map<string,{total:number;recent:boolean}>();for(const r of rows.filter(r=>r.stage==='won'&&r.account_id&&(r.win_source==='sales'||r.checked_in_at))){const key=s(r.account_id),current=repeat.get(key)??{total:0,recent:false},occurredAt=n(r.win_source==='sales'?r.won_at:r.checked_in_at);current.total++;current.recent ||= occurredAt>=start&&occurredAt<=end;repeat.set(key,current);}
 const count=(id:string,label:string,value:number,formula:string,source:string,drilldown:string[]=[])=>metric(id,label,value,'count',formula,source,f,drilldown);
 return {filters:f,metrics:[
  count('received','Leads received',cohort.length,'Distinct leads created in the selected dates','crm_leads',ids),
  count('qualified','Qualified',qualified.length,'Cohort leads with a qualification event by the end date','crm_case_events',qualified.map(r=>s(r.id))),
  count('won','Won',won.length,'Cohort leads with a confirmed Sales or PMS win event by the end date','crm_case_events',won.map(r=>s(r.id))),
  count('lost','Lost',lost.length,'Cohort leads with a loss event by the end date','crm_case_events',lost.map(r=>s(r.id))),
  metric('conversion','Lead to won conversion',cohort.length?won.length/cohort.length:null,'ratio','Won cohort leads / received cohort leads','crm_case_events + crm_leads',f),
  count('overdue','Overdue follow-ups',overdue.length,'Open CRM activities due before today or the report end date','crm_lead_activities',overdue.map(r=>s(r.lead_id))),
  count('repeat','Repeat customers',Array.from(repeat.values()).filter(v=>v.total>=2&&v.recent).length,'Accounts with at least two confirmed Sales wins or checked-in PMS reservations across history and at least one outcome in the selected dates','crm_accounts + crm_opportunities + pms_records'),
  metric('forecast','Open expected value',open.reduce((sum,r)=>sum+n(r.expected_minor),0),'minor THB','Sum expected net value of open opportunities in the lead cohort','crm_opportunities',f,open.map(r=>s(r.id))),
  metric('booked','Confirmed booked value',won.reduce((sum,r)=>sum+n(r.win_source==='sales'?r.sales_booked_minor:r.expected_minor),0),'minor THB','Sum confirmed Sales order net or PMS reservation net at win; later cancellation stays separate','commerce_orders + pms_records + crm_opportunities',f,won.map(r=>s(r.id))),
  metric('recognized','Posted net revenue',rows.reduce((sum,r)=>sum+(revenue.get(s(r.id))??0),0),'minor THB','Revenue account credits less debits in posted Finance journals, including posted Sales returns, during the selected dates','finance_journals + finance_journal_lines',f,rows.filter(r=>revenue.has(s(r.id))).map(r=>s(r.id)))
 ],definitions:[
  definition('transitions','count','One opportunity event per stage change, plus lead disqualification or reopening, created during the selected dates','crm_case_events',f),
  definition('lossReasons','count','Loss events created during the selected dates, grouped by recorded reason','crm_case_events',f),
  definition('outcomes','record','Cohort wins with the latest state of the linked Sales order or PMS reservation','crm_opportunities + commerce_orders + pms_records',f)
 ],transitions:Array.from(transitions,([action,leadIds])=>({action,count:leadIds.length,leadIds:[...new Set(leadIds)]})),lossReasons:Array.from(lossReasons,([reason,leadIds])=>({reason,count:leadIds.length,leadIds:[...new Set(leadIds)]})),outcomes:won.map(r=>({leadId:s(r.id),source:s(r.win_source),sourceId:s(r.sales_id??r.reservation_id),currentSourceState:s(r.sales_state??r.reservation_state)}))};
}

export async function marketingAnalytics(a:WorkContext,raw:unknown){
 allowed(a,'marketing');const f=filters(a,raw),{start,end}=bounds(f),{write,rows}=await baseRows(a,f),leadIds=new Set(rows.map(r=>s(r.id)));
 const candidates=(await write.prepare(`SELECT c.id,c.crm_lead_id,c.status,c.created_at,c.attribution_json FROM marketing_lead_candidates c
  WHERE c.company_id=? AND (?::text IS NULL OR c.branch_id=?) AND (?::text IS NULL OR c.department_id=?)
   AND (?::text IS NULL OR c.branch_id=?) AND c.created_at BETWEEN ? AND ? ORDER BY c.created_at,c.id`).bind(a.companyId,a.branchId,a.branchId,a.departmentId,a.departmentId,f.branchId??null,f.branchId??null,start,end).all<Row>()).results.filter(c=>{const attribution=typeof c.attribution_json==='string'?JSON.parse(c.attribution_json):c.attribution_json;return c.crm_lead_id?leadIds.has(s(c.crm_lead_id)):!(f.propertyId||f.ownerId)&&(!f.campaignId||attribution?.campaignId===f.campaignId);});
 const touches=(await write.prepare(`SELECT t.lead_id,t.campaign_id,t.source_kind,t.occurred_at,t.id FROM marketing_touches t JOIN crm_leads l ON l.id=t.lead_id
  WHERE t.company_id=? AND l.company_id=? ORDER BY t.occurred_at,t.id`).bind(a.companyId,a.companyId).all<Row>()).results.filter(t=>leadIds.has(s(t.lead_id)));
 const byLead=new Map<string,Row[]>();for(const t of touches){const arr=byLead.get(s(t.lead_id))??[];arr.push(t);byLead.set(s(t.lead_id),arr);}
 const linked=candidates.filter(c=>c.crm_lead_id),linkedLeads=Array.from(new Map(linked.map(c=>[s(c.crm_lead_id),c])).values());
 const qualificationEvents=(await write.prepare("SELECT DISTINCT e.lead_id FROM crm_case_events e JOIN crm_leads l ON l.id=e.lead_id WHERE l.company_id=? AND e.action='qualify' AND e.created_at<=?").bind(a.companyId,end).all<Row>()).results;
 const qualifiedIds=new Set(qualificationEvents.map(e=>s(e.lead_id))),qualified=linkedLeads.filter(c=>qualifiedIds.has(s(c.crm_lead_id))),won=linkedLeads.filter(c=>rows.find(r=>r.id===c.crm_lead_id)?.stage==='won');
 const attribution=(position:'first'|'last')=>{const counts=new Map<string,{campaignId:string|null;source:string;leads:number;wins:number}>();for(const c of linkedLeads){const arr=byLead.get(s(c.crm_lead_id))??[],touch=position==='first'?arr[0]:arr.at(-1),key=touch?.campaign_id?s(touch.campaign_id):'unattributed',source=touch?s(touch.source_kind):'No evidence',old=counts.get(key)??{campaignId:touch?.campaign_id?s(touch.campaign_id):null,source,leads:0,wins:0};if(old.source!==source)old.source='Multiple sources';old.leads++;if(won.includes(c))old.wins++;counts.set(key,old);}return Array.from(counts.values());};
 const costs=(await write.prepare(`SELECT c.campaign_id,COALESCE(SUM(l.debit_minor-l.credit_minor),0) amount FROM marketing_campaign_costs c
  JOIN finance_journal_lines l ON l.id=c.journal_line_id JOIN finance_journals j ON j.id=l.journal_id AND j.status='posted' AND j.company_id=c.company_id
  WHERE c.company_id=? AND j.entry_date BETWEEN ? AND ? GROUP BY c.campaign_id`).bind(a.companyId,f.from,f.to).all<Row>()).results;
 const costMap=new Map(costs.map(c=>[s(c.campaign_id),n(c.amount)]));
 const revenueMap=await postedRevenueByLead(write,a.companyId,f);
 const view=(position:'first'|'last')=>attribution(position).filter(x=>!f.campaignId||x.campaignId===f.campaignId).map(x=>{
  const cost=x.campaignId?costMap.get(x.campaignId)??null:null;
  const relevant=linkedLeads.filter(c=>{const arr=byLead.get(s(c.crm_lead_id))??[],touch=position==='first'?arr[0]:arr.at(-1);return touch?.campaign_id===x.campaignId;});
  const recognizedMinor=relevant.reduce((total,c)=>total+(revenueMap.get(s(c.crm_lead_id))??0),0);
  return {...x,costMinor:cost,recognizedMinor,roi:cost!==null&&cost>0&&x.campaignId!==null?(recognizedMinor-cost)/cost:null,roiReason:x.campaignId===null?'No campaign touch evidence.':cost===null?'No explicitly linked posted Finance cost.':cost<=0?'Posted campaign cost must be greater than zero.':null,candidateIds:relevant.map(c=>s(c.id)),leadIds:relevant.map(c=>s(c.crm_lead_id))};
 });
 const count=(id:string,label:string,value:number,formula:string,source:string,drilldown:string[]=[])=>metric(id,label,value,'count',formula,source,f,drilldown);
 return {filters:f,metrics:[
  count('received','Form submissions',candidates.length,'Distinct candidates received from a form in the selected dates','marketing_lead_candidates',candidates.map(c=>s(c.id))),
  count('pending','Awaiting review',candidates.filter(c=>c.status==='pending').length,'Received candidates still pending review','marketing_lead_candidates'),
  count('handed_off','Handed to CRM',linked.length,'Received candidates with a reviewed CRM lead link','marketing_lead_candidates',linked.map(c=>s(c.id))),
  count('qualified','Qualified in CRM',qualified.length,'Distinct handed-off CRM leads with a qualification event by the end date','crm_case_events',qualified.map(c=>s(c.id))),
  count('won','Confirmed outcome',won.length,'Distinct handed-off CRM leads with a confirmed Sales or PMS win','crm_opportunities',won.map(c=>s(c.id)))
 ],definitions:[
  definition('firstTouch','count / minor THB / ratio','One distinct handed-off lead assigned to its earliest evidence touch; wins are confirmed outcomes, revenue is posted Finance revenue, cost is explicitly linked posted expense, ROI=(revenue-cost)/cost when cost is positive','marketing_touches + crm_opportunities + finance_journals + marketing_campaign_costs',f),
  definition('lastTouch','count / minor THB / ratio','One distinct handed-off lead assigned to its latest evidence touch; wins are confirmed outcomes, revenue is posted Finance revenue, cost is explicitly linked posted expense, ROI=(revenue-cost)/cost when cost is positive','marketing_touches + crm_opportunities + finance_journals + marketing_campaign_costs',f),
  definition('unattributed','count','Distinct handed-off leads without a linked campaign touch','marketing_touches + marketing_lead_candidates',f)
 ],firstTouch:view('first'),lastTouch:view('last'),unattributed:linkedLeads.filter(c=>!(byLead.get(s(c.crm_lead_id))??[]).some(t=>t.campaign_id)).length,unattributedCandidateIds:linkedLeads.filter(c=>!(byLead.get(s(c.crm_lead_id))??[]).some(t=>t.campaign_id)).map(c=>s(c.id))};
}
