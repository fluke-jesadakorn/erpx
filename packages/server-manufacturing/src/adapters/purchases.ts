import "server-only";
import {randomUUID} from "node:crypto";
import {hasPermission} from "@erpx/feature-identity_access/lib/permissions";
import type {WorkContext} from "@erpx/server-identity_access/public/authorization";
import {assignmentReadSql} from "@erpx/server-identity_access/public/record-source-authority";
import {iamMutationGuards} from "@erpx/server-identity_access/public/iam-store";
import {workContextGuards,financialConstraintError} from '@erpx/server-workflow/public/command-guards';
import {manufacturingPackageEnforcementMode,readPurchasedAccesses,checkPurchasedCapability,companyPurchaseReadLocks,purchaseCommandBoundary,purchaseCommandFailure} from "@erpx/server-commercial/public/purchase-command-boundary";
import type {CapabilityPurpose} from "@erpx/server-commercial/public/rules/packages";
import {flushFinanceDeferredEffects} from "@erpx/server-finance/public/finance-transactions";
import {isTransactionalSqlDatabase,type SqlDatabase} from "@erpx/server-runtime/public/runtime/types";
import {holdingResources} from "../../../../packages/server-runtime/src/public/runtime";
import {ApiError} from "@erpx/server-runtime/public/http";
import {planningPurchaseScenarioRows,planningPurchaseRecommendationRows,planningPurchaseResourceRows,planningPurchaseSourceReadLocks} from "@erpx/server-planning/public/purchase-sources";
import {manufacturingWarehouseSources,manufacturingProductSources,manufacturingCommerceSourceReadLocks} from "@erpx/server-commerce/public/manufacturing-purchase-sources";
export type ManufacturingPurchaseControls={newWork:boolean;settleExisting:boolean;history:boolean;revision:string};
export type ManufacturingPurchaseTarget={kind:"specification"|"scenario"|"recommendation"|"order"|"resource"|"accounting";id:string};
export const manufacturingPurchaseSpecificationRows=`SELECT s.id,s.company_id,NULL::text branch_id,false advanced,
 jsonb_build_object('specification',to_jsonb(s)-'status'-'version'-'updated_at'-'reviewed_by','lines',COALESCE((SELECT jsonb_agg(to_jsonb(line) ORDER BY line.id) FROM manufacturing_specification_lines line WHERE line.specification_id=s.id),'[]'::jsonb),'products',COALESCE((SELECT jsonb_agg(to_jsonb(product) ORDER BY product.id) FROM (${manufacturingProductSources}) product WHERE product.company_id=s.company_id AND (product.id=s.output_product_id OR product.id IN(SELECT product_id FROM manufacturing_specification_lines WHERE specification_id=s.id))),'[]'::jsonb),'operations',COALESCE((SELECT jsonb_agg(to_jsonb(operation) ORDER BY operation.id) FROM manufacturing_specification_operations operation WHERE operation.specification_id=s.id),'[]'::jsonb)) witness
 FROM manufacturing_specifications s`;
export const manufacturingPurchaseOrderRows=`SELECT o.id,o.company_id,o.branch_id,o.warehouse_id,recommendation.advanced,
 jsonb_build_object('order',jsonb_build_array(o.id,o.company_id,o.branch_id,o.warehouse_id,o.recommendation_id,o.specification_id,o.specification_version,o.planned_quantity_milli,o.created_by,o.created_at),'recommendation',recommendation.witness,'specification',specification.witness,'warehouse',jsonb_build_array(warehouse.id,warehouse.company_id,warehouse.branch_id)) witness
 FROM manufacturing_orders o JOIN (${planningPurchaseRecommendationRows}) recommendation ON recommendation.id=o.recommendation_id AND recommendation.company_id=o.company_id AND recommendation.branch_id=o.branch_id AND recommendation.warehouse_id=o.warehouse_id AND recommendation.specification_id=o.specification_id AND recommendation.specification_version=o.specification_version
 JOIN (${manufacturingPurchaseSpecificationRows}) specification ON specification.id=o.specification_id AND specification.company_id=o.company_id
 JOIN (${manufacturingWarehouseSources}) warehouse ON warehouse.id=o.warehouse_id AND warehouse.company_id=o.company_id AND warehouse.branch_id=o.branch_id`;
