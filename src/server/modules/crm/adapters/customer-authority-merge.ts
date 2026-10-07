import "server-only";
import type {SqlDatabase} from "../../runtime/public/runtime/types";
export function prepareCustomerAuthorityCrmMerge(write:SqlDatabase,canonical:string,loser:string){return write.prepare('UPDATE crm_opportunities SET customer_id=? WHERE customer_id=?').bind(canonical,loser);}
