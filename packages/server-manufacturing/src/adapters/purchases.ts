import "server-only";
import {randomUUID} from "node:crypto";
import {hasPermission} from "@erpx/feature-identity_access/lib/permissions";
import type {WorkContext} from "@erpx/server-identity_access/public/authorization";
import {assignmentReadSql} from "@erpx/server-identity_access/public/record-source-authority";
import {iamMutationGuards} from "@erpx/server-identity_access/public/iam-store";
import {workContextGuards,financialConstraintError} from '@erpx/server-workflow/public/command-guards';
import {clearWorkflowCommandAssertions} from "@erpx/server-workflow/public/command-assertion-effects";
import {isNativeWorkflowWrite} from "@erpx/server-workflow/public/native-workflow-evidence";
import {manufacturingPackageEnforcementMode,readPurchasedAccesses,checkPurchasedCapability,companyPurchaseReadLocks,purchaseCommandBoundary,purchaseCommandFailure} from "@erpx/server-commercial/public/purchase-command-boundary";
import type {CapabilityPurpose} from "@erpx/server-commercial/public/rules/packages";
import {packageClockSql} from "@erpx/server-commercial/public/purchased-capability-sql";
import {flushFinanceDeferredEffects} from "@erpx/server-finance/public/finance-transactions";
import {isTransactionalSqlDatabase,type SqlDatabase,type SqlResult,type SqlStatement} from "@erpx/server-runtime/public/runtime/types";
import {holdingResources} from "../../../../packages/server-runtime/src/public/runtime";
import {ApiError} from "@erpx/server-runtime/public/http";
import {planningPurchaseScenarioRows,planningPurchaseRecommendationRows,planningPurchaseResourceRows,planningPurchaseSourceReadLocks} from "@erpx/server-planning/public/purchase-sources";
import {manufacturingWarehouseSources,manufacturingProductSources,manufacturingCommerceSourceReadLocks} from "@erpx/server-commerce/public/manufacturing-purchase-sources";
import {manufacturingOperationAggSql,manufacturingSpecificationLineAggSql,manufacturingSpecificationProductsAggSql,manufacturingActorScopeRows} from "./manufacturing-read-sql-fragments";
import {manufacturingPurchaseRecordSelect} from "./manufacturing-read-tail-r16-sql";
import {manufacturingPurchaseRecordBranchSelect} from "./manufacturing-read-tail-r18-sql";
import {manufacturingAccountingProfileSelect} from "./manufacturing-read-tail-r19-sql";
import {manufacturingAllowedSelect} from "./manufacturing-read-tail-r31-sql";
export type ManufacturingPurchaseControls={newWork:boolean;settleExisting:boolean;history:boolean;revision:string};
/** Owner-local fixed read: one command receipt on the caller transaction. This
 * idempotency/replay witness is identical in the purchased-command and
 * accounting-profile command paths; a single definition keeps both atomic
 * guards from drifting and removes the redundant TypeScript SQL. It stays a
 * live TypeScript read on the same transaction and snapshot, and is deliberately
 * outside the RD_* read-plan registry (an atomic witness, not a projection). */
export const commandReceiptSelect = "SELECT 1 FROM command_receipts WHERE command_id=? AND operation=? AND record_id=? AND actor_id=? AND authority_json::jsonb->>'companyId'=?";
/** Shared text-only clock CTE: one definition for every purchased manufacturing
 * read that pins its source/purchase predicates to a stable now_ms snapshot. */
export const manufacturingClockCte=(name:string)=>`${name} AS MATERIALIZED (SELECT ${packageClockSql} now_ms)`;
/** Shared text-only scoped source rows: one definition for the department-tagged
 * purchase-source wrapper used by workspace records, metrics and attachments. */
export const manufacturingPurchaseScopedRows=(alias:string)=>`(SELECT source.*,?::text department_id FROM (${manufacturingPurchaseRecordRows}) source) ${alias}`;
/** Transient guard rows are keyed by the command that staged them. Every
 * purchase/attachment/posting boundary cleans them in the same before/after
 * batch, so the statement lives in one place instead of being copied. The
 * cleanup is a local owner statement on the caller transaction, never an HTTP
 * plan round-trip inside the atomic unit. */
