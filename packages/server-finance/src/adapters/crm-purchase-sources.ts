import "server-only";
import type {SqlDatabase} from "@erpx/server-runtime/public/runtime/types";
/** Sales customer reference retains Finance's actual independently reviewed
 * identity; it does not confer generic Finance management authority. */
export const crmFinanceCustomerSources="SELECT id,company_id,kind,status FROM finance_parties";
export function crmFinanceSourceReadLocks(database:SqlDatabase){return [database.prepare("LOCK TABLE finance_parties IN SHARE MODE")];}
