import "server-only";
import {ApiError} from "@erpx/platform-runtime-server/public/http";
import type {WorkContext} from "@erpx/identity-access-server/public/authorization";
import {assignmentReadSql} from "@erpx/identity-access-server/public/record-source-authority";
import type {SqlDatabase} from "@erpx/platform-runtime-server/public/runtime/types";
import {manufacturingPackageEnforcementMode} from "@erpx/commercial-server/public/purchase-command-boundary";
import {purchasedCapabilityPredicate} from "@erpx/commercial-server/public/purchased-capability-sql";
import {requireManufacturingPurchasedRead,manufacturingSourceAccess,manufacturingClockCte} from "@erpx/manufacturing-server/adapters/purchases";
import {manufacturingIdSelect} from "./manufacturing-read-sql-fragments";
import {manufacturingPurchaseScopedSelect} from "./manufacturing-read-tail-r24-sql";
/** Values, evidence search and the retained source callback share the exact
 * native source/purchase predicate before grouping or pagination. */
export async function manufacturingMetricSource(database:SqlDatabase,actor:WorkContext,id:string){
 const planning=["planning-shortage-scenarios","proposed-planning-recommendations","capacity-feasible-recommendations"].includes(id),cost=id==="production-cost",capabilities=planning?["planning.basic"]:cost?["manufacturing.costs","finance.production-costing"]:["manufacturing.execution"];
 await requireManufacturingPurchasedRead(database,actor,{module:planning?"planning":"manufacturing",capabilities});
 if(await manufacturingPackageEnforcementMode()!=="enforced")return {sql:manufacturingIdSelect(planning?"planning_scenarios":"manufacturing_orders"),args:[] as (string|number|null)[]};
 const predicates=capabilities.map(capability=>purchasedCapabilityPredicate("production_metric.company_id",capability,"history","production_metric_clock.now_ms")),advanced=purchasedCapabilityPredicate("production_metric.company_id","planning.advanced-mrp","history","production_metric_clock.now_ms"),permission=cost?"finance.read":"records.read";
 const tail=`CROSS JOIN production_metric_clock WHERE production_metric.kind='${planning?"scenario":"order"}' AND ${assignmentReadSql("production_metric",permission)} AND ${predicates.join(" AND ")} AND (NOT production_metric.advanced OR ${advanced})`;
 return {sql:`WITH ${manufacturingClockCte("production_metric_clock")} ${manufacturingPurchaseScopedSelect("production_metric.id","production_metric",tail)}`,args:[actor.departmentId,actor.assignmentId,actor.userId,actor.role]};
}

export async function manufacturingMetricHistoryAccess(database:SqlDatabase,actor:WorkContext){
 const ids={"completed-manufacturing-orders":{module:"manufacturing",capabilities:["manufacturing.execution"]},"manufacturing-orders-with-scrap":{module:"manufacturing",capabilities:["manufacturing.execution"]},"production-cost":{module:"manufacturing",capabilities:["manufacturing.costs","finance.production-costing"]},"planning-shortage-scenarios":{module:"planning",capabilities:["planning.basic"]},"proposed-planning-recommendations":{module:"planning",capabilities:["planning.basic"]},"capacity-feasible-recommendations":{module:"planning",capabilities:["planning.basic"]}} as const;
 const result:Record<string,boolean>={};for(const [id,definition] of Object.entries(ids)){if(await manufacturingPackageEnforcementMode()!=="enforced"){result[id]=true;continue;}if(!actor.modules.includes(definition.module)){result[id]=false;continue;}try{result[id]=Boolean((await manufacturingSourceAccess(database,actor,{module:definition.module,capabilities:definition.capabilities}))?.history);}catch(error){if(error instanceof ApiError&&error.status===403){result[id]=false;continue;}throw error;}}
 return result;
}
