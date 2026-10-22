import { productPageAllowed, productFrontBackofficeAllowed } from "@erpx/feature-runtime/lib/product-route-policy";
import { parseHoldingWorkspace } from "@erpx/feature-work_projection/lib/workspace";
import { product, type ProductHostKey } from "@erpx/server-commercial/public/rules/product-registry";
import { safeReturnPath } from "@erpx/server-identity_access/domain/access-problem";

/** Company selection carries an identifier, never an authorization assertion. */
export function productWorkspaceTarget(audience: ProductHostKey, companyId: string | null, value: string | null) {
  const target = safeReturnPath(value);
  if (!companyId || companyId.length > 128 || !target) return null;
  const pathname = new URL(target, "https://erpx.invalid").pathname;
  if (product(audience)?.shell !== "office" && !productFrontBackofficeAllowed(audience, pathname)) return null;
  if (!productPageAllowed(audience, pathname)) return null;
  try {
    const parts = pathname.split("/").filter(Boolean).map(decodeURIComponent);
    const parsed = parts[0] === "holdings" ? parseHoldingWorkspace(parts[1], parts.slice(2)) : null;
    return parsed ? { companyId, target, scope: parsed.scope } : null;
  } catch { return null; }
}
