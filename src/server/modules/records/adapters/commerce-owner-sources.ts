import "server-only";
import type {SqlDatabase} from "../../runtime/public/runtime/types";
/** The approved employee purchase request is read and linked by the complete
 * PO handoff. Acquire its eventual writer mode before the first source read. */
export function commerceOwnerRequestSourceReadLocks(database:SqlDatabase){return [
 database.prepare("LOCK TABLE employee_requests IN SHARE ROW EXCLUSIVE MODE")
];}
