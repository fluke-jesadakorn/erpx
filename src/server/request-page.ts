import type { SqlDatabase } from "./runtime/types";
import type { WorkContext } from "./authorization";
import { activeStaff } from "./staff-policy";
import { ApiError } from "./http";
import { inServiceWorkspace } from "./service-access";
export async function requestPageOptions(db: SqlDatabase, actor: WorkContext) {
  const nodes = (
    await db
      .prepare(
        "SELECT id,kind,display_name,branch_id FROM organization_nodes WHERE company_id=? AND status='active' AND kind IN ('company','branch','department') AND (?::text IS NULL OR id=? OR branch_id=?) AND (?::text IS NULL OR kind='branch' OR id=?) ORDER BY display_name",
      )
      .bind(
        actor.companyId,
        actor.branchId,
        actor.branchId,
        actor.branchId,
        actor.departmentId,
        actor.departmentId,
      )
      .all<{
        id: string;
        kind: string;
        display_name: string;
        branch_id: string | null;
      }>()
  ).results;
  let branchId = actor.branchId,
    departmentId = actor.departmentId;
  try {
    const staff = await activeStaff(db, actor.companyId, actor.userId);
    if (inServiceWorkspace(actor, staff)) {
      branchId = staff.branch_id;
      departmentId = staff.department_id;
    }
  } catch (e) {
    if (!(e instanceof ApiError) || e.status !== 409) throw e;
  }
  return {
    nodes,
    branchId,
    departmentId,
    companyId: actor.companyId,
    userId: actor.userId,
  };
}
