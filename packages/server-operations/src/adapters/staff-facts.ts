import { finalRootOwnerProjectionSources as organizationProjectionSources } from "@erpx/server-organization/public/final-root-owner-projection-sources";
import type { SqlDatabase } from "@erpx/server-runtime/public/runtime/types";
/** Canonical active staff-level read shared by Operations owner facts.
 *  Projection and trailing filter are the only varying bytes; every rendering
 *  is byte-identical to the original inline literals. */
export const operationsActiveStaffLevels = (projection: string, tail = '') =>
  `SELECT ${projection} FROM (${organizationProjectionSources.staff_levels}) staff_levels WHERE company_id=? AND active=1${tail}`;
/** Native staff level facts for legacy route eligibility. */
export async function activeStaffLevelRanks(db: SqlDatabase, companyId: string) {
  return (await db.prepare(operationsActiveStaffLevels('id,rank')).bind(companyId).all<{ id: string; rank: number }>()).results;
}
