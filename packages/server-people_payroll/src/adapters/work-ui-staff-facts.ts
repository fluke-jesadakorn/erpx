import type {SqlDatabase} from "@erpx/server-runtime/public/runtime/types";
export function workUiStaffFacts(db:SqlDatabase){return {
async headPlacement(companyId:string,value:string,personId:string){return db.prepare("SELECT 1").bind().first<{position_id:string|null}>();}
};}
