import type { PermissionActor } from "@/lib/identity_access/iam";
import { hasPermission } from "@/lib/identity_access/permissions";

/** Destinations shared with PMS, gated by both the user's access and rollout. */
export function pmsCrossLinks(actor: PermissionActor, enabledModules?: ReadonlySet<string>) {
  const allowed = (module: string, permission: string) =>
    (!enabledModules || enabledModules.has(module)) && hasPermission(actor, permission);
  return {
    inventory: allowed("inventory", "commerce.read"),
    finance: allowed("finance", "finance.read"),
    pos: allowed("pos", "pos.read"),
  };
}
