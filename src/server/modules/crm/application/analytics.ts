import type {Clock} from "../../shared";
import type {Row} from "../../commerce/public/commerce";
import {parseAnalyticsFilters,analyticsRange,bounds,thaiDate,metric,definition,n,s,type AnalyticsFilters} from "../domain/analytics";
export interface CRMAnalyticsRepository{options():Promise<{branches:Row[];properties:Row[];campaigns:Row[];owners:Row[]}>;baseRows(f:AnalyticsFilters):Promise<Row[]>;revenue(f:AnalyticsFilters):Promise<Map<string,number>>;events(f:AnalyticsFilters,end:number):Promise<Row[]>;overdue(f:AnalyticsFilters,today:string):Promise<Row[]>;candidates(f:AnalyticsFilters,start:number,end:number):Promise<Row[]>;touches(f:AnalyticsFilters):Promise<Row[]>;qualificationEvents(f:AnalyticsFilters,end:number):Promise<Row[]>;costs(f:AnalyticsFilters):Promise<Row[]>;}
export function createCRMAnalyticsApplication(ports:{actor:{branchId:string|null};clock:Clock;authorize(kind:"crm"|"marketing"):void;repository:CRMAnalyticsRepository}){
async function crmAnalytics(raw:unknown){
 ports.authorize("crm");const f=parseAnalyticsFilters(ports.actor,raw),{start,end}=bounds(f),rows=await ports.repository.baseRows(f);
 const cohort=rows.filter(r=>n(r.created_at)>=start&&n(r.created_at)<=end),ids=cohort.map(r=>s(r.id)),rowIds=new Set(rows.map(r=>s(r.id)));
 const events=(await ports.repository.events(f,end)).filter(e=>rowIds.has(s(e.lead_id)));
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
 const revenue=await ports.repository.revenue(f);
 const overdue=(await ports.repository.overdue(f,thaiDate(Math.min(end,ports.clock.now())))).filter(x=>rowIds.has(s(x.lead_id)));
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
async function marketingAnalytics(raw:unknown){
 ports.authorize("marketing");const f=parseAnalyticsFilters(ports.actor,raw),{start,end}=bounds(f),rows=await ports.repository.baseRows(f),leadIds=new Set(rows.map(r=>s(r.id)));
 const candidates=(await ports.repository.candidates(f,start,end)).filter(c=>{const attribution=typeof c.attribution_json==='string'?JSON.parse(c.attribution_json):c.attribution_json;return c.crm_lead_id?leadIds.has(s(c.crm_lead_id)):!(f.propertyId||f.ownerId)&&(!f.campaignId||attribution?.campaignId===f.campaignId);});
 const touches=(await ports.repository.touches(f)).filter(t=>leadIds.has(s(t.lead_id)));
 const byLead=new Map<string,Row[]>();for(const t of touches){const arr=byLead.get(s(t.lead_id))??[];arr.push(t);byLead.set(s(t.lead_id),arr);}
 const linked=candidates.filter(c=>c.crm_lead_id),linkedLeads=Array.from(new Map(linked.map(c=>[s(c.crm_lead_id),c])).values());
 const qualificationEvents=(await ports.repository.qualificationEvents(f,end));
 const qualifiedIds=new Set(qualificationEvents.map(e=>s(e.lead_id))),qualified=linkedLeads.filter(c=>qualifiedIds.has(s(c.crm_lead_id))),won=linkedLeads.filter(c=>rows.find(r=>r.id===c.crm_lead_id)?.stage==='won');
 const attribution=(position:'first'|'last')=>{const counts=new Map<string,{campaignId:string|null;source:string;leads:number;wins:number}>();for(const c of linkedLeads){const arr=byLead.get(s(c.crm_lead_id))??[],touch=position==='first'?arr[0]:arr.at(-1),key=touch?.campaign_id?s(touch.campaign_id):'unattributed',source=touch?s(touch.source_kind):'No evidence',old=counts.get(key)??{campaignId:touch?.campaign_id?s(touch.campaign_id):null,source,leads:0,wins:0};if(old.source!==source)old.source='Multiple sources';old.leads++;if(won.includes(c))old.wins++;counts.set(key,old);}return Array.from(counts.values());};
 const costs=(await ports.repository.costs(f));
 const costMap=new Map(costs.map(c=>[s(c.campaign_id),n(c.amount)]));
 const revenueMap=await ports.repository.revenue(f);
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
return {crmAnalytics,marketingAnalytics,analyticsOptions:()=>ports.repository.options(),defaultAnalyticsRange:()=>analyticsRange(ports.clock.now())};
}
