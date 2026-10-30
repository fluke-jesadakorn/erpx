import { configuredProductOrigin } from "@erpx/feature-runtime/lib/product-hosts";
import { productIds, sharedHostKeys } from "@erpx/contracts/product-registry";
import {holdingHosts,holdingRoutingEnabled,holdingAudience} from '@erpx/contracts/holding-hosts';
const sessionCookieName = "__Host-erpx-product";

export function cookie(headers: Headers, name: string) {
  const values = headers.get("cookie")?.split(";").map(entry => entry.trim()).filter(entry => entry.startsWith(name + "=")) ?? [];
  if (values.length !== 1) return "";
  try { return decodeURIComponent(values[0].slice(name.length + 1)); } catch { return ""; }
}
export function productSessionCookie(headers: Headers) { return cookie(headers, sessionCookieName); }
export function ssoOrigins() {
  if (holdingRoutingEnabled(process.env)) return Object.fromEntries([['account', configuredProductOrigin('account')!], ...holdingHosts.map(entry => [holdingAudience(entry.hostname)!, `https://${entry.hostname}`])]);
  return Object.fromEntries([...productIds, ...sharedHostKeys].flatMap(key => {
    const origin = configuredProductOrigin(key);
    return origin ? [[key, origin]] : [];
  }));
}
