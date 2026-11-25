import 'server-only';
import type {SqlDatabase} from '@erpx/server-runtime/public/runtime/types';
import type {WorkContext} from '@erpx/server-identity_access/public/authorization';
import {hasPermission} from '@erpx/feature-identity_access/lib/permissions';
import {hasModuleAccess} from '@erpx/catalogue/module-access';
import {assetSourceProbeSql} from "./record-purchases";
import {assetProjectionFromSql,assetSelectProjectionSql} from "./asset-scope-tuple";
/** Exact native physical links and original retirement/Maintenance sources.
 * No physical record is created or relabelled by the Finance continuation. */
/** Canonical owner-local fixed JSON aggregate for the physical lineage sources.
 * Assets, events, finance links and Maintenance work share this one literal so
 * the TS read tail carries no duplicate statement. Callers pass only fixed
 * aliases and owner-owned FROM fragments; no SQL is accepted from a request. */
export const assetPhysicalAggSql = (alias:string,from:string) =>
  `(${assetProjectionFromSql(`jsonb_agg(to_jsonb(${alias}) ORDER BY ${alias}.id)`,` ${from}`)})`;
/** Canonical owner-local source-image witness. The physical lineage and the
 * purchase owner source share this one literal so the TS read tail carries no
 * duplicate image statement. Callers pass only a fixed JSON projection and an
 * owner-owned tail; no SQL or relation name is accepted from a request. */
export const assetSourceImageSql = (projection:string,tail="") =>
  assetSelectProjectionSql(projection,`::text image${tail}`);
/** Canonical owner-local Finance-link keyed asset read. The physical lineage
 * witness and the Finance-visible asset links share this one statement so the
 * TS read tail carries no duplicate literal. Callers pass only a fixed
 * projection, an optional table alias and an owner-owned tail; no SQL or
 * relation name is accepted from a request. */
export const assetFinanceLinkReadSql = (projection:string,alias="",tail="") => {
  const qualifier=alias?`${alias}.`:"";
  return assetProjectionFromSql(projection,` operational_assets${alias?` ${alias}`:""}`,` WHERE ${qualifier}company_id=? AND ${qualifier}finance_asset_id=?${tail}`);
};
export function assetsFinancialPhysicalSourceLocks(database:SqlDatabase){return [database.prepare('LOCK TABLE operational_assets,operational_asset_events,operational_asset_finance_links,maintenance_work_orders IN SHARE MODE')];}
export async function assetsFinancialPhysicalSource(database:SqlDatabase,companyId:string,financeAssetId:string){
 const value=await database.prepare(`WITH physical AS MATERIALIZED(${assetFinanceLinkReadSql("a.*","a")})
 ${assetSourceImageSql(`jsonb_build_object('assets',${assetPhysicalAggSql("a","physical a")},'events',${assetPhysicalAggSql("e","operational_asset_events e JOIN physical a ON a.id=e.asset_id")},'links',${assetPhysicalAggSql("l","operational_asset_finance_links l WHERE l.finance_asset_id=?")},'work',${assetPhysicalAggSql("w","maintenance_work_orders w JOIN physical a ON a.id=w.asset_id")})`,`,
 EXISTS(${assetSourceProbeSql("operational_asset_finance_links l LEFT JOIN physical a ON a.id=l.asset_id", "l.finance_asset_id=? AND a.id IS NULL")})
 OR EXISTS(${assetSourceProbeSql("maintenance_work_orders w JOIN physical a ON a.id=w.asset_id", "w.company_id<>a.company_id")}) invalid`)}`).bind(companyId,financeAssetId,financeAssetId,financeAssetId).first<{image:string;invalid:boolean}>();
 if(!value)throw Error('Original physical Asset source unavailable');return value;
}
export async function assetsFinancialPhysicalLinks(database:SqlDatabase,actor:WorkContext,financeAssetId:string){
 if(!hasPermission(actor,'records.read')||!hasModuleAccess(actor.modules,'assets'))return [];
 return (await database.prepare(assetFinanceLinkReadSql("id,code,name,lifecycle_status status,version",""," AND (?::text IS NULL OR branch_id=?) AND (?::text IS NULL OR department_id=?) ORDER BY id")).bind(actor.companyId,financeAssetId,actor.branchId,actor.branchId,actor.departmentId,actor.departmentId).all<{id:string;code:string;name:string;status:string;version:number}>()).results;
}
