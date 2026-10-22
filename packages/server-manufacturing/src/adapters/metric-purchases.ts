import "server-only";
import {ApiError} from "@erpx/server-runtime/public/http";
import type {WorkContext} from "@erpx/server-identity_access/public/authorization";
import {assignmentReadSql} from "@erpx/server-identity_access/public/record-source-authority";
import type {SqlDatabase} from "@erpx/server-runtime/public/runtime/types";
import {manufacturingPackageEnforcementMode} from "@erpx/server-commercial/public/purchase-command-boundary";
import {packageClockSql,purchasedCapabilityPredicate} from "@erpx/server-commercial/public/purchased-capability-sql";
import {manufacturingPurchaseRecordRows,requireManufacturingPurchasedRead,manufacturingSourceAccess} from "@erpx/server-manufacturing/adapters/purchases";
/** Values, evidence search and the retained source callback share the exact
 * native source/purchase predicate before grouping or pagination. */
export async function manufacturingMetricSource(database:SqlDatabase,actor:WorkContext,id:string){
 const planning=["planning-shortage-scenarios","proposed-planning-recommendations","capacity-feasible-recommendations"].includes(id),cost=id==="production-cost",capabilities=planning?["planning.basic"]:cost?["manufacturing.costs","finance.production-costing"]:["manufacturing.execution"];
 await requireManufacturingPurchasedRead(database,actor,{module:planning?"planning":"manufacturing",capabilities});
 if(await manufacturingPackageEnforcementMode()!=="enforced")return {sql:planning?"SELECT id FROM planning_scenarios":"SELECT id FROM manufacturing_orders",args:[] as (string|number|null)[]};
 const predicates=capabilities.map(capability=>purchasedCapabilityPredicate("production_metric.company_id",capability,"history","production_metric_clock.now_ms")),advanced=purchasedCapabilityPredicate("production_metric.company_id","planning.advanced-mrp","history","production_metric_clock.now_ms"),permission=cost?"finance.read":"records.read";
 return {sql:`WITH production_metric_clock AS MATERIALIZED (SELECT ${packageClockSql} now_ms) SELECT production_metric.id FROM (SELECT source.*,?::text department_id FROM (${manufacturingPurchaseRecordRows}) source) production_metric CROSS JOIN production_metric_clock WHERE production_metric.kind='${planning?"scenario":"order"}' AND ${assignmentReadSql("production_metric",permission)} AND ${predicates.join(" AND ")} AND (NOT production_metric.advanced OR ${advanced})`,args:[actor.departmentId,actor.assignmentId,actor.userId,actor.role]};
}

export async function manufacturingMetricHistoryAccess(database:SqlDatabase,actor:WorkContext){
 const ids={"completed-manufacturing-orders":{module:"manufacturing",capabilities:["manufacturing.execution"]},"manufacturing-orders-with-scrap":{module:"manufacturing",capabilities:["manufacturing.execution"]},"production-cost":{module:"manufacturing",capabilities:["manufacturing.costs","finance.production-costing"]},"planning-shortage-scenarios":{module:"planning",capabilities:["planning.basic"]},"proposed-planning-recommendations":{module:"planning",capabilities:["planning.basic"]},"capacity-feasible-recommendations":{module:"planning",capabilities:["planning.basic"]}} as const;
 const result:Record<string,boolean>={};for(const [id,definition] of Object.entries(ids)){if(await manufacturingPackageEnforcementMode()!=="enforced"){result[id]=true;continue;}if(!actor.modules.includes(definition.module)){result[id]=false;continue;}try{result[id]=Boolean((await manufacturingSourceAccess(database,actor,{module:definition.module,capabilities:definition.capabilities}))?.history);}catch(error){if(error instanceof ApiError&&error.status===403){result[id]=false;continue;}throw error;}}
 return result;
}
