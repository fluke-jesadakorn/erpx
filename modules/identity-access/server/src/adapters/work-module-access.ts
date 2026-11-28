import "server-only";
import { holdingResources } from "../../../../../platform/runtime/server/src/public/runtime";
import { ApiError } from "@erpx/platform-runtime-server/public/http";
import type { AccountAccessContext } from "@erpx/identity-access-server/domain/authorization";
import { effectiveWorkModules } from "@erpx/identity-access-server/domain/work-modules";
import { loadIamAuthority } from "@erpx/identity-access-server/adapters/iam-store";
import { betaAccess } from "@erpx/identity-access-server/adapters/beta-access";
import { legacyModuleCompatibility } from "@erpx/identity-access-server/domain/legacy-module-compatibility";
import type { ProductModuleAccess } from "@erpx/identity-access-server/domain/product-module-access";

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