export function clearCommandAssertions(database:SqlDatabase,key:string):SqlStatement{return clearWorkflowCommandAssertions(database,key);}
/** Execute one owner-unit effect list in order on the caller transaction.
 * Native assertion markers resolve through the typed Rust plan on this same
 * connection (never HTTP as an atomic-guard substitute), runnable owner
 * effects run in place, and only real statements share one batch. The fixed
 * lock order never reverses: receipt replay/lock, doc row lock, stock/lot,
 * quality hold, finance journal, outbox. */
export async function runOwnerUnitEffects(database:SqlDatabase,companyId:string,key:string,effects:readonly SqlStatement[]):Promise<SqlResult[]>{
 const observed:SqlResult[]=[];
 let pending:SqlStatement[]=[];
 const flush=async()=>{if(!pending.length)return;const batch=pending;pending=[];observed.push(...await database.batch(batch));};
 for(const effect of effects){
 if(isNativeWorkflowWrite(effect)){await flush();await effect.withContext({db:database,companyId,key,now:Date.now()}).run();continue;}
 const candidate=effect as SqlStatement&{sql?:unknown};
 if(typeof candidate.run==="function"&&typeof candidate.sql!=="string"){await flush();observed.push(await candidate.run());continue;}
 pending.push(effect);
 }
 await flush();
 return observed;
}
export type ManufacturingPurchaseTarget={kind:"specification"|"scenario"|"recommendation"|"order"|"resource"|"accounting";id:string};
export const manufacturingPurchaseSpecificationRows=`SELECT s.id,s.company_id,NULL::text branch_id,false advanced,
 jsonb_build_object('specification',to_jsonb(s)-'status'-'version'-'updated_at'-'reviewed_by','lines',COALESCE(${manufacturingSpecificationLineAggSql("s")},'[]'::jsonb),'products',COALESCE(${manufacturingSpecificationProductsAggSql("s",manufacturingProductSources)},'[]'::jsonb),'operations',COALESCE(${manufacturingOperationAggSql("manufacturing_specification_operations operation","WHERE operation.specification_id=s.id")},'[]'::jsonb)) witness
 FROM manufacturing_specifications s`;
export const manufacturingPurchaseOrderRows=`SELECT o.id,o.company_id,o.branch_id,o.warehouse_id,recommendation.advanced,
 jsonb_build_object('order',jsonb_build_array(o.id,o.company_id,o.branch_id,o.warehouse_id,o.recommendation_id,o.specification_id,o.specification_version,o.planned_quantity_milli,o.created_by,o.created_at),'recommendation',recommendation.witness,'specification',specification.witness,'warehouse',jsonb_build_array(warehouse.id,warehouse.company_id,warehouse.branch_id)) witness
 FROM manufacturing_orders o JOIN (${planningPurchaseRecommendationRows}) recommendation ON recommendation.id=o.recommendation_id AND recommendation.company_id=o.company_id AND recommendation.branch_id=o.branch_id AND recommendation.warehouse_id=o.warehouse_id AND recommendation.specification_id=o.specification_id AND recommendation.specification_version=o.specification_version
 JOIN (${manufacturingPurchaseSpecificationRows}) specification ON specification.id=o.specification_id AND specification.company_id=o.company_id
 JOIN (${manufacturingWarehouseSources}) warehouse ON warehouse.id=o.warehouse_id AND warehouse.company_id=o.company_id AND warehouse.branch_id=o.branch_id`;
