export type CommerceOrderKind = "quote" | "sales" | "purchase";
export type CommercePurchaseControls = {
  newWork: boolean; settleExisting: boolean; history: boolean; revision: string;
  convertNewWork: boolean;
  stockNewWork: boolean; stockSettleExisting: boolean; financeSettleExisting: boolean; receivingSettleExisting: boolean;
};
/** Shared item/party/warehouse operations have their own owner authority. */
export function commerceCommandPurpose(path: string, input: unknown): "new-work" | "settle-existing" | null {
  if (path === "orders" || /^orders\/[^/]+$/.test(path)) return "new-work";
  const value = input && typeof input === "object" ? input as Record<string, unknown> : {};
  if (path === "customer-notifications/settings") return value.adapter === "disabled" ? "settle-existing" : "new-work";
  if (/^customer-notifications\/[^/]+\/(?:retry|reconcile)$/.test(path)) return "settle-existing";
  if (/^fulfillments\/[^/]+\/notify$/.test(path)) return "new-work";
  if (/^orders\/[^/]+\/transition$/.test(path)) return ["cancel", "return"].includes(String(value.action)) ? "settle-existing" : "new-work";
  if (/^orders\/[^/]+\/reservations$/.test(path)) return value.action === "release" ? "settle-existing" : "new-work";
  return /^(?:orders\/[^/]+\/fulfill|fulfillments\/[^/]+\/(?:invoice|returns)|returns\/[^/]+\/(?:approve|reject|complete)|invoices\/[^/]+\/settle)$/.test(path) ? "settle-existing" : null;
}
export function commerceCommandAllowed(controls: CommercePurchaseControls | undefined, path: string, input: unknown) {
  const purpose = commerceCommandPurpose(path, input);
  if (!controls || !purpose) return true;
  if (!(purpose === "new-work" ? controls.newWork : controls.settleExisting)) return false;
  if (/\/transition$/.test(path) && input && typeof input === "object" && "action" in input && input.action === "convert") return controls.convertNewWork;
  if (/\/reservations$/.test(path)) return purpose === "new-work" ? controls.stockNewWork : controls.stockSettleExisting;
  if (/\/fulfill$/.test(path)) {
    const mode = input && typeof input === "object" && "completionMode" in input ? input.completionMode : undefined;
    return controls.financeSettleExisting && controls.receivingSettleExisting && (mode === "service" || controls.stockSettleExisting);
  }
  if (/^returns\/[^/]+\/approve$/.test(path)) return controls.stockSettleExisting && controls.financeSettleExisting;
  if (/\/(?:invoice|settle|complete)$/.test(path)) return controls.financeSettleExisting;
  return true;
}
