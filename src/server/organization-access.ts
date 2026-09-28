import "server-only";
import { hasPermission } from "@/lib/permissions";
import { reportingIssues, type OrganizationNode } from "@/lib/organization";
import type { OrganizationAccessData } from "@/lib/organization-access";
import type { WorkContext } from "./authorization";
import type { SqlDatabase } from "./runtime/types";
import { ApiError } from "./http";
import { organizationRoster } from "./organization";

export async function organizationAccessData(db: SqlDatabase, actor: WorkContext): Promise<OrganizationAccessData> {
  if (!["iam.read", "people.read", "people.manage", "organization.manage"].some(permission => hasPermission(actor, permission))) {
    throw new ApiError(403, "PERMISSION_REQUIRED", "Your selected role cannot view organization and access.");
  }
  const [{ people, positions }, nodeRows, levelRows] = await Promise.all([
    organizationRoster(db, actor.companyId),
    db.prepare(`SELECT id,kind,slug,display_name,branch_id,status,hierarchy_revision
      FROM organization_nodes WHERE company_id=? AND kind IN ('company','branch','department')
      AND (?::text IS NULL OR kind='company' OR id=? OR branch_id=?)
      AND (?::text IS NULL OR kind!='department' OR id=?) ORDER BY display_name,id`)
      .bind(actor.companyId, actor.branchId, actor.branchId, actor.branchId, actor.departmentId, actor.departmentId)
      .all<OrganizationNode>(),
    db.prepare("SELECT id,name,level_number FROM staff_levels WHERE company_id=? AND active=1 AND level_number IS NOT NULL ORDER BY level_number")
      .bind(actor.companyId).all<OrganizationAccessData["levels"][number]>(),
  ]);
  const inScope = (item: { branch_id: string | null; department_id: string | null }) =>
    (!actor.branchId || item.branch_id === actor.branchId) && (!actor.departmentId || item.department_id === actor.departmentId);
  const company = actor.scope.kind === "holding" && actor.assignmentScope === "holding" && !actor.branchId && !actor.departmentId;
  return {
    positions: positions.filter(position => position.active === 1 && inScope(position)),
    people: people.filter(inScope).map(person => ({ ...person, issues: reportingIssues(person, people, positions) })),
    nodes: nodeRows.results,
    levels: levelRows.results,
    canManage: company && hasPermission(actor, "organization.manage") && hasPermission(actor, "people.manage"),
    canMovePeople: company && hasPermission(actor, "people.manage"),
    actorId: actor.userId,
  };
}
