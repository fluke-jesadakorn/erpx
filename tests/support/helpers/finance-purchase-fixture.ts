import type { PostgresDatabase } from "../../../src/server/modules/runtime/public/runtime/postgres";
import { packageGrantStore, readCompanyPackageGrants } from "../../../src/server/modules/commercial/public/package-grants";
/** Disposable native fixtures only; never substitutes a withdrawal witness. */
export async function reviewedFinanceGrant(database: PostgresDatabase, companyId: string, packageId: "finance" | "finance-advanced" | "assets", state: "active" | "draining" = "active") {
  const prior = (await readCompanyPackageGrants(database,companyId)).find(grant=>grant.packageId===packageId);
  return packageGrantStore(database,{authorize:async()=>({userId:"isolated-finance-purchase-review"}),obligations:async()=>{throw Error("Finance withdrawal requires its real owner witness");}}).apply({expectedVersion:prior?.version??0,idempotencyKey:crypto.randomUUID(),grant:{companyId,packageId,packageVersion:1,state,startsAt:0,endsAt:null,version:(prior?.version??0)+1,reason:"Reviewed disposable Finance lifecycle",evidence:"test:finance-advanced-purchase"}});
}
