import "server-only";
import type {SqlDatabase} from "@erpx/platform-runtime-server/public/runtime/types";
import { clearAssertionSql } from "./native-command-assertion-sql";
/** Expense consumes owner effects on its supplied transaction; this port only prepares the original statements in their original order. */
export function expenseNativeEffects(db:SqlDatabase){return {
clearAssertions:(facts:{commandId:string})=>db.prepare(clearAssertionSql()).bind(facts.commandId)
};}
/** Reuse the existing assertion clearing operation for other owner-supplied transaction batches. */
export function clearWorkflowCommandAssertions(db:SqlDatabase,commandId:string){return expenseNativeEffects(db).clearAssertions({commandId});}
