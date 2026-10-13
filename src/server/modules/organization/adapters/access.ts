import "server-only";
import { hasPermission } from "@/lib/identity_access/permissions";
import type { OrganizationNode } from "@/lib/organization/organization";
import type { OrganizationAccessData } from "@/lib/organization/organization-access";
import type { WorkContext } from "@/server/modules/identity_access/public/authorization";
import type { SqlDatabase } from "@/server/modules/runtime/public/runtime/types";
import { ApiError } from "@/server/modules/runtime/public/http";
import { organizationRoster } from "../public/organization";
import { BusinessRuleError } from "../../shared";
import { createOrganizationAccessApplication } from "../application/access";

export async function organizationAccessData(db: SqlDatabase, actor: WorkContext): Promise<OrganizationAccessData> {
  const application = createOrganizationAccessApplication({ actorId: actor.userId, branchId: actor.branchId, departmentId: actor.departmentId, scopeKind: actor.scope.kind, assignmentScope: actor.assignmentScope, mayReadIam: hasPermission(actor, "iam.read"), mayManagePeople: hasPermission(actor, "people.manage"), mayManageOrganization: hasPermission(actor, "organization.manage") }, {
    roster: () => organizationRoster(db, actor.companyId),
    async nodes() { return (await db.prepare(`SELECT id,kind,slug,display_name,branch_id,status,hierarchy_revision
      FROM organization_nodes WHERE company_id=? AND kind IN ('company','branch','department')
      AND (?::text IS NULL OR kind='company' OR id=? OR branch_id=?)
      AND (?::text IS NULL OR kind!='department' OR id=?) ORDER BY display_name,id`).bind(actor.companyId, actor.branchId, actor.branchId, actor.branchId, actor.departmentId, actor.departmentId).all<OrganizationNode>()).results; },
    async levels() { return (await db.prepare("SELECT id,name,level_number FROM staff_levels WHERE company_id=? AND active=1 AND level_number IS NOT NULL ORDER BY level_number").bind(actor.companyId).all<OrganizationAccessData["levels"][number]>()).results; },
  }, { now: () => Date.now() });
  try { return await application.read(); }
  catch (error) { if (error instanceof BusinessRuleError) throw new ApiError(error.status, error.code, error.message); throw error; }
}