const profileRows=manufacturingAccountingProfileSelect("id,company_id,NULL::text branch_id,false advanced,to_jsonb(profile)-'status'-'version'-'updated_at'-'reviewed_by' witness","profile");
function planningRows(source:string){return `SELECT planning.*,jsonb_build_object('planning',planning.witness,'specification',specification.witness,'warehouse',jsonb_build_array(warehouse.id,warehouse.company_id,warehouse.branch_id)) source_witness FROM (${source}) planning JOIN (${manufacturingPurchaseSpecificationRows}) specification ON specification.id=planning.specification_id AND specification.company_id=planning.company_id JOIN (${manufacturingWarehouseSources}) warehouse ON warehouse.id=planning.warehouse_id AND warehouse.company_id=planning.company_id AND warehouse.branch_id=planning.branch_id`;}
function rows(kind:ManufacturingPurchaseTarget["kind"]){switch(kind){case "specification":return manufacturingPurchaseSpecificationRows;case "scenario":return planningRows(planningPurchaseScenarioRows);case "recommendation":return planningRows(planningPurchaseRecommendationRows);case "order":return manufacturingPurchaseOrderRows;case "resource":return planningPurchaseResourceRows;case "accounting":return profileRows;}}
/** Owner source snapshot for one Manufacturing purchase check. Writer commands
 * hold ROW SHARE (never SHARE/SHARE ROW EXCLUSIVE): the same unit flushes
 * native owner writes (planning recommendation review + manufacturing order
 * create) on a separate binary connection, and a self-conflicting table lock
 * would block the native flush behind this transaction until it times out.
 * Row locks, serializable snapshots and the before/after witness assertions
 * still serialize concurrent commands. Planning tables are locked here with
 * ROW SHARE directly (rather than planningPurchaseSourceReadLocks(writer))
 * because the planning helper only offers SHARE/SHARE ROW EXCLUSIVE, both of
 * which conflict with the native ROW EXCLUSIVE and would reintroduce the
 * 10s MANUFACTURING_OWNER_WRITE_UNAVAILABLE timeout. */
