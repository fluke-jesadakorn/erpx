import "server-only";
import {createHash} from "node:crypto";
import type {PostgresDatabase} from "../../runtime/public/runtime/postgres";
import type {SqlDatabase} from "../../runtime/public/runtime/types";
import {packageSettlementSource,type PackageSettlementSourceLock} from "../../commercial/public/package-settlement-sources";
import {assertServicePackageObligationSources,servicePackageSettlementSources as service} from "../../field_service/public/package-obligations";
type Input={companyId:string;packageId:string;packageVersion:number;moduleId:string;now:number};const sources=new WeakMap<SqlDatabase,string>();
export async function lockServiceAssetPackageObligationSources(owner:PostgresDatabase,lock:PackageSettlementSourceLock){const {database,companyId}=packageSettlementSource(owner,lock);await database.prepare("LOCK TABLE operational_assets,operational_asset_events IN SHARE MODE").run();sources.set(database,companyId);}
/** Equipment remains the customer's retained property after this software is
 * withdrawn. Validate the actual reference; never force retirement or disposal. */
export async function serviceAssetPackageObligations(owner:PostgresDatabase,database:SqlDatabase,input:Input){
 owner.assertActiveTransaction(database);if(sources.get(database)!==input.companyId)throw Error("SERVICE_ASSET_PACKAGE_OBLIGATION_SOURCE_REQUIRED");assertServicePackageObligationSources(owner,database,input.companyId);if(input.moduleId!=="assets"||!["service","service-parts-assets"].includes(input.packageId)||input.packageVersion!==1||!Number.isSafeInteger(input.now)||input.now<0)throw Error("SERVICE_ASSET_PACKAGE_OBLIGATION_SCOPE_REQUIRED");
 const facts=(await database.prepare(`WITH scope AS(SELECT ?::text company_id),refs AS(SELECT agreement.id,agreement.company_id,agreement.branch_id,agreement.department_id,agreement.asset_id FROM (${service.agreements}) agreement,scope WHERE agreement.company_id=scope.company_id AND agreement.asset_id IS NOT NULL UNION ALL SELECT source.id,source.company_id,source.branch_id,source.department_id,source.asset_id FROM (${service.orders}) source,scope WHERE source.company_id=scope.company_id AND source.asset_id IS NOT NULL)
 SELECT source.id,to_jsonb(source)::text fact FROM refs source LEFT JOIN operational_assets asset ON asset.id=source.asset_id AND asset.company_id=source.company_id AND asset.branch_id=source.branch_id AND (source.department_id IS NULL OR asset.department_id=source.department_id) WHERE asset.id IS NULL ORDER BY source.id,fact`).bind(input.companyId).all()).results;
 return {companyId:input.companyId,packageId:input.packageId,moduleId:"assets",observedAt:input.now,revision:createHash("sha256").update(JSON.stringify({companyId:input.companyId,packageId:input.packageId,packageVersion:input.packageVersion,facts})).digest("hex"),outstanding:facts.length};
}
