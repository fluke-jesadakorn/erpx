import type {SqlDatabase,SqlStatement} from "@erpx/server-runtime/public/runtime/types";
import {assertion} from "@erpx/server-workflow/public/command-assertions";
import { nativeOrgStatement } from './native-organization-owner-write';
export function spendingTemplateOwner(db:SqlDatabase,companyId=''){return {
read:(companyId:string)=>db.prepare("SELECT version,exceptions_json,reason FROM spending_approval_templates WHERE company_id=?").bind(companyId).first<{version:number;exceptions_json:string;reason:string}>(),
guard:(key:string,companyId:string,expectedVersion:number)=>assertion(db,key,"template-version","COALESCE((SELECT version FROM spending_approval_templates WHERE company_id=?),1)=?",[companyId,expectedVersion]),
publish:(companyId:string,version:number,exceptionsJson:string,reason:string,userId:string,now:number):Promise<SqlStatement>=>nativeOrgStatement(db,companyId,'spending_template_publish',[companyId,version,exceptionsJson,reason,userId,now],`spending-template:${companyId}:${version}`,now)
};}
