import "server-only";
import {one} from '@erpx/server-runtime/public/runtime/sql';
import type {SqlDatabase} from "@erpx/server-runtime/public/runtime/types";
export function posCoreIdentityAccessFacts(db:SqlDatabase){return {
readAssignments1:(facts:{assignmentId:string;userId:string;companyId:string;role:string;fact4:number;fact5:number;branchId:string | null;})=>one(db,"SELECT id FROM assignments WHERE id=? AND user_id=? AND company_id=? AND role=? AND revoked_at IS NULL AND valid_from<=? AND valid_until>? AND (scope_kind='holding' OR (scope_kind='branch' AND scope_node_id=?)) FOR SHARE",facts.assignmentId,facts.userId,facts.companyId,facts.role,facts.fact4,facts.fact5,facts.branchId)
};}