const profileRows=`SELECT id,company_id,NULL::text branch_id,false advanced,to_jsonb(profile)-'status'-'version'-'updated_at'-'reviewed_by' witness FROM manufacturing_accounting_profiles profile`;
function planningRows(source:string){return `SELECT planning.*,jsonb_build_object('planning',planning.witness,'specification',specification.witness,'warehouse',jsonb_build_array(warehouse.id,warehouse.company_id,warehouse.branch_id)) source_witness FROM (${source}) planning JOIN (${manufacturingPurchaseSpecificationRows}) specification ON specification.id=planning.specification_id AND specification.company_id=planning.company_id JOIN (${manufacturingWarehouseSources}) warehouse ON warehouse.id=planning.warehouse_id AND warehouse.company_id=planning.company_id AND warehouse.branch_id=planning.branch_id`;}
function rows(kind:ManufacturingPurchaseTarget["kind"]){switch(kind){case "specification":return manufacturingPurchaseSpecificationRows;case "scenario":return planningRows(planningPurchaseScenarioRows);case "recommendation":return planningRows(planningPurchaseRecommendationRows);case "order":return manufacturingPurchaseOrderRows;case "resource":return planningPurchaseResourceRows;case "accounting":return profileRows;}}
export function manufacturingPurchaseSourceReadLocks(database:SqlDatabase,writer=false){return [...planningPurchaseSourceReadLocks(database,writer),...manufacturingCommerceSourceReadLocks(database),database.prepare(`LOCK TABLE manufacturing_specifications,manufacturing_specification_lines,manufacturing_specification_operations,manufacturing_orders,manufacturing_order_events,manufacturing_accounting_profiles,manufacturing_order_postings,manufacturing_cost_movements IN ${writer?"SHARE ROW EXCLUSIVE":"SHARE"} MODE`)];}
async function current(database:SqlDatabase,actor:WorkContext,key:string,permission:string,module?:"planning"|"manufacturing"){
 if(module&&!actor.modules.includes(module))throw new ApiError(403,"MANUFACTURING_ACCESS_CHANGED","Choose a current original production assignment.");
 for(const required of new Set([permission,...(module&&permission==="records.review"?["records.write"]:[])])){
 if(!hasPermission(actor,required))throw new ApiError(403,"MANUFACTURING_ACCESS_CHANGED","Choose a current original production assignment.");
 const row=await database.prepare(`SELECT ${assignmentReadSql("production_scope",required)} allowed FROM (SELECT ?::text company_id,?::text branch_id,?::text department_id) production_scope`).bind(actor.assignmentId,actor.userId,actor.role,actor.companyId,actor.branchId,actor.departmentId).first<{allowed:boolean}>();
 if(!row?.allowed)throw new ApiError(403,"MANUFACTURING_ACCESS_CHANGED","The original production assignment is no longer current.");}
 if(module){const selection=await database.prepare("SELECT (jsonb_array_length(modules_json::jsonb)=0 OR modules_json::jsonb @> jsonb_build_array(?::text)) allowed FROM assignments WHERE id=?").bind(module,actor.assignmentId).first<{allowed:boolean}>();if(!selection?.allowed)throw new ApiError(403,"MANUFACTURING_ACCESS_CHANGED","The current assignment no longer selects the original production module.");}
 await database.batch([...workContextGuards(database,actor,key),...iamMutationGuards(database,actor,key),database.prepare("DELETE FROM command_assertions WHERE command_id=?").bind(key)]);
}
async function source(database:SqlDatabase,actor:WorkContext,target:ManufacturingPurchaseTarget){const row=await database.prepare(`SELECT id,advanced,${target.kind==="scenario"||target.kind==="recommendation"?"source_witness":"witness"}::text witness FROM (${rows(target.kind)}) actual_production WHERE id=? AND company_id=? AND (branch_id IS NULL OR ?::text IS NULL OR branch_id=?)`).bind(target.id,actor.companyId,actor.branchId,actor.branchId).first<{id:string;advanced:boolean;witness:string}>();if(!row)throw new ApiError(404,"PRODUCTION_SOURCE_NOT_FOUND","The original production source is unavailable in this workspace.");return row;}
function base(kind:ManufacturingPurchaseTarget["kind"]){return kind==="specification"?["manufacturing.bom"]:kind==="accounting"?["manufacturing.costs","finance.production-costing"]:kind==="order"?["manufacturing.execution"]:["planning.basic"];}
export async function manufacturingSourceAccess(database:SqlDatabase,actor:WorkContext,input:{target?:ManufacturingPurchaseTarget;capabilities?:readonly string[];module?:"planning"|"manufacturing"}={}):Promise<ManufacturingPurchaseControls|undefined>{
 const mode=await manufacturingPackageEnforcementMode();if(mode==="disabled")return undefined;
 await current(database,actor,`production-read:${randomUUID()}`,"records.read",input.module??(input.target?.kind==="order"||input.target?.kind==="specification"?"manufacturing":"planning"));
 const actual=input.target?await source(database,actor,input.target):null,capabilities=[...new Set([...(input.capabilities??base(input.target?.kind??"order")),...(actual?.advanced?["planning.advanced-mrp"]:[])])],access=await readPurchasedAccesses(database,mode,actor.companyId,capabilities);
 return {newWork:capabilities.every(capability=>access[capability].newWork),settleExisting:capabilities.every(capability=>access[capability].settleExisting),history:capabilities.every(capability=>access[capability].history),revision:capabilities.map(capability=>access[capability].revision).join(":")};
}
export async function manufacturingPurchaseControls(actor:WorkContext,input:Parameters<typeof manufacturingSourceAccess>[2]={}){return manufacturingSourceAccess((await holdingResources(actor.holdingId)).write,actor,input);}
export async function requireManufacturingPurchasedRead(database:SqlDatabase,actor:WorkContext,input:Parameters<typeof manufacturingSourceAccess>[2]={}){const access=await manufacturingSourceAccess(database,actor,input);if(access&&!access.history)throw new ApiError(403,"PACKAGE_REQUIRED","The original production capability or retained history is required.");return access;}
async function currentPlanningSpecification(database:SqlDatabase,actor:WorkContext,target:ManufacturingPurchaseTarget){
 if(target.kind!=="scenario"&&target.kind!=="recommendation")return;
 const actual=await database.prepare(`SELECT 1 FROM (${rows(target.kind)}) source JOIN manufacturing_specifications spec ON spec.id=source.specification_id AND spec.company_id=source.company_id AND spec.version=source.specification_version AND spec.status='approved' WHERE source.id=? AND source.company_id=?`).bind(target.id,actor.companyId).first();
 if(!actual)throw new ApiError(409,"PRODUCTION_SOURCE_CHANGED","The original approved specification changed before production release.");
}
export async function withManufacturingPurchasedCommand<Result>(owner:SqlDatabase,actor:WorkContext,command:{key:string;operation:string;target:ManufacturingPurchaseTarget;purpose:CapabilityPurpose;permission:string;module?:"planning"|"manufacturing";capabilities?:readonly string[];existing?:boolean;sources?:readonly ManufacturingPurchaseTarget[];advanced?:boolean},run:(database:SqlDatabase)=>Promise<Result>){
 const mode=await manufacturingPackageEnforcementMode();if(mode==="disabled")return run(owner);if(!isTransactionalSqlDatabase(owner))throw new ApiError(503,"PRODUCTION_TRANSACTION_REQUIRED","Production requires its original owned Holding transaction.");
 try{return await owner.transaction(async database=>{
 const locks=companyPurchaseReadLocks(database,actor.companyId);await database.batch([locks[0],...manufacturingPurchaseSourceReadLocks(database,true),...locks.slice(1)]);const key=`production-purchase:${randomUUID()}`;await current(database,actor,key,command.permission,command.module);
 const replay=await database.prepare("SELECT 1 FROM command_receipts WHERE command_id=? AND operation=? AND record_id=? AND actor_id=? AND authority_json::jsonb->>'companyId'=?").bind(command.key,command.operation,command.target.id,actor.userId,actor.companyId).first(),purpose=replay?"history" as const:command.purpose,before=command.existing||replay?await source(database,actor,command.target):null;
 const sources=await Promise.all((command.sources??[]).map(async target=>({target,value:await source(database,actor,target)}))),advanced=before?.advanced||sources.some(item=>item.value.advanced)||command.advanced,
 capabilities=[...new Set([...(command.capabilities??base(command.target.kind)),...(advanced?["planning.advanced-mrp"]:[])])],scopes=capabilities.map(capability=>({companyId:actor.companyId,capability,purpose})),boundaries=scopes.map((scope,index)=>({key:`${key}:${index}`,boundary:purchaseCommandBoundary(database,mode,`${key}:${index}`,scope)!}));
 for(const scope of scopes)await checkPurchasedCapability(database,mode,scope);await database.batch(boundaries.flatMap(item=>item.boundary.before));const result=await run(database);await flushFinanceDeferredEffects(database);
 if(!replay&&purpose==="new-work")await currentPlanningSpecification(database,actor,command.target);
 const after=await source(database,actor,command.target);if(before&&after.witness!==before.witness||advanced&&!after.advanced&&command.target.kind!=="resource"&&command.target.kind!=="accounting")throw new ApiError(409,"PRODUCTION_SOURCE_CHANGED","The original production source changed before commit.");
 for(const item of sources){if((await source(database,actor,item.target)).witness!==item.value.witness)throw new ApiError(409,"PRODUCTION_SOURCE_CHANGED","The captured production source changed before commit.");}
 await current(database,actor,key,command.permission,command.module);const observed=await database.batch(boundaries.flatMap(item=>[...item.boundary.after,database.prepare("DELETE FROM command_assertions WHERE command_id=?").bind(item.key)]));boundaries[0]?.boundary.observe?.(observed);return result;
 });}catch(error){throw purchaseCommandFailure(error)??financialConstraintError(error)??error;}
}

