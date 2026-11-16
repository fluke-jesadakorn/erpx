import "server-only";
import type {SqlDatabase} from "@erpx/server-runtime/public/runtime/types";
export function legalPortalStorageAllowance(db:SqlDatabase,companyId:string){return db.prepare("SELECT storage_bytes FROM beta_policy WHERE company_id=?").bind(companyId).first<{storage_bytes:number}>();}
