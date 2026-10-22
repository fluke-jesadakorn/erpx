import "server-only";
import { holdingResources } from "../../../../packages/server-runtime/src/public/runtime";
import { ApiError } from "@erpx/server-runtime/public/http";
import type { AccountAccessContext } from "@erpx/server-identity_access/domain/authorization";
import { effectiveWorkModules } from "@erpx/server-identity_access/domain/work-modules";
import { loadIamAuthority } from "@erpx/server-identity_access/adapters/iam-store";
import { betaAccess } from "@erpx/server-identity_access/adapters/beta-access";
import { legacyModuleCompatibility } from "@erpx/server-identity_access/domain/legacy-module-compatibility";
import type { ProductModuleAccess } from "@erpx/server-identity_access/domain/product-module-access";

/** Private render projection; published custom roles and HR appointments retain owner semantics. */
export async function accountWorkModuleAccess(access: AccountAccessContext): Promise<ProductModuleAccess> {
  const { write } = await holdingResources(access.scope.holdingId);
  const beta = await betaAccess(write, access.scope.companyId, new Headers());
  const byRole = new Map<string, Awaited<ReturnType<typeof loadIamAuthority>>>();
  const modulesByAssignment = new Map<string, readonly string[]>();
  const presentationModulesByAssignment = new Map<string, readonly string[]>();
  const permissionsByAssignment = new Map<string, readonly string[]>();
  for (const assignment of access.assignments) {
    try {
      let authority = byRole.get(assignment.role);
      if (!authority) {
        authority = await loadIamAuthority(write, access.scope.companyId, access.userId, assignment.role, access.sessionId);
        byRole.set(assignment.role, authority);
      }
      const modules = effectiveWorkModules(assignment.role, assignment.modules, authority.permissions, beta);
      modulesByAssignment.set(assignment.id, modules);
      permissionsByAssignment.set(assignment.id, authority.permissions);
      const compatibility = beta ? [] : legacyModuleCompatibility({ role: assignment.role, permissions: authority.permissions,
        assignmentScope: assignment.scopeKind }, modules);
      presentationModulesByAssignment.set(assignment.id, [...new Set([...modules, ...compatibility.flatMap(proof => proof.modules)])]);
    } catch (error) {
      if (error instanceof ApiError && [401,403].includes(error.status)) continue;
      throw error;
    }
  }
  return { modulesByAssignment, presentationModulesByAssignment, permissionsByAssignment, beta };
}
