import "server-only";
import type { SqlDatabase } from "../../runtime/public/runtime/types";
import type { WorkContext } from "../../identity_access/public/authorization";
import { ApiError } from "../../runtime/public/http";
import { financePackageEnforcementMode, checkPurchasedCapability } from "../../commercial/public/purchase-command-boundary";
import { purchasedCapabilityPredicate, packageClockSql } from "../../commercial/public/purchased-capability-sql";

export const groupCompanyPurchases = (companies: readonly string[]) => [...new Set(companies.filter(company=>typeof company==="string"&&company.length>0))].map(companyId => ({companyId,capability:"finance.accounting",purpose:"history" as const}));
export async function requireGroupCompanyHistory(database: SqlDatabase, companyId: string) {
  await checkPurchasedCapability(database,await financePackageEnforcementMode(),{companyId,capability:"finance.accounting",purpose:"history"});
}
export async function groupHistoryPredicate() {
  const enforced = await financePackageEnforcementMode() === "enforced";
  return (company: string) => enforced ? `(SELECT ${purchasedCapabilityPredicate("group_purchase.company_id","finance.accounting","history","finance_group_purchase_clock.now_ms")} FROM (SELECT ${company} AS company_id) group_purchase CROSS JOIN finance_group_purchase_clock)` : "TRUE";
}
/** A retained edition contains complete source books; never substitute zero
 * balances for a Company whose financial history is unavailable. */
export async function groupEditionPurchases(database: SqlDatabase, actor: WorkContext, id: string) {
  const row = await database.prepare("SELECT content_json FROM finance_group_report_editions WHERE id=? AND holding_id=? AND company_id=?")
    .bind(id,actor.holdingId,actor.companyId).first<{content_json:string}>();
  if (!row) return [];
  const content = JSON.parse(row.content_json) as {entities?:{companyId?:string}[]};
  if (!Array.isArray(content.entities) || content.entities.length<2 || content.entities.some(entity => typeof entity.companyId !== "string" || entity.companyId.length===0)) throw new ApiError(409,"GROUP_PURCHASE_SOURCE_INVALID","The retained edition's Company sources are invalid.");
  return groupCompanyPurchases(content.entities.map(entity=>entity.companyId!));
}
export async function registeredGroupPurchases(database: SqlDatabase, actor: WorkContext, entityIds?: readonly string[]) {
  const rows = (await database.prepare(`SELECT company_id FROM finance_group_entities WHERE holding_id=?${entityIds ? ` AND id IN (${entityIds.map(()=>"?").join(",") || "NULL"})` : " AND status='active'"}`)
    .bind(actor.holdingId,...entityIds??[]).all<{company_id:string}>()).results;
  return groupCompanyPurchases(rows.map(row=>row.company_id));
}

export function groupSourceSql(sql: string, predicates: readonly string[]) {
  const required=predicates.filter(predicate=>predicate!=="TRUE");if(!required.length)return sql;
  const where=sql.indexOf(" WHERE ");
  const position=where>=0?where:Math.min(...[" ORDER BY "," GROUP BY "," LIMIT "].map(clause=>{const index=sql.indexOf(clause);return index<0?sql.length:index;}));
  return `WITH finance_group_purchase_clock AS MATERIALIZED (SELECT ${packageClockSql} AS now_ms) `+sql.slice(0,position)+" WHERE "+required.map(predicate=>`(${predicate})`).join(" AND ")+(where>=0?" AND "+sql.slice(position+7):sql.slice(position));
}
export function groupEditionHistorySql(alias: string, history: (company: string)=>string) {
  if(history("source->>'companyId'")==="TRUE")return "TRUE";
  return `jsonb_typeof(${alias}.content_json::jsonb->'entities')='array' AND jsonb_array_length(${alias}.content_json::jsonb->'entities')>=2 AND NOT EXISTS(SELECT 1 FROM jsonb_array_elements(${alias}.content_json::jsonb->'entities') source WHERE jsonb_typeof(source->'companyId') IS DISTINCT FROM 'string' OR NOT (${history("source->>'companyId'")}))`;
}
export async function groupRecordPurchases(database:SqlDatabase,actor:WorkContext,kind:"ownership"|"acquisition"|"pair"|"intercompany",id:string){
 if(kind==="ownership"||kind==="acquisition"){
  const query=kind==="ownership"?"SELECT investor_entity_id,investee_entity_id FROM finance_ownership_interests WHERE id=? AND holding_id=? AND company_id=?":"SELECT o.investor_entity_id,o.investee_entity_id FROM finance_acquisitions a JOIN finance_ownership_interests o ON o.id=a.ownership_id WHERE a.id=? AND a.holding_id=? AND a.company_id=?";
  const row=await database.prepare(query).bind(id,actor.holdingId,actor.companyId).first<{investor_entity_id:string;investee_entity_id:string}>();return row?registeredGroupPurchases(database,actor,[row.investor_entity_id,row.investee_entity_id]):[];
 }
 const table=kind==="pair"?"finance_intercompany_document_pairs":"finance_intercompany_transactions";
 const row=await database.prepare(`SELECT source_company_id,target_company_id FROM ${table} WHERE id=? AND holding_id=?`).bind(id,actor.holdingId).first<{source_company_id:string;target_company_id:string}>();return groupCompanyPurchases(row?[row.source_company_id,row.target_company_id]:[]);
}
export async function checkGroupPurchases(database:SqlDatabase,purchases:Awaited<ReturnType<typeof groupEditionPurchases>>){for(const scope of purchases)await requireGroupCompanyHistory(database,scope.companyId);}

export function groupRecordHistorySql(kind:"ownership"|"acquisition"|"pair"|"intercompany",alias:string,history:(company:string)=>string){
 if(history("purchased_company")==="TRUE")return "TRUE";
 if(kind==="pair"||kind==="intercompany")return `(${history(alias+".source_company_id")}) AND (${history(alias+".target_company_id")})`;
 const investor=kind==="ownership"?`${alias}.investor_entity_id`:`(SELECT o.investor_entity_id FROM finance_ownership_interests o WHERE o.id=${alias}.ownership_id)`;
 return `(${history(`(SELECT e.company_id FROM finance_group_entities e WHERE e.id=${investor})`)}) AND (${history(`(SELECT e.company_id FROM finance_group_entities e WHERE e.id=${alias}.investee_entity_id)`)})`;
}
export function consolidationHistorySql(alias:string,history:(company:string)=>string){
 if(history("source->>'company_id'")==="TRUE")return "TRUE";
 return `jsonb_typeof(${alias}.source_snapshot_json::jsonb->'companies')='array' AND NOT EXISTS(SELECT 1 FROM jsonb_array_elements(${alias}.source_snapshot_json::jsonb->'companies') source WHERE jsonb_typeof(source->'companyId') IS DISTINCT FROM 'string' OR NOT (${history("source->>'companyId'")}))`;
}
