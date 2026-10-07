import type {SqlDatabase} from "../../runtime/public/runtime/types";
import {assertion} from "../../workflow/public/commands";
export function companyAcceptanceFacts(db:SqlDatabase){return {
independentReview(key:string,value:"independent-review",userId:string,requested_by:string,companyId:string){return assertion(db, key, value, "NOT iam_same_person(?,?,?)", [userId, requested_by, companyId]);}
};}
