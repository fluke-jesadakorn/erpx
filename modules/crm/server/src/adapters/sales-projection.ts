import {crmLeadProjectionScope} from "@erpx/crm-server/adapters/record-purchases";
import {crmSalesOpportunitySql} from './read-tail-r37-sql';
import 'server-only';
import type {SqlDatabase} from '@erpx/platform-runtime-server/public/runtime/types';
import type {WorkContext} from '@erpx/identity-access-server/public/authorization';
import type {Row} from '@erpx/commerce-server/public/commerce';
function orderScope(actor: WorkContext, alias = "o") {
  return { sql: `${alias}.company_id=? AND (?::text IS NULL OR ${alias}.branch_id=?) AND (?::text IS NULL OR ${alias}.department_id=?)`, args: [actor.companyId, actor.branchId, actor.branchId, actor.departmentId, actor.departmentId] };
}
export async function salesOpportunityCounts(write:SqlDatabase,actor:WorkContext,ids:string[]){if(!ids.length)return [];const purchase=await crmLeadProjectionScope(write,actor,'l'),lead=orderScope(actor,'l'),placeholders=ids.map(()=>'?').join(',');return (await write.prepare(crmSalesOpportunitySql('v.customer_id,COUNT(*) count','crm_opportunities v JOIN crm_leads l ON l.id=v.lead_id',`${lead.sql} AND ${purchase.where} AND v.customer_id IN (${placeholders}) AND l.status='qualified' AND v.stage NOT IN ('won','lost') GROUP BY v.customer_id`)).bind(...lead.args,...purchase.args, ...ids).all<{ customer_id: string; count: number }>()).results;}
export async function salesOpenOpportunities(write:SqlDatabase,actor:WorkContext,id:string){const purchase=await crmLeadProjectionScope(write,actor,'l'),lead=orderScope(actor,'l');return (await write.prepare(crmSalesOpportunitySql('l.id,l.title,l.due_date,v.stage,v.close_date,v.expected_minor,v.currency','crm_leads l JOIN crm_opportunities v ON v.lead_id=l.id',`${lead.sql} AND ${purchase.where} AND v.customer_id=? AND l.status='qualified' AND v.stage NOT IN ('won','lost') ORDER BY v.close_date,l.id LIMIT 30`)).bind(...lead.args,...purchase.args, id).all<Row>()).results;}
