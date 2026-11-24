import "server-only";
import {createHash} from "node:crypto";
import type {PostgresDatabase} from "@erpx/server-runtime/public/runtime/postgres";
import type {SqlDatabase} from "@erpx/server-runtime/public/runtime/types";
import {packageSettlementSource,type PackageSettlementSourceLock} from "@erpx/server-commercial/public/package-settlement-sources";
import {assertAssetPackageObligationSources,assetPackageSettlementSources as assets} from "@erpx/server-assets/public/package-obligations";
import {maintenanceSettlementConsumptions,maintenanceSettlementInventoryReadLocks} from "@erpx/server-inventory/public/maintenance-settlement-sources";
type Input={companyId:string;packageId:string;packageVersion:number;moduleId:string;now:number};const sources=new WeakMap<SqlDatabase,string>();
/** Canonical owner-local Work/Plan source image for Maintenance package settlement.
 * Both callers share this one fixed statement so the TS read tail carries no
 * duplicate literal. Callers pass only owner-owned relation and alias fragments;
 * no request SQL or relation name is accepted. */
const ownerSourceSql = (table:string,alias:string) =>
  `SELECT ${alias}.*,COALESCE(asset.source_valid AND asset.branch_id=${alias}.branch_id AND asset.department_id IS NOT DISTINCT FROM ${alias}.department_id,false) source_valid FROM ${table} ${alias} LEFT JOIN (${assets.assets}) asset ON asset.id=${alias}.asset_id AND asset.company_id=${alias}.company_id`;
const work=ownerSourceSql("maintenance_work_orders","work"),plans=ownerSourceSql("maintenance_preventive_plans","plan");
/** Canonical owner-local full-row read shared by the occurrence and work-event
 * settlement sources. Callers pass only an owner-fixed table from the closed
 * union, never request-authored SQL; both composed strings stay byte-identical. */
const settlementEventRows=(table:"maintenance_preventive_occurrences"|"maintenance_work_events")=>`SELECT * FROM ${table}`;
/** Canonical owner-local upstream-source witness shared by the preventive and
 * parts-costs package scopes. Both assert that a Work row is backed by a real
 * owner-owned source (preventive occurrence or part consumption); callers pass
 * only an owner-fixed relation, alias and column from closed sets, never
 * request-authored SQL. Both composed strings stay byte-identical to their
 * pre-dedupe literals. */
const workSourceExists=(source:string,alias:string,column:string)=>`EXISTS(SELECT 1 FROM ${source} ${alias} WHERE ${alias}.${column}=work.id)`;
/** Canonical owner-local scoped source CTE shared by the Work and Plan branches
 * of the package-obligations facts read. Callers pass only an owner-fixed alias,
 * a closed source relation and a fixed scope predicate, never request-authored
 * SQL; each composed CTE stays byte-identical to its pre-dedupe literal. */
export const scopedSourceCte=(alias:"work"|"plan",source:string,extra:string)=>`SELECT ${alias}.* FROM (${source}) ${alias},scope WHERE ${alias}.company_id=scope.company_id AND ${extra}`;
const workScopeExtra=`(scope.package_id='assets' OR scope.package_id='assets-preventive' AND ${workSourceExists("maintenance_preventive_occurrences","occurrence","work_order_id")} OR scope.package_id='assets-parts-costs' AND ${workSourceExists(`(${maintenanceSettlementConsumptions})`,"part","source_id")})`;
export const maintenancePackageSettlementSources={work,plans,consumptions:maintenanceSettlementConsumptions,occurrences:settlementEventRows("maintenance_preventive_occurrences"),workEvents:settlementEventRows("maintenance_work_events")} as const;
export async function lockMaintenancePackageObligationSources(owner:PostgresDatabase,lock:PackageSettlementSourceLock){const {database,companyId}=packageSettlementSource(owner,lock);await database.prepare("LOCK TABLE maintenance_work_orders,maintenance_work_events,maintenance_preventive_plans,maintenance_preventive_events,maintenance_preventive_occurrences IN SHARE MODE").run();await database.batch(maintenanceSettlementInventoryReadLocks(database));sources.set(database,companyId);}
export function assertMaintenancePackageObligationSources(owner:PostgresDatabase,database:SqlDatabase,companyId:string){owner.assertActiveTransaction(database);if(sources.get(database)!==companyId)throw Error("MAINTENANCE_PACKAGE_OBLIGATION_SOURCE_REQUIRED");assertAssetPackageObligationSources(owner,database,companyId);}
export async function maintenancePackageObligations(owner:PostgresDatabase,database:SqlDatabase,input:Input){
 assertMaintenancePackageObligationSources(owner,database,input.companyId);if(input.moduleId!=="maintenance"||!["assets","assets-preventive","assets-parts-costs"].includes(input.packageId)||input.packageVersion!==1||!Number.isSafeInteger(input.now)||input.now<0)throw Error("MAINTENANCE_PACKAGE_OBLIGATION_SCOPE_REQUIRED");
 const facts=(await database.prepare(`WITH scope AS(SELECT ?::text company_id,?::text package_id),work AS MATERIALIZED(${scopedSourceCte("work",work,workScopeExtra)}),plans AS MATERIALIZED(${scopedSourceCte("plan",plans,"scope.package_id<>'assets-parts-costs'")})
 SELECT 'work' kind,work.id,work.version,work.status state,NOT work.source_valid OR work.status NOT IN ('completed','cancelled') OR work.downtime_started_at IS NOT NULL AND work.downtime_ended_at IS NULL pending FROM work
 UNION ALL SELECT 'preventive-plan',plan.id,plan.version,plan.status,NOT plan.source_valid OR plan.status<>'cancelled' FROM plans plan
 UNION ALL SELECT 'occurrence',occurrence.id,occurrence.plan_version,'ambiguous-source',true FROM maintenance_preventive_occurrences occurrence JOIN plans plan ON plan.id=occurrence.plan_id LEFT JOIN (${work}) generated ON generated.id=occurrence.work_order_id AND generated.company_id=plan.company_id WHERE generated.id IS NULL OR generated.asset_id<>plan.asset_id OR occurrence.plan_version>plan.version OR NOT generated.source_valid OR occurrence.source_asset_version>(SELECT version FROM (${assets.assets}) actual WHERE actual.id=plan.asset_id AND actual.company_id=plan.company_id)
 ORDER BY kind,id`).bind(input.companyId,input.packageId).all<{kind:string;id:string;version:number;state:string;pending:boolean}>()).results;
 return {companyId:input.companyId,packageId:input.packageId,moduleId:"maintenance",observedAt:input.now,revision:createHash("sha256").update(JSON.stringify({companyId:input.companyId,packageId:input.packageId,packageVersion:input.packageVersion,facts})).digest("hex"),outstanding:facts.filter(fact=>fact.pending).length};
}
