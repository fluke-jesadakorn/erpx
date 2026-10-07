import type {SqlDatabase} from "../../runtime/public/runtime/types";
import {assertion} from "../../workflow/public/commands";
export function operationsPolicyVersionGuard(db:SqlDatabase,key:string,step:"policy-review",id:string,companyId:string,version:number,status:"published"|"draft",creatorArgs:string[]){return assertion(db,key,step,`EXISTS(SELECT 1 FROM operations_queue_policies WHERE id=? AND company_id=? AND version=? AND status=? ${status!=="published"?"AND NOT iam_same_person(created_by,?,company_id)":""})`,[id,companyId,version,status,...creatorArgs]);}
