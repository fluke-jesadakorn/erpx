import "server-only";
import type {SqlDatabase} from "@erpx/server-runtime/public/runtime/types";
/** Expense consumes owner effects on its supplied transaction; this port only prepares the original statements in their original order. */
export function expenseNativeEffects(db:SqlDatabase){return {
clearAssertions:(facts:{commandId:string})=>db.prepare("DELETE FROM command_assertions WHERE command_id=?").bind(facts.commandId)
};}
/** Reuse the existing assertion clearing operation for other owner-supplied transaction batches. */
export function clearWorkflowCommandAssertions(db:SqlDatabase,commandId:string){return expenseNativeEffects(db).clearAssertions({commandId});}