export function manufacturingPurchaseSourceReadLocks(database:SqlDatabase,writer=false){if(!writer)return [...planningPurchaseSourceReadLocks(database,false),...manufacturingCommerceSourceReadLocks(database),database.prepare(`LOCK TABLE manufacturing_specifications,manufacturing_specification_lines,manufacturing_specification_operations,manufacturing_orders,manufacturing_order_events,manufacturing_accounting_profiles,manufacturing_order_postings,manufacturing_cost_movements IN SHARE MODE`)];return [database.prepare(`LOCK TABLE planning_scenarios,planning_recommendations,planning_scenario_events,planning_resource_allocations,production_resources,production_resource_calendar IN ROW SHARE MODE`),...manufacturingCommerceSourceReadLocks(database),database.prepare(`LOCK TABLE manufacturing_specifications,manufacturing_specification_lines,manufacturing_specification_operations,manufacturing_orders,manufacturing_order_events,manufacturing_accounting_profiles,manufacturing_order_postings,manufacturing_cost_movements IN ROW SHARE MODE`)];}
async function current(database:SqlDatabase,actor:WorkContext,key:string,permission:string,module?:"planning"|"manufacturing"){
 if(module&&!actor.modules.includes(module))throw new ApiError(403,"MANUFACTURING_ACCESS_CHANGED","Choose a current original production assignment.");
 for(const required of new Set([permission,...(module&&permission==="records.review"?["records.write"]:[])])){
 if(!hasPermission(actor,required))throw new ApiError(403,"MANUFACTURING_ACCESS_CHANGED","Choose a current original production assignment.");
 const row=await database.prepare(manufacturingAllowedSelect(assignmentReadSql("production_scope",required),`FROM ${manufacturingActorScopeRows()} production_scope`)).bind(actor.assignmentId,actor.userId,actor.role,actor.companyId,actor.branchId,actor.departmentId).first<{allowed:boolean}>();
 if(!row?.allowed)throw new ApiError(403,"MANUFACTURING_ACCESS_CHANGED","The original production assignment is no longer current.");}
 if(module){const selection=await database.prepare(manufacturingAllowedSelect("(jsonb_array_length(modules_json::jsonb)=0 OR modules_json::jsonb @> jsonb_build_array(?::text))","FROM assignments WHERE id=?")).bind(module,actor.assignmentId).first<{allowed:boolean}>();if(!selection?.allowed)throw new ApiError(403,"MANUFACTURING_ACCESS_CHANGED","The current assignment no longer selects the original production module.");}
 await runOwnerUnitEffects(database,actor.companyId,key,[...workContextGuards(database,actor,key),...iamMutationGuards(database,actor,key),clearCommandAssertions(database,key)]);
}
async function source(database:SqlDatabase,actor:WorkContext,target:ManufacturingPurchaseTarget){const row=await database.prepare(`SELECT id,advanced,${target.kind==="scenario"||target.kind==="recommendation"?"source_witness":"witness"}::text witness FROM (${rows(target.kind)}) actual_production WHERE id=? AND company_id=? AND (branch_id IS NULL OR ?::text IS NULL OR branch_id=?)`).bind(target.id,actor.companyId,actor.branchId,actor.branchId).first<{id:string;advanced:boolean;witness:string}>();if(!row)throw new ApiError(404,"PRODUCTION_SOURCE_NOT_FOUND","The original production source is unavailable in this workspace.");return row;}
function base(kind:ManufacturingPurchaseTarget["kind"]){return kind==="specification"?["manufacturing.bom"]:kind==="accounting"?["manufacturing.costs","finance.production-costing"]:kind==="order"?["manufacturing.execution"]:["planning.basic"];}
export async function manufacturingSourceAccess(database:SqlDatabase,actor:WorkContext,input:{target?:ManufacturingPurchaseTarget;capabilities?:readonly string[];module?:"planning"|"manufacturing"}={}):Promise<ManufacturingPurchaseControls|undefined>{
 const mode=await manufacturingPackageEnforcementMode();if(mode==="disabled")return undefined;
 await current(database,actor,`production-read:${randomUUID()}`,"records.read",input.module??(input.target?.kind==="order"||input.target?.kind==="specification"?"manufacturing":"planning"));
 // Read paths only need the source witness to decide the advanced-MRP scope.
 // A file bind bumps the owner record version inside its command transaction;
 // the derived source view can therefore drop the row for one freshness read
 // while the command guard remains authoritative. Never turn that transient
 // view miss into a 404 on a purchase read: fall back to the base capabilities
 // and let readPurchasedAccesses fail closed on the actual package grants. The
 // command path (withManufacturingPurchasedCommand) keeps source() strict.
 let actual:Awaited<ReturnType<typeof source>>|null=null;
 if(input.target){try{actual=await source(database,actor,input.target);}catch(error){if(!(error instanceof ApiError&&error.code==="PRODUCTION_SOURCE_NOT_FOUND"))throw error;}}
 const capabilities=[...new Set([...(input.capabilities??base(input.target?.kind??"order")),...(actual?.advanced?["planning.advanced-mrp"]:[])])],access=await readPurchasedAccesses(database,mode,actor.companyId,capabilities);
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
 const replay=await database.prepare(commandReceiptSelect).bind(command.key,command.operation,command.target.id,actor.userId,actor.companyId).first(),purpose=replay?"history" as const:command.purpose,before=command.existing||replay?await source(database,actor,command.target):null;
 const sources=await Promise.all((command.sources??[]).map(async target=>({target,value:await source(database,actor,target)}))),advanced=before?.advanced||sources.some(item=>item.value.advanced)||command.advanced,
 capabilities=[...new Set([...(command.capabilities??base(command.target.kind)),...(advanced?["planning.advanced-mrp"]:[])])],scopes=capabilities.map(capability=>({companyId:actor.companyId,capability,purpose})),boundaries=scopes.map((scope,index)=>({key:`${key}:${index}`,boundary:purchaseCommandBoundary(database,mode,`${key}:${index}`,scope)!}));
 for(const scope of scopes)await checkPurchasedCapability(database,mode,scope);await runOwnerUnitEffects(database,actor.companyId,key,boundaries.flatMap(item=>item.boundary.before));const result=await run(database);await flushFinanceDeferredEffects(database);
 if(!replay&&purpose==="new-work")await currentPlanningSpecification(database,actor,command.target);
 const after=await source(database,actor,command.target);if(before&&after.witness!==before.witness||advanced&&!after.advanced&&command.target.kind!=="resource"&&command.target.kind!=="accounting")throw new ApiError(409,"PRODUCTION_SOURCE_CHANGED","The original production source changed before commit.");
 for(const item of sources){if((await source(database,actor,item.target)).witness!==item.value.witness)throw new ApiError(409,"PRODUCTION_SOURCE_CHANGED","The captured production source changed before commit.");}
 await current(database,actor,key,command.permission,command.module);const observed=await runOwnerUnitEffects(database,actor.companyId,key,boundaries.flatMap(item=>[...item.boundary.after,clearCommandAssertions(database,item.key)]));boundaries[0]?.boundary.observe?.(observed);return result;
 });}catch(error){throw purchaseCommandFailure(error)??financialConstraintError(error)??error;}
}

