import "server-only";
import type {SqlDatabase} from "@erpx/server-runtime/public/runtime/types";
export function prepareCustomerAuthorityEcommerceMerge(write:SqlDatabase,canonical:string,loser:string){return write.prepare('UPDATE ecommerce_carts SET party_id=? WHERE party_id=?').bind(canonical,loser);}
