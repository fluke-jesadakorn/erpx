import "server-only";
import {createHash} from "node:crypto";
import type {PostgresDatabase} from "../../runtime/public/runtime/postgres";
import type {SqlDatabase} from "../../runtime/public/runtime/types";
import {packageSettlementSource,type PackageSettlementSourceLock} from "../../commercial/public/package-settlement-sources";
import {planningPurchaseSourceReadLocks,planningPurchaseScenarioRows} from "./purchase-sources";
type Input={companyId:string;packageId:string;packageVersion:number;moduleId:string;now:number};const sources=new WeakMap<SqlDatabase,string>();
export async function lockManufacturingPlanningPackageObligationSources(owner:PostgresDatabase,lock:PackageSettlementSourceLock){const {database,companyId}=packageSettlementSource(owner,lock);await database.batch(planningPurchaseSourceReadLocks(database));sources.set(database,companyId);}
export async function manufacturingPlanningPackageObligations(owner:PostgresDatabase,database:SqlDatabase,input:Input){
 owner.assertActiveTransaction(database);if(sources.get(database)!==input.companyId)throw Error("MANUFACTURING_PLANNING_PACKAGE_OBLIGATION_SOURCE_REQUIRED");if(input.moduleId!=="planning"||!["manufacturing","manufacturing-mrp"].includes(input.packageId)||input.packageVersion!==1||!Number.isSafeInteger(input.now)||input.now<0)throw Error("MANUFACTURING_PLANNING_PACKAGE_OBLIGATION_SCOPE_REQUIRED");
 const facts=(await database.prepare(`WITH scope AS(SELECT ?::text company_id,?::boolean full),scenarios AS MATERIALIZED(SELECT s.*,valid.id IS NOT NULL source_valid FROM planning_scenarios s LEFT JOIN (${planningPurchaseScenarioRows}) valid ON valid.id=s.id AND valid.company_id=s.company_id,scope WHERE s.company_id=scope.company_id AND (scope.full OR COALESCE(s.context_json::jsonb->>'comparisonScenarioId','')<>'' OR jsonb_exists(s.context_json::jsonb,'comparison') OR valid.id IS NULL))
 SELECT 'scenario' kind,s.id,to_jsonb(s)::text fact FROM scenarios s WHERE s.status NOT IN ('released','cancelled') OR NOT s.source_valid
 UNION ALL SELECT 'recommendation',r.id,to_jsonb(r)::text FROM planning_recommendations r JOIN scenarios s ON s.id=r.scenario_id WHERE r.status NOT IN ('approved','rejected')
 UNION ALL SELECT 'capacity',a.id,to_jsonb(a)::text FROM planning_resource_allocations a JOIN scenarios s ON s.id=a.scenario_id WHERE a.released_at IS NULL
 ORDER BY kind,id`).bind(input.companyId,input.packageId==="manufacturing").all()).results;
 return {companyId:input.companyId,packageId:input.packageId,moduleId:"planning",observedAt:input.now,revision:createHash("sha256").update(JSON.stringify({companyId:input.companyId,packageVersion:input.packageVersion,facts})).digest("hex"),outstanding:facts.length};
}