/** Only actual native identities participate in shared source surfaces. */
export const manufacturingPurchaseRecordRows=`${manufacturingPurchaseRecordBranchSelect("'specification'::text kind","'manufacturing_spec'::text record_type","'manufacturing'::text module_id","'manufacturing.bom'::text capability","witness",manufacturingPurchaseSpecificationRows,"spec")}
 UNION ALL ${manufacturingPurchaseRecordBranchSelect("'order'","'manufacturing_order'","'manufacturing'","'manufacturing.execution'","witness",manufacturingPurchaseOrderRows,"production")}
 UNION ALL ${manufacturingPurchaseRecordBranchSelect("'scenario'","'planning_scenario'","'planning'","'planning.basic'","source_witness",planningRows(planningPurchaseScenarioRows),"scenario")}
 UNION ALL ${manufacturingPurchaseRecordBranchSelect("'recommendation'","'planning_recommendation'","'planning'","'planning.basic'","source_witness",planningRows(planningPurchaseRecommendationRows),"recommendation")}`;

/** One current Company purchase snapshot and one native identity query serve an
 * entire owner landing/queue. The number of IAM/grant reads never grows with
 * the number of production rows. No source witness leaves this reader. */
export async function manufacturingWorkspacePurchases(database:SqlDatabase,actor:WorkContext,module:"planning"|"manufacturing"){
 const mode=await manufacturingPackageEnforcementMode();if(mode==="disabled")return undefined;
 await current(database,actor,`production-workspace-read:${randomUUID()}`,"records.read",module);
 const access=await readPurchasedAccesses(database,mode,actor.companyId,["manufacturing.bom","manufacturing.execution","planning.basic","planning.advanced-mrp"]);
 const controls=(capabilities:readonly string[]):ManufacturingPurchaseControls=>({newWork:capabilities.every(capability=>access[capability].newWork),settleExisting:capabilities.every(capability=>access[capability].settleExisting),history:capabilities.every(capability=>access[capability].history),revision:access[capabilities[0]].revision});
 const purchase=controls([module==="planning"?"planning.basic":"manufacturing.execution"]),advancedPurchase=controls(["planning.basic","planning.advanced-mrp"]),recordPurchases:Record<string,ManufacturingPurchaseControls>={};
 if(purchase.history){const native=await database.prepare(manufacturingPurchaseRecordSelect("id,capability,advanced",manufacturingPurchaseRecordRows,"production_workspace_source","company_id=? AND (branch_id IS NULL OR ?::text IS NULL OR branch_id=?)")).bind(actor.companyId,actor.branchId,actor.branchId).all<{id:string;capability:string;advanced:boolean}>();for(const row of native.results)recordPurchases[row.id]=controls([row.capability,...(row.advanced?["planning.advanced-mrp"]:[])]);}
 return {purchase,advancedPurchase,recordPurchases};
}
