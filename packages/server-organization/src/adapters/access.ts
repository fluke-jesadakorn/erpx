import "server-only";
import { hasPermission } from "@erpx/feature-identity_access/lib/permissions";
import type { OrganizationNode } from "@erpx/feature-organization/lib/organization";
import type { OrganizationAccessData } from "@erpx/feature-organization/lib/organization-access";
import type { WorkContext } from "@erpx/server-identity_access/public/authorization";
import type { SqlDatabase } from "@erpx/server-runtime/public/runtime/types";
import { ApiError } from "@erpx/server-runtime/public/http";
import {organizationRoster} from '@erpx/server-organization/public/organization-roster-read';
import { BusinessRuleError } from "@erpx/server-shared/index";
import { createOrganizationAccessApplication } from "@erpx/server-organization/application/access";
import { nativeOrgAll } from './native-organization-owner-write';
// Both reads are owner-local Company-keyed SELECTs over org-owned tables.
// SQL is authoritative in erpx-server-organization::adapters::organization_owner_write
// and reached only by typed kind with positional values on the caller's unit.
// The `?::text IS NULL` branch/department windows keep their null-scope meaning.

export async function organizationAccessData(db: SqlDatabase, actor: WorkContext): Promise<OrganizationAccessData> {
  const application = createOrganizationAccessApplication({ actorId: actor.userId, branchId: actor.branchId, departmentId: actor.departmentId, scopeKind: actor.scope.kind, assignmentScope: actor.assignmentScope, mayReadIam: hasPermission(actor, "iam.read"), mayManagePeople: hasPermission(actor, "people.manage"), mayManageOrganization: hasPermission(actor, "organization.manage") }, {
    roster: () => organizationRoster(db, actor.companyId),
    nodes: () => nativeOrgAll<OrganizationNode>(db, actor.companyId, 'org_access_read_nodes', [actor.companyId, actor.branchId, actor.branchId, actor.branchId, actor.departmentId, actor.departmentId]),
    levels: () => nativeOrgAll<OrganizationAccessData["levels"][number]>(db, actor.companyId, 'org_access_read_levels', [actor.companyId]),
  }, { now: () => Date.now() });
  try { return await application.read(); }
  catch (error) { if (error instanceof BusinessRuleError) throw new ApiError(error.status, error.code, error.message); throw error; }
}
