import "server-only";
import type { SqlDatabase } from "@erpx/server-runtime/public/runtime/types";

export const fieldServiceAssetSourceRows = "SELECT id,company_id,branch_id,department_id,lifecycle_status,warranty_end,version FROM operational_assets";
/** Private access-audit effects can write a source; lease before its snapshot. */
export function fieldServiceAssetAttachmentSourceLocks(database:SqlDatabase){return database.prepare('LOCK TABLE operational_assets IN SHARE ROW EXCLUSIVE MODE');}
export function fieldServiceAssetAttachmentSourceImage(database:SqlDatabase,companyId:string,id:string){return database.prepare('SELECT to_jsonb(source)::text image FROM operational_assets source WHERE source.id=? AND source.company_id=?').bind(id,companyId).first<string>('image');}
