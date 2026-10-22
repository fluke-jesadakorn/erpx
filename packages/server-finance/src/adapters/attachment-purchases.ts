import {manufacturingFinanceSourceReadLocks} from "@erpx/server-manufacturing/public/finance-record-sources";
import {fieldServiceFinanceSourceReadLocks} from "@erpx/server-field_service/public/finance-record-sources";
import "server-only";
import type {SqlDatabase} from "@erpx/server-runtime/public/runtime/types";
import type {WorkContext} from "@erpx/server-identity_access/public/authorization";
import type {AttachmentOperation} from "@erpx/server-files/public/attachments/types";
import type {CapabilityPurpose} from "@erpx/server-commercial/public/rules/packages";
import {financePackageEnforcementMode,readPurchasedAccesses,checkPurchasedCapability,purchaseCommandBoundary,observePurchaseResults,companyPurchaseReadLocks} from "@erpx/server-commercial/public/purchase-command-boundary";
import {commerceFinanceSourceReadLocks} from "@erpx/server-commerce/public/finance-record-sources";
import {maintenanceFinanceSourceReadLocks} from "@erpx/server-maintenance/public/finance-record-sources";
import {inventoryFinanceSourceReadLocks} from "@erpx/server-inventory/public/finance-record-sources";
import {assignmentReadSql} from "@erpx/server-identity_access/public/record-source-authority";
import {iamMutationGuards} from "@erpx/server-identity_access/public/iam-store";
import {assertion} from "@erpx/server-workflow/public/command-assertions";
import {workContextGuards} from "@erpx/server-workflow/public/commands";
import {requireFinanceCurrentAuthority,financeReferenceCapabilities} from "@erpx/server-finance/adapters/purchase-boundary";
import {financeDeferredEffectStatements} from "@erpx/server-finance/adapters/finance-transactions";
import {groupEditionPurchases,groupRecordPurchases} from "@erpx/server-finance/adapters/group-purchase-sources";
import {initialPhysicalStockJournalMarked,requireFinanceDocumentHistory,financeDocumentSourceGuards,financeSourceDocumentReadLocks,type FinanceDocumentKind} from "@erpx/server-finance/adapters/document-purchases";

const advanced=new Set(["finance_book_adjustment","finance_instrument","finance_instrument_run","finance_group_report","finance_ownership_interest","finance_acquisition","finance_intercompany_pair","finance_lease","finance_loan"]);
const banking=new Set(["finance_bank_match_group","finance_bank_statement","finance_bank_transaction","finance_payment","finance_refund_obligation","finance_supplier_batch","invoice_payment"]);
const accounting=new Set(["finance_billing_correction","finance_business_bill","finance_plan_edition","finance_book_rule","finance_collection_case","finance_business_contract","finance_recognition_run","finance_asset_policy","finance_asset_policy_run","finance_journal","finance_accrual_schedule","finance_asset","finance_tax_identity","finance_revenue_contract","supplier_bill"]);
export type FinanceAttachmentPurchase={companyId:string;capability:string;purpose:CapabilityPurpose;actor?:WorkContext;document?:{kind:FinanceDocumentKind;id:string;commerceProof?:"ordinary"|"retained"};supplierBatchWitness?:string;strictPurchase?:true};
const supplierBatchFileImage=`SELECT jsonb_build_object('batch',to_jsonb(b)-'version'-'updated_at','instructions',(SELECT jsonb_agg(to_jsonb(i)-'version'-'updated_at' ORDER BY i.id) FROM finance_supplier_instructions i WHERE i.batch_id=b.id))::text image FROM finance_supplier_batches b WHERE b.id=? AND b.company_id=?`;
/** Files invokes these at the start of its named atomic unit, before any
 * business snapshot. A preflight on the pool is never its source lease. */
