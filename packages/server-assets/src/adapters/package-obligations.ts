import "server-only";
import {createHash} from "node:crypto";
import type {PostgresDatabase} from "@erpx/server-runtime/public/runtime/postgres";
import type {SqlDatabase} from "@erpx/server-runtime/public/runtime/types";
import {packageSettlementSource,type PackageSettlementSourceLock} from "@erpx/server-commercial/public/package-settlement-sources";
import {talentSettlementPlacements as placements,talentSettlementPlacementReadLocks as placementLocks} from "@erpx/server-organization/public/talent-settlement-sources";
import {assetsSettlementFinancialAssets,assetsSettlementFinancialAssetReadLocks} from "@erpx/server-finance/public/assets-settlement-sources";
import {maintenanceAssetSettlementRefs,maintenanceAssetSettlementRefReadLocks} from "@erpx/server-maintenance/public/asset-settlement-refs";
import {maintenanceSettlementInventoryReadLocks} from "@erpx/server-inventory/public/maintenance-settlement-sources";
import {assetSourceProbeSql} from "./record-purchases";
import {assetAllRowsSql} from "./asset-all-rows-sql";
import {
  ASSETS_RD_OBLIGATIONS_BASE,
  ASSETS_RD_OBLIGATIONS_PARTS,
  ASSETS_RD_OBLIGATIONS_PREVENTIVE,
  nativeAssetsReadAll,
  nativeAssetsReadConfigured,
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
export const assetPackageSettlementSources={assets:`SELECT asset.*,COALESCE(${assetSourceExistsSql(placements,"branch","branch.id=asset.branch_id AND branch.company_id=asset.company_id AND branch.kind IN ('branch','headquarters')")} AND (asset.department_id IS NULL OR ${assetSourceExistsSql(placements,"department","department.id=asset.department_id AND department.company_id=asset.company_id AND department.branch_id=asset.branch_id AND department.kind='department'")}),false) source_valid FROM operational_assets asset`,links:assetAllRowsSql("operational_asset_finance_links")} as const;
export async function lockAssetPackageObligationSources(owner:PostgresDatabase,lock:PackageSettlementSourceLock){const {database,companyId}=packageSettlementSource(owner,lock);await database.prepare("LOCK TABLE operational_assets,operational_asset_events,operational_asset_finance_links IN SHARE MODE").run();await database.batch([...placementLocks(database),...assetsSettlementFinancialAssetReadLocks(database),...maintenanceAssetSettlementRefReadLocks(database),...maintenanceSettlementInventoryReadLocks(database)]);sources.set(database,companyId);}
export function assertAssetPackageObligationSources(owner:PostgresDatabase,database:SqlDatabase,companyId:string){owner.assertActiveTransaction(database);if(sources.get(database)!==companyId)throw Error("ASSETS_PACKAGE_OBLIGATION_SOURCE_REQUIRED");}
export async function assetPackageObligations(owner:PostgresDatabase,database:SqlDatabase,input:Input){
 assertAssetPackageObligationSources(owner,database,input.companyId);if(input.moduleId!=="assets"||!["assets","assets-preventive","assets-parts-costs"].includes(input.packageId)||input.packageVersion!==1||!Number.isSafeInteger(input.now)||input.now<0)throw Error("ASSETS_PACKAGE_OBLIGATION_SCOPE_REQUIRED");const full=input.packageId==="assets",refs=input.packageId==="assets-preventive"?maintenanceAssetSettlementRefs.preventive:maintenanceAssetSettlementRefs.parts;
 const kind=full?ASSETS_RD_OBLIGATIONS_BASE:input.packageId==="assets-preventive"?ASSETS_RD_OBLIGATIONS_PREVENTIVE:ASSETS_RD_OBLIGATIONS_PARTS;
 const facts=nativeAssetsReadConfigured()
   ? (await nativeAssetsReadAll<{kind:string;id:string;version:number;state:string;pending:boolean}>(kind,{companyId:input.companyId,isBase:full},database)).results
   : (await database.prepare(`WITH scope AS(SELECT ?::text company_id,?::boolean is_base),assets AS MATERIALIZED(SELECT asset.* FROM (${assetPackageSettlementSources.assets}) asset,scope WHERE asset.company_id=scope.company_id AND (scope.is_base OR ${assetSourceExistsSql(refs,"source","source.asset_id=asset.id AND source.company_id=asset.company_id")}))
 SELECT 'asset' kind,asset.id,asset.version,asset.lifecycle_status state,NOT asset.source_valid OR (SELECT is_base FROM scope) AND (asset.lifecycle_status IN ('maintenance','retirement_pending') OR asset.cost_minor>0 AND NOT ${assetExistsSql(`(${assetsSettlementFinancialAssets}) financial JOIN (${assetPackageSettlementSources.links}) link ON link.asset_id=asset.id AND link.finance_asset_id=financial.id`, `financial.id=asset.finance_asset_id AND financial.company_id=asset.company_id AND financial.currency=asset.currency AND financial.cost_minor=asset.cost_minor AND link.cost_minor=asset.cost_minor AND link.currency=asset.currency AND link.asset_version<=asset.version AND link.finance_asset_version<=financial.version AND financial.activation_journal_id IS NOT NULL AND (asset.lifecycle_status='retired' AND financial.status='retired' OR asset.lifecycle_status<>'retired' AND financial.status='in_service')`)}) pending FROM assets asset
 UNION ALL SELECT 'asset-link',link.id,link.asset_version,'ambiguous-source',true FROM (${assetPackageSettlementSources.links}) link JOIN assets asset ON asset.id=link.asset_id LEFT JOIN (${assetsSettlementFinancialAssets}) financial ON financial.id=link.finance_asset_id AND financial.company_id=asset.company_id WHERE financial.id IS NULL OR link.currency<>asset.currency OR link.cost_minor<>asset.cost_minor OR financial.cost_minor<>link.cost_minor OR financial.currency<>link.currency OR link.asset_version>asset.version OR link.finance_asset_version>financial.version
 ORDER BY kind,id`).bind(input.companyId,full).all<{kind:string;id:string;version:number;state:string;pending:boolean}>()).results;
 return {companyId:input.companyId,packageId:input.packageId,moduleId:"assets",observedAt:input.now,revision:createHash("sha256").update(JSON.stringify({companyId:input.companyId,packageId:input.packageId,packageVersion:input.packageVersion,facts})).digest("hex"),outstanding:facts.filter(fact=>fact.pending).length};
}