/** Only actual native identities participate in shared source surfaces. */
export const manufacturingPurchaseRecordRows=`SELECT 'specification'::text kind,'manufacturing_spec'::text record_type,'manufacturing'::text module_id,'manufacturing.bom'::text capability,id,company_id,branch_id,advanced,witness FROM (${manufacturingPurchaseSpecificationRows}) spec
 UNION ALL SELECT 'order','manufacturing_order','manufacturing','manufacturing.execution',id,company_id,branch_id,advanced,witness FROM (${manufacturingPurchaseOrderRows}) production
 UNION ALL SELECT 'scenario','planning_scenario','planning','planning.basic',id,company_id,branch_id,advanced,source_witness FROM (${planningRows(planningPurchaseScenarioRows)}) scenario
 UNION ALL SELECT 'recommendation','planning_recommendation','planning','planning.basic',id,company_id,branch_id,advanced,source_witness FROM (${planningRows(planningPurchaseRecommendationRows)}) recommendation`;

/** One current Company purchase snapshot and one native identity query serve an
 * entire owner landing/queue. The number of IAM/grant reads never grows with
 * the number of production rows. No source witness leaves this reader. */
export async function manufacturingWorkspacePurchases(database:SqlDatabase,actor:WorkContext,module:"planning"|"manufacturing"){
 const mode=await manufacturingPackageEnforcementMode();if(mode==="disabled")return undefined;
 await current(database,actor,`production-workspace-read:${randomUUID()}`,"records.read",module);
 const access=await readPurchasedAccesses(database,mode,actor.companyId,["manufacturing.bom","manufacturing.execution","planning.basic","planning.advanced-mrp"]);
 const controls=(capabilities:readonly string[]):ManufacturingPurchaseControls=>({newWork:capabilities.every(capability=>access[capability].newWork),settleExisting:capabilities.every(capability=>access[capability].settleExisting),history:capabilities.every(capability=>access[capability].history),revision:access[capabilities[0]].revision});
 const purchase=controls([module==="planning"?"planning.basic":"manufacturing.execution"]),advancedPurchase=controls(["planning.basic","planning.advanced-mrp"]),recordPurchases:Record<string,ManufacturingPurchaseControls>={};
 if(purchase.history){const native=await database.prepare(`SELECT id,capability,advanced FROM (${manufacturingPurchaseRecordRows}) production_workspace_source WHERE company_id=? AND (branch_id IS NULL OR ?::text IS NULL OR branch_id=?)`).bind(actor.companyId,actor.branchId,actor.branchId).all<{id:string;capability:string;advanced:boolean}>();for(const row of native.results)recordPurchases[row.id]=controls([row.capability,...(row.advanced?["planning.advanced-mrp"]:[])]);}
 return {purchase,advancedPurchase,recordPurchases};
}