export async function financeAttachmentSourceReadLocks(database:SqlDatabase,scopes:readonly FinanceAttachmentPurchase[]){
 if(await financePackageEnforcementMode()==="disabled"&&!scopes.some(scope=>scope.strictPurchase))return [];
 const included=scopes.filter(scope=>scope.document&&["finance.sales-posting","finance.inventory-valuation","assets.parts-costs","finance.production-costing","finance.service-invoice","finance.pos-posting"].includes(scope.capability));
 if(!included.length)return [];
 if(included.some(scope=>scope.capability==="finance.service-invoice"&&scope.document?.kind==="payment"))return [...new Set(included.map(scope=>scope.companyId))].sort().flatMap(company=>financeSourceDocumentReadLocks(database,company,true));
 const companies=[...new Set(included.map(scope=>scope.companyId))].sort().map(company=>companyPurchaseReadLocks(database,company));
 return [...(included.some(scope=>scope.strictPurchase&&['finance.production-costing','finance.pos-posting','finance.service-invoice'].includes(scope.capability))?[database.prepare('LOCK TABLE company_package_grants IN SHARE ROW EXCLUSIVE MODE'),database.prepare('LOCK TABLE command_receipts,inventory_counts,inventory_count_lines,inventory_count_lot_lines,inventory_count_decisions,inventory_adjustment_moves IN SHARE ROW EXCLUSIVE MODE')]:[]),...companies.map(locks=>locks[0]),...(included.some(scope=>scope.capability==="finance.service-invoice")?fieldServiceFinanceSourceReadLocks(database):[]),...commerceFinanceSourceReadLocks(database),...(included.some(scope=>scope.capability==="finance.production-costing")?manufacturingFinanceSourceReadLocks(database):[]),...(included.some(scope=>scope.capability==="assets.parts-costs")?maintenanceFinanceSourceReadLocks(database):[]),...(included.some(scope=>scope.capability==="finance.inventory-valuation"||["finance.sales-posting","finance.production-costing","finance.pos-posting","finance.service-invoice"].includes(scope.capability)&&scope.document?.kind==="journal")?inventoryFinanceSourceReadLocks(database):[]),...companies.flatMap(locks=>locks.slice(1))];
}
export function financeAttachmentCapability(targetType:string) {
  return advanced.has(targetType)?"finance.advanced":banking.has(targetType)?"finance.banking":targetType==="finance_period"?"finance.close":targetType==="finance_collection_case"?"finance.receivables":accounting.has(targetType)?"finance.accounting":null;
}
/** Original Files policy has already identified the actual retained owner.
 * Extra Company scopes never impersonate an assignment in those Companies. */
export async function financeAttachmentPurchaseScopes(database:SqlDatabase,actor:WorkContext,targetType:string,targetId:string,purpose:CapabilityPurpose="settle-existing"):Promise<FinanceAttachmentPurchase[]> {
  let capability=financeAttachmentCapability(targetType);
  const initial=targetType==='finance_journal'&&await initialPhysicalStockJournalMarked(database,actor.companyId,targetId);
  if(!capability||await financePackageEnforcementMode()==="disabled"&&!initial)return [];
  if(targetType==="finance_journal"||targetType==="finance_payment"||targetType==="finance_refund_obligation"||targetType==="finance_supplier_batch"){
    const kind=targetType==="finance_journal"?"journal" as const:targetType==="finance_refund_obligation"?"refund" as const:targetType==="finance_supplier_batch"?"supplier-batch" as const:"payment" as const;
    const access=await requireFinanceDocumentHistory(database,actor,kind,targetId,purpose);
    const supplierBatchWitness=kind==="supplier-batch"?await database.prepare(supplierBatchFileImage).bind(targetId,actor.companyId).first<string>("image"):undefined;
    return [{companyId:actor.companyId,capability:access.capability,purpose,actor,...(initial?{strictPurchase:true as const}:{}),document:{kind,id:targetId,commerceProof:access.commerceProof},...(supplierBatchWitness?{supplierBatchWitness}:{})},...access.additionalCapabilities.map(capability=>({companyId:actor.companyId,capability,purpose,actor,...(initial?{strictPurchase:true as const}:{})}))];
  }
  if(targetType==="supplier_bill"||targetType==="invoice_payment"){
    const purchased=await readPurchasedAccesses(database,await financePackageEnforcementMode(),actor.companyId,[capability]);
    if(purchased[capability].codes[purpose]!=="GRANTED"){
      const access=await requireFinanceDocumentHistory(database,actor,"invoice",targetId,purpose);
      if(["finance.sales-posting","finance.inventory-valuation","finance.service-invoice"].includes(access.capability))return [{companyId:actor.companyId,capability:access.capability,purpose,actor,document:{kind:"invoice",id:targetId,commerceProof:access.commerceProof}},...access.additionalCapabilities.map(capability=>({companyId:actor.companyId,capability,purpose,actor}))];
    }
  }
  if(targetType==="finance_tax_identity"){
    const access=await readPurchasedAccesses(database,await financePackageEnforcementMode(),actor.companyId,financeReferenceCapabilities());
    capability=Object.keys(access).find(key=>access[key].codes[purpose]==="GRANTED")??capability;
  }
  if(targetType==="finance_asset"){
    // Files has already resolved the actual same-Company Finance asset under
    // its owner policy; this cannot turn an arbitrary GL target into Assets.
    const access=await readPurchasedAccesses(database,await financePackageEnforcementMode(),actor.companyId,["finance.accounting","finance.asset-recognition","finance.asset-disposal"]);
    capability=Object.keys(access).find(key=>access[key].codes[purpose]==="GRANTED")??capability;
  }
  const source=targetType==="finance_group_report"?await groupEditionPurchases(database,actor,targetId):targetType==="finance_ownership_interest"?await groupRecordPurchases(database,actor,"ownership",targetId):targetType==="finance_acquisition"?await groupRecordPurchases(database,actor,"acquisition",targetId):targetType==="finance_intercompany_pair"?await groupRecordPurchases(database,actor,"pair",targetId):[];
  return [{companyId:actor.companyId,capability,purpose,actor},...source];
}
export async function authorizeFinanceAttachmentPurchase(database:SqlDatabase,actor:WorkContext,targetType:string,targetId:string,operation:AttachmentOperation) {
  const scopes=await financeAttachmentPurchaseScopes(database,actor,targetType,targetId,operation==="read"?"history":"settle-existing");
  if(!scopes.length)return;
  const mode=await financePackageEnforcementMode();
  await requireFinanceCurrentAuthority(database,actor,`finance-file-read:${crypto.randomUUID()}`);
  for(const scope of scopes)await checkPurchasedCapability(database,scope.strictPurchase?"enforced":mode,{companyId:scope.companyId,capability:scope.capability,purpose:operation==="read"?"history":scope.purpose});
}
/** Raw file batches and normal commands retain their owner transaction, version,
 * scanner/Legal policy, receipt, audit and final authority/purchase checks. */
