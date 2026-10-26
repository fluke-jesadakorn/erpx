import { destinationProduct, type ProductHostKey } from "@erpx/server-commercial/public/product-registry";
import { canonicalUiHref, workspacePathParts } from "@erpx/feature-work_projection/lib/destination-registry";
import { configuredProductHandoff, configuredProductOrigin, trustedProductOrigin } from "@erpx/feature-runtime/lib/product-hosts";
import { consoleOrigin } from "@erpx/feature-runtime/lib/erpx-hosts";
import {holdingRoutingEnabled,holdingUiUrl,holdingPublicUiUrl} from '@erpx/contracts/holding-hosts';

/** Link construction only. Every destination retains its normal session,
 * Company, assignment, purchase and record checks. Console is a legacy origin. */
export function applicationOrigin(owner: ProductHostKey, legacy = consoleOrigin()?.origin): string | undefined {
  const configured = consoleOrigin()?.origin === "https://console-dev-erpx.jesadakorn.com" ? configuredProductOrigin(owner) : null;
  // Explicit origins also occur in isolated fixtures and separately configured
  // workers. Never move their tokens/returns into the installed development realm.
  if (!configured || !legacy) return legacy;
  const oldOrigin = new URL(legacy).origin;
  return oldOrigin === consoleOrigin()?.origin || trustedProductOrigin(oldOrigin) ? configured : legacy;
}

export function appsOrigin() {
  const value = applicationOrigin("apps");
  return value ? new URL(value) : null;
}

/** Re-enter through Account when leaving another host; a previously signed-in
 * Apps cookie must not silently select a different account's session. */
export function appsEntryUrl(): string | undefined {
  if(holdingRoutingEnabled(process.env))return '/';
  const origin = applicationOrigin("apps");
  return origin === configuredProductOrigin("apps") ? configuredProductHandoff("apps")! : origin;
}

export function accountOrigin() {
  const value = applicationOrigin("account");
  return value ? new URL(value) : null;
}

/** A provider return selects a Company through ordinary Admin SSO/launch.
 * The hint cannot supply Company, Platform, IAM or payment authority. */
export function adminCompanyHandoff(companyId: string, pathname: string, legacy = consoleOrigin()?.origin): string {
  if(holdingRoutingEnabled(process.env)){
    const mapped=holdingUiUrl(canonicalUiHref(pathname),companyId);
    if(mapped)return mapped;
  }
  const origin = applicationOrigin("admin", legacy);
  if (!origin) throw new Error("APPLICATION_ORIGIN_REQUIRED");
  const target = canonicalUiHref(pathname);
  if (origin === configuredProductOrigin("admin")) {
    const next = "/admin/launch?" + new URLSearchParams({ companyId, next: target });
    return configuredProductHandoff("admin", next)!;
  }
  return new URL(target, origin).href;
}

/** Pending recovery jobs retain their original encrypted token and expiry.
 * Only known Console recovery links move; third-party URLs are never adopted. */
export function accountRecoveryDeliveryUrl(value: string): string {
  const account = configuredProductOrigin("account"), legacy = consoleOrigin();
  if (!account || legacy?.origin !== "https://console-dev-erpx.jesadakorn.com") return value;
  const url = new URL(value);
  if (url.origin !== legacy.origin || url.username || url.password
    || !/^\/(?:api\/auth\/reset-password(?:\/[^/]+)?|reset-password)$/.test(url.pathname)) return value;
  const result = new URL(url.pathname + url.search + url.hash, account);
  for (const key of ["callbackURL", "redirectTo"]) {
    const next = result.searchParams.get(key);
    if (!next) continue;
    const destination = new URL(next, account);
    if (destination.origin === legacy.origin && destination.pathname === "/reset-password") {
      result.searchParams.set(key, new URL(destination.pathname + destination.search, account).href);
    }
  }
  return result.href;
}

/** Resolve old page links without treating Console as a shared destination.
 * Set pathname separately so even a double slash can never change the host. */
export function applicationUrl(pathname = "/", search = "") {
  if(holdingRoutingEnabled(process.env)){
    const target=canonicalUiHref(pathname+search),mapped=holdingUiUrl(target)??holdingPublicUiUrl(target);
    if(mapped)return new URL(mapped);
  }
  let owner: ProductHostKey = "apps";
  if (/^\/(?:login|signup|activate|join|forgot-password|reset-password|verify-email|my-profile|my-access|security|account)(?:\/|$)/.test(pathname)
    || /^\/legal-portal\/(?:enroll|continue)(?:\/|$)/.test(pathname)) owner = "account";
  else if (/^\/provider(?:\/|$)/.test(pathname) || /^\/platform(?:\/|$)/.test(pathname)) owner = "admin";
  else if (/^\/book(?:\/|$)/.test(pathname)) owner = "booking";
  else if (/^\/customer-portal(?:\/|$)/.test(pathname)) owner = "portal";
  else if (/^\/legal-portal(?:\/|$)/.test(pathname)) owner = "legal-portal";
  else if (/^\/forms(?:\/|$)/.test(pathname)) owner = "forms";
  else {
    const parts = workspacePathParts(new URL(canonicalUiHref(pathname), "https://path.invalid").pathname);
    if (parts) owner = /^(?:administration|erpx-management|manage-erpx)(?:\/|$)/.test(parts.path)
      ? "admin" : destinationProduct(parts.path) ?? "apps";
  }
  const base = applicationOrigin(owner);
  if (!base) throw new Error("APPLICATION_ORIGIN_REQUIRED");
  const result = new URL(base);
  // Keep legacy paths when separation is disabled in an installed environment.
  result.pathname = configuredProductOrigin(owner) ? new URL(canonicalUiHref(pathname), "https://path.invalid").pathname : pathname;
  result.search = search;
  return result;
}
