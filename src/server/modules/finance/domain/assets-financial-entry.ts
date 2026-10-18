export const assetsFinancialLanding = "services/assets/accounting";
export type AssetsFinancialEntry = { assetId?: string };
/** The workspace parser supplies decoded path segments. Reject residual escapes
 * and extra selectors instead of treating them as another Finance destination. */
export function assetsFinancialEntry(destination: string): AssetsFinancialEntry | null {
  if (destination === assetsFinancialLanding) return {};
  if (!destination.startsWith(assetsFinancialLanding + "/")) return null;
  const assetId = destination.slice(assetsFinancialLanding.length + 1);
  return assetId && !/[\\/%?#\u0000-\u001f\u007f]/.test(assetId) && ![".", ".."].includes(assetId) ? { assetId } : null;
}
export function assetsFinancialPresentationAllowed(context: { scopeKind: string; beta: boolean; permissions?: readonly string[] }) {
  return !context.beta && context.scopeKind === "holding" && context.permissions?.includes("finance.read") === true;
}
