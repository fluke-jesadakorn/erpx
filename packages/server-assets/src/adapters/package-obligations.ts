import "server-only";
import {createHash} from "node:crypto";
import type {PostgresDatabase} from "@erpx/server-runtime/public/runtime/postgres";
import type {SqlDatabase} from "@erpx/server-runtime/public/runtime/types";
import {packageSettlementSource,type PackageSettlementSourceLock} from "@erpx/server-commercial/public/package-settlement-sources";
import {talentSettlementPlacements as placements,talentSettlementPlacementReadLocks as placementLocks} from "@erpx/server-organization/public/talent-settlement-sources";
import {assetsSettlementFinancialAssetReadLocks} from "@erpx/server-finance/public/assets-settlement-sources";
import {maintenanceAssetSettlementRefReadLocks} from "@erpx/server-maintenance/public/asset-settlement-refs";
import {maintenanceSettlementInventoryReadLocks} from "@erpx/server-inventory/public/maintenance-settlement-sources";
import {assetSourceProbeSql} from "./record-purchases";
import {assetAllRowsSql} from "./asset-all-rows-sql";
import {assetProjectionFromSql} from "./asset-scope-tuple";
import {
  ASSETS_RD_OBLIGATIONS_BASE,
  ASSETS_RD_OBLIGATIONS_PARTS,
  ASSETS_RD_OBLIGATIONS_PREVENTIVE,
  nativeAssetsReadAll,
} from "./native-assets-read";
type Input={companyId:string;packageId:string;packageVersion:number;moduleId:string;now:number};const sources=new WeakMap<SqlDatabase,string>();
/** Canonical owner-local EXISTS wrapper. It reuses the owner-local source probe
 * so the row-existence literal is defined once in this owner; callers pass only
 * fixed FROM and predicate fragments, never request SQL or relation names. */
const assetExistsSql = (from:string,where:string) => `EXISTS(${assetSourceProbeSql(from,where)})`;
/** Canonical owner-local source-existence probe for Assets package settlement.
 * Branch, department and maintenance-ref callers share this one literal so the
 * TS read tail carries no duplicate statement. Callers pass only owner-owned
 * relation, alias and predicate fragments; no request SQL or relation name is accepted. */
const assetSourceExistsSql = (source:string,alias:string,where:string) =>
    assetExistsSql(`(${source}) ${alias}`, where);
export const assetPackageSettlementSources={assets:assetProjectionFromSql(`asset.*,COALESCE(${assetSourceExistsSql(placements,"branch","branch.id=asset.branch_id AND branch.company_id=asset.company_id AND branch.kind IN ('branch','headquarters')")} AND (asset.department_id IS NULL OR ${assetSourceExistsSql(placements,"department","department.id=asset.department_id AND department.company_id=asset.company_id AND department.branch_id=asset.branch_id AND department.kind='department'")}),false) source_valid`," operational_assets asset"),links:assetAllRowsSql("operational_asset_finance_links")} as const;
export async function lockAssetPackageObligationSources(owner:PostgresDatabase,lock:PackageSettlementSourceLock){const {database,companyId}=packageSettlementSource(owner,lock);await database.prepare("LOCK TABLE operational_assets,operational_asset_events,operational_asset_finance_links IN SHARE MODE").run();await database.batch([...placementLocks(database),...assetsSettlementFinancialAssetReadLocks(database),...maintenanceAssetSettlementRefReadLocks(database),...maintenanceSettlementInventoryReadLocks(database)]);sources.set(database,companyId);}
export function assertAssetPackageObligationSources(owner:PostgresDatabase,database:SqlDatabase,companyId:string){owner.assertActiveTransaction(database);if(sources.get(database)!==companyId)throw Error("ASSETS_PACKAGE_OBLIGATION_SOURCE_REQUIRED");}
/** Retired TS obligation fallback; the registered owner read plans are byte-identical. */
export async function assetPackageObligations(owner:PostgresDatabase,database:SqlDatabase,input:Input){
 assertAssetPackageObligationSources(owner,database,input.companyId);if(input.moduleId!=="assets"||!["assets","assets-preventive","assets-parts-costs"].includes(input.packageId)||input.packageVersion!==1||!Number.isSafeInteger(input.now)||input.now<0)throw Error("ASSETS_PACKAGE_OBLIGATION_SCOPE_REQUIRED");const full=input.packageId==="assets";
 const kind=full?ASSETS_RD_OBLIGATIONS_BASE:input.packageId==="assets-preventive"?ASSETS_RD_OBLIGATIONS_PREVENTIVE:ASSETS_RD_OBLIGATIONS_PARTS;
 const facts=(await nativeAssetsReadAll<{kind:string;id:string;version:number;state:string;pending:boolean}>(kind,{companyId:input.companyId,isBase:full},database)).results;
 return {companyId:input.companyId,packageId:input.packageId,moduleId:"assets",observedAt:input.now,revision:createHash("sha256").update(JSON.stringify({companyId:input.companyId,packageId:input.packageId,packageVersion:input.packageVersion,facts})).digest("hex"),outstanding:facts.filter(fact=>fact.pending).length};
}
