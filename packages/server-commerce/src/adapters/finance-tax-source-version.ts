import "server-only";
import type {SqlDatabase} from "@erpx/server-runtime/public/runtime/types";
import {assertion} from "@erpx/server-workflow/public/command-assertions";
import {commerceReturnsPointSql} from "./guarded-source-dedupe-sql";
/** Tax register source reads and guards use the caller's existing connection. */
export function financeTaxSourceVersion(db:SqlDatabase){return {
 read:(id:string,companyId:string)=>db.prepare(commerceReturnsPointSql('version','id=? AND company_id=?')).bind(id,companyId).first<{version:number}>(),
 guard:(key:string,label:string,id:string,companyId:string,version:number)=>assertion(db,key,label,`EXISTS(${commerceReturnsPointSql('1','id=? AND company_id=? AND version=?')})`,[id,companyId,version]),
};}