export async function financeAttachmentCommandBoundary(database:SqlDatabase,scopes:readonly FinanceAttachmentPurchase[]) {
  if(!scopes.length)return undefined;const mode=await financePackageEnforcementMode();if(mode==="disabled"&&!scopes.some(scope=>scope.strictPurchase))return undefined;
  const unique=[...new Map(scopes.map(scope=>[`${scope.companyId}:${scope.capability}:${scope.purpose}`,scope])).values()].sort((a,b)=>a.companyId.localeCompare(b.companyId)||a.capability.localeCompare(b.capability));
  const boundaries=unique.map(scope=>{const key=`finance-file-purchase:${crypto.randomUUID()}`;return {key,boundary:purchaseCommandBoundary(database,scope.strictPurchase?"enforced":mode,key,scope)!};});
  const actors=[...new Map(scopes.flatMap(scope=>scope.actor?[[scope.actor.assignmentId,scope.actor] as const]:[])).values()];
  const authority=(phase:string)=>actors.flatMap(actor=>{
    const key=`finance-file-${phase}:${crypto.randomUUID()}`;
    return [database.prepare("SELECT id FROM assignments WHERE id=? AND company_id=? FOR SHARE").bind(actor.assignmentId,actor.companyId),...workContextGuards(database,actor,key),...iamMutationGuards(database,actor,key),
      assertion(database,key,"finance-file-assignment",`(SELECT ${assignmentReadSql("finance_file_scope","finance.read")} FROM (SELECT ?::text company_id,?::text branch_id,?::text department_id) finance_file_scope)`,[actor.assignmentId,actor.userId,actor.role,actor.companyId,actor.branchId,actor.departmentId]),database.prepare("DELETE FROM command_assertions WHERE command_id=?").bind(key)];
  });
  const source=(phase:string)=>scopes.flatMap(scope=>{
    if(!scope.document||!scope.actor)return [];
    const key=`finance-file-source-${phase}:${crypto.randomUUID()}`;
    const guards=financeDocumentSourceGuards(database,scope.actor,scope.document.kind,scope.document.id,scope.capability,key,scope.document.commerceProof);
    const batch=scope.supplierBatchWitness?[assertion(database,key,"finance-supplier-batch-owner",`(${supplierBatchFileImage})=?`,[scope.document.id,scope.companyId,scope.supplierBatchWitness])]:[];
    return guards.length||batch.length?[...guards,...batch,database.prepare("DELETE FROM command_assertions WHERE command_id=?").bind(key)]:[];
  });
  return {before:[...boundaries.flatMap(item=>item.boundary.before),...authority("before"),...source("before")],after:[...financeDeferredEffectStatements(database),...authority("after"),...source("after"),...boundaries.flatMap(item=>item.boundary.after),...boundaries.map(item=>database.prepare("DELETE FROM command_assertions WHERE command_id=?").bind(item.key))],observe:mode==="shadow"?observePurchaseResults:undefined};
}
