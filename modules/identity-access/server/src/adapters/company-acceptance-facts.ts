import type {SqlDatabase} from "@erpx/platform-runtime-server/public/runtime/types";
import {assertion} from "@erpx/workflow-server/public/command-assertions";
export function companyAcceptanceFacts(db:SqlDatabase){return {
independentReview(key:string,value:"independent-review",userId:string,requested_by:string,companyId:string){return assertion(db, key, value, "NOT iam_same_person(?,?,?)", [userId, requested_by, companyId]);}
};}
