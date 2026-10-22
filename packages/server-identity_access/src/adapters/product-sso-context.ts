import { configuredProductOrigin } from "@erpx/feature-runtime/lib/product-hosts";
import { productIds, sharedHostKeys } from "@erpx/contracts/product-registry";
const sessionCookieName = "__Host-erpx-product";

export function cookie(headers: Headers, name: string) {
  const values = headers.get("cookie")?.split(";").map(entry => entry.trim()).filter(entry => entry.startsWith(name + "=")) ?? [];
  if (values.length !== 1) return "";
  try { return decodeURIComponent(values[0].slice(name.length + 1)); } catch { return ""; }
}
export function productSessionCookie(headers: Headers) { return cookie(headers, sessionCookieName); }
export function ssoOrigins() {
  return Object.fromEntries([...productIds, ...sharedHostKeys].flatMap(key => {
    const origin = configuredProductOrigin(key);
    return origin ? [[key, origin]] : [];
  }));
}
