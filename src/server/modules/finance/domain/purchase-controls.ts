export type FinancePurchaseControls = Record<string, { newWork: boolean; settleExisting: boolean; history: boolean; revision: string }>;
export function financeActionAllowed(purchases: FinancePurchaseControls | undefined, purpose: "new-work" | "settle-existing" | "history", capability = "finance.accounting") {
  if (!purchases) return true;
  const access = purchases[capability];
  return !!access && (purpose === "new-work" ? access.newWork : purpose === "settle-existing" ? access.settleExisting : access.history);
}
