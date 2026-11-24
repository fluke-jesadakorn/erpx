import type {SqlDatabase} from "@erpx/server-runtime/public/runtime/types";
import {assertion} from "@erpx/server-workflow/public/command-assertions";
/** Canonical owner-version EXISTS guard shared by Operations owner writes.
 *  Source, trailing filter and key predicate are the only varying bytes; every
 *  rendering is byte-identical to the original inline literals. */
export const operationsOwnerVersionExists = (
  from: string,
  tail = '',
  predicate = 'id=? AND company_id=? AND version=?',
) => `EXISTS(SELECT 1 FROM ${from} WHERE ${predicate}${tail})`;
export function operationsPolicyVersionGuard(db:SqlDatabase,key:string,step:"policy-review",id:string,companyId:string,version:number,status:"published"|"draft",creatorArgs:string[]){return assertion(db,key,step,operationsOwnerVersionExists('operations_queue_policies',` AND status=? ${status!=="published"?"AND NOT iam_same_person(created_by,?,company_id)":""}`),[id,companyId,version,status,...creatorArgs]);}
