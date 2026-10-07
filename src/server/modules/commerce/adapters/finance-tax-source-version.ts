import "server-only";
import type {SqlDatabase} from "../../runtime/public/runtime/types";
import {assertion} from "../../workflow/public/commands";
/** Tax register source reads and guards use the caller's existing connection. */
export function financeTaxSourceVersion(db:SqlDatabase){return {
 read:(id:string,companyId:string)=>db.prepare("SELECT version FROM commerce_returns WHERE id=? AND company_id=?").bind(id,companyId).first<{version:number}>(),
 guard:(key:string,label:string,id:string,companyId:string,version:number)=>assertion(db,key,label,"EXISTS(SELECT 1 FROM commerce_returns WHERE id=? AND company_id=? AND version=?)",[id,companyId,version]),
};}
