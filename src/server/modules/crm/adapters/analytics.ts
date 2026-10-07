import { commercialOwnerProjectionSources as commerceProjectionSources } from "../../commerce/public/commercial-owner-projection-sources";
import { commercialOwnerProjectionSources as financeProjectionSources } from "../../finance/public/commercial-owner-projection-sources";
import { commercialOwnerProjectionSources as marketingProjectionSources } from "../../marketing/public/commercial-owner-projection-sources";
import { commercialOwnerProjectionSources as organizationProjectionSources } from "../../organization/public/commercial-owner-projection-sources";
import { commercialOwnerProjectionSources as people_payrollProjectionSources } from "../../people_payroll/public/commercial-owner-projection-sources";
import { commercialOwnerProjectionSources as pmsProjectionSources } from "../../pms/public/commercial-owner-projection-sources";
import 'server-only';

import type {WorkContext} from "@/server/modules/identity_access/public/authorization";
import type {Row} from "@/server/modules/commerce/public/commerce";
import {holdingResources} from "@/server/modules/runtime/public/runtime";
import {requirePermission} from "@/server/modules/identity_access/public/iam-permissions";
import {hasModuleAccess} from '@/data/module-access';
import {ApiError} from "@/server/modules/runtime/public/http";

import {BusinessRuleError} from "../../shared";
import {createCRMAnalyticsApplication,type CRMAnalyticsRepository} from "../application/analytics";
import {n,s,analyticsRange} from "../domain/analytics";
export type {AnalyticsFilters} from "../domain/analytics";
function application(a:WorkContext){let resource:Promise<Awaited<ReturnType<typeof holdingResources>>['write']>|null=null;const database=()=>resource??=holdingResources(a.holdingId).then(x=>x.write);
const repository:CRMAnalyticsRepository={
async options(){const write=await database();

 const [branches,properties,campaigns,owners]=await Promise.all([
  write.prepare(`SELECT id,display_name name FROM (${organizationProjectionSources.organization_nodes}) organization_nodes WHERE company_id=? AND kind IN ('branch','headquarters') AND (?::text IS NULL OR id=?) ORDER BY display_name`).bind(a.companyId,a.branchId,a.branchId).all<Row>(),
  write.prepare(`SELECT id,name FROM (${pmsProjectionSources.pms_properties}) pms_properties WHERE company_id=? AND (?::text IS NULL OR branch_id=?) ORDER BY name`).bind(a.companyId,a.branchId,a.branchId).all<Row>(),
  write.prepare(`SELECT id,code name FROM (${marketingProjectionSources.marketing_campaigns}) marketing_campaigns WHERE company_id=? ORDER BY code`).bind(a.companyId).all<Row>(),
  write.prepare(`SELECT DISTINCT l.owner_id id,COALESCE(p.legal_name,l.owner_id) name FROM crm_leads l LEFT JOIN (${people_payrollProjectionSources.hr_people}) p ON p.user_id=l.owner_id AND p.company_id=l.company_id AND p.merged_into_id IS NULL WHERE l.company_id=? AND (?::text IS NULL OR l.branch_id=?) ORDER BY name`).bind(a.companyId,a.branchId,a.branchId).all<Row>()
 ]);
 return {branches:branches.results,properties:properties.results,campaigns:campaigns.results,owners:owners.results};
},
async baseRows(f){const write=await database();

 const rows=(await write.prepare(`SELECT l.id,l.account_id,l.contact_id,l.created_at,l.owner_id,l.status,l.due_date,
  v.id opportunity_id,v.stage,v.expected_minor,v.currency,v.sales_id,v.reservation_id,v.win_source,v.won_at,
  o.subtotal_minor sales_booked_minor,o.state sales_state,
  r.data_json::jsonb->>'state' reservation_state,p.id property_id,
  (SELECT MIN(COALESCE((st.data_json::jsonb->>'checkedInAt')::bigint,st.updated_at)) FROM (${pmsProjectionSources.pms_records}) st WHERE st.kind='stay' AND st.property_id=p.id AND st.data_json::jsonb->>'reservationId'=v.reservation_id AND st.data_json::jsonb->>'state' IN ('in_house','checked_out')) checked_in_at
  FROM crm_leads l LEFT JOIN crm_opportunities v ON v.lead_id=l.id
  LEFT JOIN (${commerceProjectionSources.commerce_orders}) o ON o.id=v.sales_id AND o.company_id=l.company_id
  LEFT JOIN (${pmsProjectionSources.pms_records}) r ON r.id=v.reservation_id AND r.kind='reservation'
  LEFT JOIN (${pmsProjectionSources.pms_properties}) p ON p.id=r.property_id AND p.company_id=l.company_id
  WHERE l.company_id=? AND (?::text IS NULL OR l.branch_id=?) AND (?::text IS NULL OR l.department_id=?)
   AND (?::text IS NULL OR l.branch_id=?) AND (?::text IS NULL OR l.owner_id=?)
   AND (?::text IS NULL OR p.id=?)
   AND (?::text IS NULL OR EXISTS(SELECT 1 FROM (${marketingProjectionSources.marketing_touches}) t WHERE t.lead_id=l.id AND t.campaign_id=?))
  ORDER BY l.created_at,l.id`).bind(a.companyId,a.branchId,a.branchId,a.departmentId,a.departmentId,f.branchId??null,f.branchId??null,f.ownerId??null,f.ownerId??null,f.propertyId??null,f.propertyId??null,f.campaignId??null,f.campaignId??null).all<Row>()).results;
 return rows;
},
async revenue(f){const write=await database(),companyId=a.companyId;
 const posted=(await write.prepare(`WITH source_journals AS (
   SELECT v.lead_id,j.id journal_id FROM crm_opportunities v JOIN crm_leads l ON l.id=v.lead_id
    JOIN (${commerceProjectionSources.commerce_fulfillments}) ful ON ful.order_id=v.sales_id JOIN (${financeProjectionSources.finance_invoices}) i ON i.id=ful.invoice_id
    JOIN (${financeProjectionSources.finance_journals}) j ON j.id=i.journal_id AND j.company_id=l.company_id AND j.status='posted'
    WHERE l.company_id=? AND j.entry_date BETWEEN ? AND ?
   UNION
   SELECT v.lead_id,j.id FROM crm_opportunities v JOIN crm_leads l ON l.id=v.lead_id
    JOIN (${commerceProjectionSources.commerce_returns}) ret ON ret.order_id=v.sales_id AND ret.company_id=l.company_id AND ret.status='posted'
    JOIN (${financeProjectionSources.finance_journals}) j ON j.id=ret.credit_journal_id AND j.status='posted' AND j.company_id=l.company_id
    WHERE l.company_id=? AND j.entry_date BETWEEN ? AND ?
   UNION
   SELECT v.lead_id,j.id FROM crm_opportunities v JOIN crm_leads l ON l.id=v.lead_id
    JOIN (${pmsProjectionSources.pms_records}) folio ON folio.kind='folio' AND folio.data_json::jsonb->>'reservationId'=v.reservation_id
    JOIN (${pmsProjectionSources.pms_records}) entry ON entry.kind='entry' AND entry.property_id=folio.property_id AND entry.data_json::jsonb->>'folioId'=folio.id
    JOIN (${pmsProjectionSources.pms_postings}) pp ON pp.record_id=entry.id AND pp.property_id=entry.property_id
    JOIN (${financeProjectionSources.finance_journals}) j ON j.id=pp.journal_id AND j.company_id=l.company_id AND j.status='posted'
    WHERE l.company_id=? AND j.entry_date BETWEEN ? AND ?
  ) SELECT sj.lead_id,SUM(jl.credit_minor-jl.debit_minor) amount FROM source_journals sj
  JOIN (${financeProjectionSources.finance_journal_lines}) jl ON jl.journal_id=sj.journal_id JOIN (${financeProjectionSources.finance_accounts}) ac ON ac.id=jl.account_id AND ac.category='revenue'
  GROUP BY sj.lead_id`).bind(companyId,f.from,f.to,companyId,f.from,f.to,companyId,f.from,f.to).all<Row>()).results;
 return new Map(posted.map(r=>[s(r.lead_id),n(r.amount)]));
},
async events(f,end){const write=await database();return (await write.prepare(`SELECT e.lead_id,e.opportunity_id,e.action,e.snapshot_json,e.created_at
  FROM crm_case_events e JOIN crm_leads l ON l.id=e.lead_id
  WHERE l.company_id=? AND e.created_at<=? AND (?::text IS NULL OR l.branch_id=?) AND (?::text IS NULL OR l.department_id=?)
  ORDER BY e.created_at,e.id`).bind(a.companyId,end,a.branchId,a.branchId,a.departmentId,a.departmentId).all<Row>()).results;},
async overdue(f,today){const write=await database();return (await write.prepare(`SELECT x.id,x.lead_id FROM crm_lead_activities x JOIN crm_leads l ON l.id=x.lead_id WHERE x.company_id=? AND x.status='open' AND x.due_date<? AND (?::text IS NULL OR l.branch_id=?) AND (?::text IS NULL OR x.owner_id=?)`).bind(a.companyId,today,a.branchId,a.branchId,f.ownerId??null,f.ownerId??null).all<Row>()).results;},
async candidates(f,start,end){const write=await database();return (await write.prepare(`SELECT c.id,c.crm_lead_id,c.status,c.created_at,c.attribution_json FROM (${marketingProjectionSources.marketing_lead_candidates}) c
  WHERE c.company_id=? AND (?::text IS NULL OR c.branch_id=?) AND (?::text IS NULL OR c.department_id=?)
   AND (?::text IS NULL OR c.branch_id=?) AND c.created_at BETWEEN ? AND ? ORDER BY c.created_at,c.id`).bind(a.companyId,a.branchId,a.branchId,a.departmentId,a.departmentId,f.branchId??null,f.branchId??null,start,end).all<Row>()).results;},
async touches(){const write=await database();return (await write.prepare(`SELECT t.lead_id,t.campaign_id,t.source_kind,t.occurred_at,t.id FROM (${marketingProjectionSources.marketing_touches}) t JOIN crm_leads l ON l.id=t.lead_id
  WHERE t.company_id=? AND l.company_id=? ORDER BY t.occurred_at,t.id`).bind(a.companyId,a.companyId).all<Row>()).results;},
async qualificationEvents(f,end){const write=await database();return (await write.prepare("SELECT DISTINCT e.lead_id FROM crm_case_events e JOIN crm_leads l ON l.id=e.lead_id WHERE l.company_id=? AND e.action='qualify' AND e.created_at<=?").bind(a.companyId,end).all<Row>()).results;},
async costs(f){const write=await database();return (await write.prepare(`SELECT c.campaign_id,COALESCE(SUM(l.debit_minor-l.credit_minor),0) amount FROM (${marketingProjectionSources.marketing_campaign_costs}) c
  JOIN (${financeProjectionSources.finance_journal_lines}) l ON l.id=c.journal_line_id JOIN (${financeProjectionSources.finance_journals}) j ON j.id=l.journal_id AND j.status='posted' AND j.company_id=c.company_id
  WHERE c.company_id=? AND j.entry_date BETWEEN ? AND ? GROUP BY c.campaign_id`).bind(a.companyId,f.from,f.to).all<Row>()).results;},
};
return createCRMAnalyticsApplication({actor:a,clock:{now:()=>Date.now()},repository,authorize(kind){requirePermission(a,kind==='crm'?'commerce.read':'records.read');if(kind==='marketing'&&!hasModuleAccess(a.modules,'marketing'))throw new ApiError(403,'MODULE_FORBIDDEN','Marketing access required.');}});}
async function translated<Result>(action:()=>Promise<Result>){try{return await action();}catch(error){if(error instanceof BusinessRuleError)throw new ApiError(error.status,error.code,error.message);throw error;}}
export function defaultAnalyticsRange(){return analyticsRange(Date.now());}
export function analyticsOptions(a:WorkContext){return application(a).analyticsOptions();}
export function crmAnalytics(a:WorkContext,raw:unknown){return translated(()=>application(a).crmAnalytics(raw));}
export function marketingAnalytics(a:WorkContext,raw:unknown){return translated(()=>application(a).marketingAnalytics(raw));}
