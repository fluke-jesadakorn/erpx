export const servicePartsSetupLanding = "services/field-service/settings/parts";
/** Presentation prefilter only. The Commerce owner proves current purchase,
 * source, release, assignment and session before selecting this destination. */
export function servicePartsSetupEntry(destination: string) {
  return destination === servicePartsSetupLanding;
}
export function servicePartsPresentationAllowed(context: { scopeKind: string; beta?: boolean; permissions?: readonly string[] }) {
  if (context.beta || !["holding", "branch"].includes(context.scopeKind)) return false;
  const permissions = context.permissions ?? [];
  return context.scopeKind === "holding" && permissions.includes("finance.read")
    && ["finance.write", "finance.approve"].some(permission => permissions.includes(permission))
    || permissions.includes("inventory.read") && permissions.includes("inventory.prepare");
}
