import { finalRootOwnerProjectionSources as organizationProjectionSources } from "../../organization/public/final-root-owner-projection-sources";
import type { SqlDatabase } from "@/server/modules/runtime/public/runtime/types";
/** Native staff level facts for legacy route eligibility. */
export async function activeStaffLevelRanks(db: SqlDatabase, companyId: string) {
  return (await db.prepare(`SELECT id,rank FROM (${organizationProjectionSources.staff_levels}) staff_levels WHERE company_id=? AND active=1`).bind(companyId).all<{ id: string; rank: number }>()).results;
}
