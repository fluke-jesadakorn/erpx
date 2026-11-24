import "server-only";
import type { SqlDatabase } from "@erpx/server-runtime/public/runtime/types";
import {
  ASSETS_RD_FS_ASSET_ATTACHMENT_IMAGE,
  nativeAssetsReadConfigured,
  nativeAssetsReadFirst,
} from "./native-assets-read";
import {assetOperationalRowsSql} from "./asset-operational-rows-sql";

export const fieldServiceAssetSourceRows = assetOperationalRowsSql("id,company_id,branch_id,department_id,lifecycle_status,warranty_end,version");
/** Private access-audit effects can write a source; lease before its snapshot. */
export function fieldServiceAssetAttachmentSourceLocks(database:SqlDatabase){return database.prepare('LOCK TABLE operational_assets IN SHARE ROW EXCLUSIVE MODE');}
export async function fieldServiceAssetAttachmentSourceImage(database:SqlDatabase,companyId:string,id:string){
  if(nativeAssetsReadConfigured()){
    return nativeAssetsReadFirst<string>(ASSETS_RD_FS_ASSET_ATTACHMENT_IMAGE,{assetId:id,companyId},database);
  }
  return database.prepare('SELECT to_jsonb(source)::text image FROM operational_assets source WHERE source.id=? AND source.company_id=?').bind(id,companyId).first<string>('image');
}
