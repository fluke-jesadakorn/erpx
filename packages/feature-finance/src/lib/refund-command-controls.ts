export type RefundCommandScope = {ownerId: string; instructionIds: readonly string[]};
/** Only the actual refund's server-resolved Commerce foundation may join banking. */
export function refundSourceCapabilities(source?: {capability: string}) {
  const capability = source?.capability;
  return capability && ["finance.sales-posting", "finance.inventory-valuation"].includes(capability) ? ["finance.banking",capability] : ["finance.banking"];
}
export function refundCommandWithinSource(path: string, input: unknown, scope: RefundCommandScope) {
  if (path === `refunds/${scope.ownerId}/approve` || path === `refunds/${scope.ownerId}/instructions`) return true;
  if (scope.instructionIds.some(id => path === `refund-instructions/${id}/confirm` || path === `refund-instructions/${id}/exception`)) return true;
  if (path !== "refund-instructions/export" || !input || typeof input !== "object" || !("instructions" in input) || !Array.isArray(input.instructions) || !input.instructions.length) return false;
  return input.instructions.every(value => !!value && typeof value === "object" && "id" in value && typeof value.id === "string" && scope.instructionIds.includes(value.id));
}
