import "server-only";
import type {SqlDatabase} from "../../runtime/public/runtime/types";
export function prepareCustomerAuthorityPeoplePayrollMerge(write:SqlDatabase,canonical:string,loser:string){return write.prepare('UPDATE finance_expenses SET payee_party_id=? WHERE payee_party_id=?').bind(canonical,loser);}
