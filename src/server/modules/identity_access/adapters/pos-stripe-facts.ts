import "server-only";
import {one} from "../../pos/public/pos-core";
import type {SqlDatabase} from "../../runtime/public/runtime/types";
export function posStripeIdentityAccessFacts(db:SqlDatabase){return {
readAssignments1:(facts:{assignmentId:string | undefined;userId:string | undefined;company_id:string | number | boolean | null;fact3:number;fact4:number;})=>one(db,"SELECT role FROM assignments WHERE id=? AND user_id=? AND company_id=? AND scope_kind='holding' AND revoked_at IS NULL AND valid_from<=? AND valid_until>?",facts.assignmentId,facts.userId,facts.company_id,facts.fact3,facts.fact4),
readAssignments2:(facts:{assignmentId:string | undefined;userId:string | undefined;company_id:string | number | boolean | null;fact3:number;fact4:number;})=>one(db,"SELECT role FROM assignments WHERE id=? AND user_id=? AND company_id=? AND scope_kind='holding' AND revoked_at IS NULL AND valid_from<=? AND valid_until>?",facts.assignmentId,facts.userId,facts.company_id,facts.fact3,facts.fact4)
};}
