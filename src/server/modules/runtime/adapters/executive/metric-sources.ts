import 'server-only';
import {ApiError} from '../../public/http';
import type {SqlDatabase} from '../../public/runtime/types';
import type {MetricId} from '@/lib/executive/canvas';
import type {RecordKind} from '@/lib/record-drawer';
import {metricSourceSelection,type MetricSourceGroup,type MetricSourceRow,type MetricSourcePage} from '../../domain/executive/metric-sources';
import {BusinessRuleError} from '../../../shared';
type Projection={alias:string;id?:string;owner?:RecordKind;ownerId?:string;extra?:string};
const projections:Partial<Record<MetricId,Projection>>={};
function group(metrics:MetricId[],projection:Projection){for(const metric of metrics)projections[metric]=projection;}
group(['receivables','payables','invoiced-sales'],{alias:'finance_invoices',owner:'invoice'});
group(['sales-orders','purchase-orders','open-po-receipts','sales-fulfillment-rate','sales-cancellations','sales-cancellation-value'],{alias:'o',owner:'order'});
group(['cash-receipts'],{alias:'p',owner:'payment'});
group(['close-blockers'],{alias:'latest',owner:'finance_period',ownerId:'latest.period_id'});
group(['invoice-match-exceptions'],{alias:'latest',owner:'supplier_bill'});
group(['headcount'],{alias:'e',id:'e.effective_person_id',owner:'person',ownerId:'e.effective_person_id'});
group(['approved-leave-duration'],{alias:'l',owner:'leave'});
group(['authorized-overtime-duration'],{alias:'o',owner:'overtime'});
group(['payroll-summary'],{alias:'r',owner:'payroll_run'});
group(['my-tasks','open-operations-exceptions','exception-resolution-time','operations-escalations'],{alias:'t',owner:'staff_task'});
group(['inventory'],{alias:'s',id:"s.product_id||':'||s.warehouse_id",owner:'stock_position',ownerId:'s.product_id'});
group(['available-stock'],{alias:'positions',id:"product_id||':'||warehouse_id",owner:'stock_position',ownerId:'product_id'});
group(['inventory-shortages'],{alias:'positions',owner:'reorder_policy',extra:'available_milli<reorder_point_milli'});
group(['inventory-movements'],{alias:'l',owner:'inventory_movement'});
group(['pos-sales'],{alias:'s',owner:'pos_sale'});
group(['net-pos-sales'],{alias:'x',owner:'pos_sale',ownerId:'x.sale_id'});
group(['pos-payment-mix'],{alias:'p',owner:'pos_sale',ownerId:'s.id'});
group(['hotel-rooms','hotel-occupancy','hotel-night-audit-blockers'],{alias:'r'});
group(['hotel-adr','hotel-revpar'],{alias:'n'});
group(['hotel-guest-account-balance'],{alias:'f'});
group(['projects'],{alias:'p',owner:'project'});
group(['overdue-project-milestones','project-dependency-blockers'],{alias:'m',owner:'project',ownerId:'p.id'});
group(['project-actual-hours'],{alias:'l',owner:'project',ownerId:'p.id'});
group(['project-posted-cost'],{alias:'a',owner:'journal',ownerId:'j.id'});
group(['routines'],{alias:'t',owner:'routine_template'});
group(['active-customers'],{alias:'finance_parties',owner:'party'});
group(['recent-customer-activity','collaboration-thread-activity'],{alias:'e'});
group(['duplicate-reviews-pending'],{alias:'customer_duplicate_reviews',owner:'customer_duplicate'});
group(['identity-change-events'],{alias:'e',owner:'customer_identity',ownerId:'c.id'});
group(['open-support-tickets','overdue-support-sla'],{alias:'t',owner:'support_ticket'});
group(['active-support-incidents'],{alias:'t',owner:'support_incident'});
group(['active-assets','asset-condition-location','asset-retirements'],{alias:'a',owner:'asset'});
group(['open-work-orders','blocked-maintenance'],{alias:'w',owner:'maintenance'});
group(['asset-downtime'],{alias:'s',id:'s.asset_id',owner:'asset',ownerId:'s.asset_id'});
group(['quality-holds','quality-releases','inspection-pass-rate'],{alias:'h',owner:'quality_hold'});
group(['opportunity-pipeline'],{alias:'v',owner:'crm_lead',ownerId:'l.id'});
group(['overdue-followups'],{alias:'c',owner:'customer_followup'});
group(['brand-profiles','brand-profile-updates'],{alias:'p',id:"p.scope_kind||':'||p.scope_id"});
group(['active-assignments'],{alias:'x'});
group(['pending-access-reviews'],{alias:'i'});
group(['active-organization-nodes'],{alias:'n'});
group(['pending-structure-changes'],{alias:'c'});
group(['marketing-lead-candidates','marketing-engagement-events'],{alias:'c',owner:'marketing_candidate'});
group(['completed-manufacturing-orders','manufacturing-orders-with-scrap','production-cost'],{alias:'o',owner:'manufacturing_order'});
group(['planning-shortage-scenarios','proposed-planning-recommendations','capacity-feasible-recommendations'],{alias:'s',owner:'planning_scenario'});
group(['open-ecommerce-carts'],{alias:'c',owner:'ecommerce_cart'});
group(['completed-ecommerce-checkouts'],{alias:'x',owner:'ecommerce_checkout'});
group(['ecommerce-return-links'],{alias:'r',owner:'ecommerce_checkout',ownerId:'x.id'});
group(['ecommerce-fulfillment-links'],{alias:'l',owner:'ecommerce_checkout',ownerId:'x.id'});
group(['active-benefit-elections'],{alias:'e',owner:'benefit_enrollment'});
group(['published-benefit-plans'],{alias:'benefit_plans',owner:'benefit_plan'});
group(['learning-completions','overdue-required-learning'],{alias:'a',owner:'learning_assignment'});
group(['open-requisitions'],{alias:'r',owner:'talent_requisition'});
group(['application-stage-counts'],{alias:'a',owner:'talent_application'});
group(['open-service-orders','service-billing-releases','field-visit-completion'],{alias:'o',owner:'field_service'});
group(['approved-agent-versions'],{alias:'v',owner:'agent_version'});
group(['agent-invocation-outcomes'],{alias:'e'});
group(['legal-records','legal-contract-value','legal-exposure'],{alias:'i',owner:'legal'});
group(['active-legal-obligations'],{alias:'s',owner:'legal',ownerId:'i.id'});
group(['active-legal-holds'],{alias:'h',owner:'legal',ownerId:'i.id'});
group(['active-subscriptions'],{alias:'subscriptions'});
group(['overdue-invoices'],{alias:'i'});
/** Parse only this module's trusted SELECT statements; skip quoted strings and nested CTEs. */
function topWords(sql:string){let depth=0,quoted=false;const words:{word:string;at:number}[]=[];for(let i=0;i<sql.length;i++){
 if(sql[i]==="'"){if(quoted&&sql[i+1]==="'"){i++;continue;}quoted=!quoted;continue;}if(quoted)continue;
 if(sql[i]==='('){depth++;continue;}if(sql[i]===')'){depth--;continue;}
 if(depth===0&&/[a-z_]/i.test(sql[i])){let end=i+1;while(end<sql.length&&/[a-z_]/i.test(sql[end]))end++;words.push({word:sql.slice(i,end).toUpperCase(),at:i});i=end-1;}
}return words;}
export function metricSourceQuery(metric:MetricId,sql:string,unit?:string){
 const projection=projections[metric];if(!projection)throw new ApiError(422,'METRIC_SOURCES_UNAVAILABLE','Sources are unavailable for this metric.');
 if(metric==='exception-resolution-time')sql=sql.slice(0,sql.indexOf(', grouped AS'))+" SELECT "+(sql.includes("to_char(to_timestamp(t.updated_at")?"to_char(to_timestamp(t.updated_at/1000.0) AT TIME ZONE 'UTC','YYYY-MM')":"'Median overdue time to completion'")+" label,count(*) value FROM visible_tasks t GROUP BY 1";
 const words=topWords(sql),select=words.find(w=>w.word==='SELECT'),from=words.find(w=>w.word==='FROM'&&w.at>(select?.at??0));
 if(!select||!from)throw new ApiError(500,'METRIC_SOURCE_QUERY_INVALID','The metric source query could not be read.');
 const tail=words.find(w=>['GROUP','ORDER','LIMIT'].includes(w.word)&&w.at>from.at)?.at??sql.length;
 const selection=sql.slice(select.at+6,from.at).trim();
 const label=selection.match(/^([\s\S]+?)\s+label(?:\s*,|\s*$)/i)?.[1];
 if(!label)throw new ApiError(500,'METRIC_SOURCE_LABEL_INVALID','The metric grouping could not be read.');
 let labelExpression=label.replace(/^min\(([\s\S]+)\)$/i,'$1');
 // Quantities group on a stable group_label and unit rather than an aggregate expression.
 if(metric==='available-stock'||metric==='inventory-shortages')labelExpression='group_label';
 const {alias}=projection,id=projection.id??`${alias}.id`;
 const fieldNames=['id','number','journal_number','code','sku','name','title','description','status','state','version','currency','unit','kind','date','entry_date','invoice_date','business_date','movement_date','start_date','end_date','due_date','pay_date','created_at','updated_at','amount_minor','total_minor','outstanding_minor','net_minor','quantity_milli','warehouse_name','product_name','available_milli','reorder_point_milli','roomMetrics','roomRevenueMinor','reason','reference','plan','asset_code','asset_name','segment_start','segment_end','capacity','sold','valid_metrics'];
 const facts=`jsonb_strip_nulls(jsonb_build_object(${fieldNames.map(key=>`'${key}',to_jsonb(${alias})->'${key}'`).join(',')}))`;
 const owner=projection.owner?`'${projection.owner}'`:'NULL::text',ownerId=projection.owner?(projection.ownerId??id):'NULL::text';
 const ownerArgs=projection.owner==='stock_position'?`jsonb_build_object('warehouseId',${alias==='positions'?'warehouse_id':'s.warehouse_id'})`:'NULL::jsonb';
 const body=sql.slice(from.at,tail);const extra=projection.extra?` ${words.some(w=>w.word==='WHERE'&&w.at>from.at&&w.at<tail)?'AND':'WHERE'} ${projection.extra}`:'';
 return `${sql.slice(0,select.at)} SELECT DISTINCT ${labelExpression} group_label,${id}::text id,${facts} facts,${owner} owner_kind,${ownerId}::text owner_id,${ownerArgs} owner_args${unit!==undefined?`,${alias==='positions'?'unit':metric==='inventory-movements'?'p.unit':metric==='approved-leave-duration'?"'days'":"''"} source_unit`:''} ${body}${extra}`;
}
export async function readMetricSources(db:SqlDatabase,metric:MetricId,sql:string,values:unknown[],selection:MetricSourceGroup,raw:{label:string;unit?:string;value?:unknown}[],description:string,reported?:string):Promise<MetricSourcePage>{
 let selectionFacts:ReturnType<typeof metricSourceSelection>;try{selectionFacts=metricSourceSelection(selection,raw);}catch(error){if(error instanceof BusinessRuleError)throw new ApiError(error.status,error.code,error.message);throw error;}const {other,labels}=selectionFacts;
 const query=metricSourceQuery(metric,sql,selection.unit);
 const conditions=other?`group_label NOT IN (${raw.slice(0,99).map(()=>'?').join(',')})`:'group_label=?';
 const args=[...values,...labels,...(selection.unit!==undefined?[selection.unit]:[]),...(selection.recordId?[selection.recordId]:[]),...(selection.recordId?[]:[(selection.page-1)*30])];
 const stored=(await db.prepare(`SELECT id,facts,owner_kind,owner_id,owner_args FROM (${query}) sources WHERE ${conditions}${selection.unit!==undefined?' AND source_unit=?':''}${selection.recordId?' AND id=?':''} ORDER BY id LIMIT ${selection.recordId?'1':'31 OFFSET ?'}`).bind(...args).all<MetricSourceRow>()).results;
 const rows=stored.map(row=>({...row,facts:typeof row.facts==='string'?JSON.parse(row.facts):row.facts,owner_args:typeof row.owner_args==='string'?JSON.parse(row.owner_args):row.owner_args}));
 return {row:{name:selection.label,reported_total:reported,description},rows,page:selection.page,hasNext:rows.length>30};
}

export type {MetricSourceGroup,MetricSourceRow,MetricSourcePage} from '../../domain/executive/metric-sources';
