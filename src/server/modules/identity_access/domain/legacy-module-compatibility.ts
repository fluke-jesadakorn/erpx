import { hasPermission } from "@/lib/identity_access/permissions";
import type { PermissionActor } from "@/lib/identity_access/iam";

export type LegacyModuleCompatibility = {
  label: string;
  modules: string[];
  source: "published-permission-policy/v1";
};

/** License/navigation evidence for the three historical group labels. This never changes
 * an assignment, the acting Work modules, or an owner/record authorization. */
export function legacyModuleCompatibility(actor: PermissionActor, modules: readonly string[]): LegacyModuleCompatibility[] {
  const result: LegacyModuleCompatibility[] = [];
  const record = (label: string, owners: string[]) => {
    if (modules.includes(label) && owners.length) result.push({ label, modules: owners, source: "published-permission-policy/v1" });
  };
  record("iam", hasPermission(actor, "iam.read") ? ["identity_access"] : []);
  record("people", hasPermission(actor, "module.people_payroll.read") ? ["people_payroll"] : []);
  // The old Commerce workspace exposed these owner operations. Require both
  // the published module opening and its actual business permission; a group
  // label or a generic business read alone supplies no purchase evidence.
  record("commerce", hasPermission(actor, "commerce.read") ? [
    ...(hasPermission(actor, "module.sales.read") && hasPermission(actor, "commerce.sales") ? ["sales"] : []),
    ...(hasPermission(actor, "module.procurement.read") && hasPermission(actor, "commerce.purchase") ? ["procurement"] : []),
    ...(hasPermission(actor, "module.inventory.read") && hasPermission(actor, "inventory.read") ? ["inventory"] : []),
  ] : []);
  return result;
}
