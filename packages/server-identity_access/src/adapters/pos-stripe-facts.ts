import "server-only";
import {one} from '@erpx/server-runtime/public/runtime/sql';
import type {SqlDatabase} from "@erpx/server-runtime/public/runtime/types";
/** Owner-local fixed projection for the POS Stripe holding-assignment role read.
 * The two facts ports previously repeated one byte-identical SQL literal; one
 * projection now serves both call sites so they cannot drift apart. The native
 * identity assignment reader owns the same authority shape. */
const holdingAssignmentRoleSql = "SELECT role FROM assignments WHERE id=? AND user_id=? AND company_id=? AND scope_kind='holding' AND revoked_at IS NULL AND valid_from<=? AND valid_until>?";
export function posStripeIdentityAccessFacts(db:SqlDatabase){return {
readAssignments1:(facts:{assignmentId:string | undefined;userId:string | undefined;company_id:string | number | boolean | null;fact3:number;fact4:number;})=>one(db,holdingAssignmentRoleSql,facts.assignmentId,facts.userId,facts.company_id,facts.fact3,facts.fact4),
readAssignments2:(facts:{assignmentId:string | undefined;userId:string | undefined;company_id:string | number | boolean | null;fact3:number;fact4:number;})=>one(db,holdingAssignmentRoleSql,facts.assignmentId,facts.userId,facts.company_id,facts.fact3,facts.fact4)
};}
