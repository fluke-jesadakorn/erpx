import { developmentProductOrigin, isProductHostKey, productIds, sharedHostKeys, type ProductHostKey } from "@/server/modules/commercial/public/product-registry";
import { safeProductReturnPath } from "@/server/modules/identity_access/public/rules/product-sso";

export type ProductHostConfiguration = { routing: boolean; sso: boolean; environment: "development" | null };
export function productHostConfiguration(): ProductHostConfiguration {
  return {
    routing: process.env.ERPX_PRODUCT_ROUTING === "enabled",
    sso: process.env.ERPX_PRODUCT_SSO === "enabled",
    environment: process.env.ERPX_PRODUCT_ENVIRONMENT === "development" && process.env.ERPX_ENVIRONMENT === "local" ? "development" : null,
  };
}
/** Exact, owned development origins only; no suffix/wildcard or future-origin trust. */
export function productHostForHost(host: string | null, config = productHostConfiguration()): ProductHostKey | null {
  if (!host || !config.routing || !config.sso || config.environment !== "development") return null;
  for (const key of [...productIds, ...sharedHostKeys]) if (new URL(developmentProductOrigin(key)).host === host) return key;
  return null;
}
export function trustedProductOrigin(value: string, config = productHostConfiguration()): string | null {
  let url: URL;
  try { url = new URL(value); } catch { return null; }
  if (url.protocol !== "https:" || url.username || url.password || url.pathname !== "/" || url.search || url.hash) return null;
  return productHostForHost(url.host, config) ? url.origin : null;
}
export function configuredProductOrigin(key: string, config = productHostConfiguration()): string | null {
  return isProductHostKey(key) ? trustedProductOrigin(developmentProductOrigin(key), config) : null;
}

/** Authenticated GET navigation only. Re-enter through the central account
 * rather than silently keeping a different account's existing child cookie. */
export function configuredProductHandoff(key: ProductHostKey, next = "/", config = productHostConfiguration()): string | null {
  const origin = configuredProductOrigin(key, config);
  if (!origin) return null;
  const target = safeProductReturnPath(next);
  if (key === "account") return new URL(target, origin).href;
  const href = new URL("/sso/start", origin);
  href.searchParams.set("next", target);
  return href.href;
}
