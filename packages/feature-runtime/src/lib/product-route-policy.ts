import { workforceSelfServiceEntry } from "@erpx/server-people_payroll/public/rules/self-service-product-entry";
import { socialPostSourceEntry } from "@erpx/server-collaboration/public/rules/product-entry";
import { destinationProduct, product, type ProductHostKey } from "@erpx/server-commercial/public/rules/product-registry";
import { officeApiAllowed, posStartupApiAllowed, posInitialPhysicalStockApiAllowed, supportTicketPortalApiAllowed } from "@erpx/feature-runtime/lib/product-api-policy";

/** Presentation boundaries supplement, and never replace, owner API/record policy. */
export function productPageAllowed(host: ProductHostKey, pathname: string, method = "GET", registeredAppsPages: readonly string[] = []) {
  if (host === "apps" && (method === "GET" || method === "HEAD") && /^\/applications\/[a-z0-9-]+$/.test(pathname) && registeredAppsPages.includes(pathname)) return true;
  if (pathname === "/" || pathname.startsWith("/sso/")) return true;
  if (pathname === "/public/launch") return host === "portal" || host === "legal-portal";
  if (pathname === "/account/continue") return host === "apps" || host === "admin" || product(host) !== undefined;
  if (pathname === "/account/device-return") return host === "account";
  if (["/api/v1/account/switcher-context", "/api/v1/account/role-selection"].includes(pathname)) return (host === "apps" || host === "admin" || product(host) !== undefined) && (pathname.endsWith("switcher-context") ? method === "GET" : method === "POST");
  if (pathname === "/legal-portal/continue") return host === "account";
  if (pathname === "/legal-portal/enroll") return host === "account" || host === "legal-portal";
  if (["/my-access", "/security", "/my-profile", "/account/launch"].includes(pathname)) return host === "account";
  if (["/admin/launch", "/admin/choose-work-role", "/provider"].includes(pathname)) return host === "admin";
  if (["/launch", "/choose-work-role"].includes(pathname) && (product(host)?.shell === "office" || host === "pos" || host === "pms")) return true;
  if (["/provider/commercial", "/provider/commercial/packages"].includes(pathname)) return host === "admin";
  if (pathname.startsWith("/api/")) {
    const supportPortal = supportTicketPortalApiAllowed(host, pathname, method);
    if (supportPortal !== null) return supportPortal;
    if (pathname === "/api/legal-portal/enroll") return host === "account";
    if (pathname === "/api/v1/account-invitations" || pathname === "/api/v1/join") return host === "account";
    if (pathname === "/api/pos/stripe/webhook") return host === "pos";
    if (pathname === "/api/v1/stripe/webhook") return host === "admin";
    if (pathname === "/api/v1/account/preferences") return true;
    if (pathname === "/api/v1/account/time-preference" && (method === "GET" || method === "HEAD")) return true;
    if (pathname === "/api/v1/platform/commercial" || pathname.startsWith("/api/v1/platform/commercial/")) return host === "admin";
    // Admin hosts the existing provider controllers. Their current Platform
    // and, where required, ERPX Management authority still authorizes each call.
    if (/^\/api\/v1\/platform\/(?:iam|operations|executive|billing)(?:\/|$)/.test(pathname)
      || pathname === "/api/v1/platform/assignments/active" || pathname === "/api/v1/platform/context"
      || /^\/api\/v1\/platform\/page-layouts\/[^/]+$/.test(pathname)) return host === "admin";
    if (pathname.startsWith("/api/v1/platform/")) return false;
    if (/^\/api\/v1\/holdings\/[^/]+\/(?:manage-erpx|erpx-management)(?:\/|$)/.test(pathname))
      return host === "admin" && /^\/api\/v1\/holdings\/erpx\/(?:manage-erpx|erpx-management)(?:\/|$)/.test(pathname);
    if (host === "booking") return pathname.startsWith("/api/public/booking/") || pathname.startsWith("/api/public/holdings/");
    if (host === "portal") return pathname.startsWith("/api/customer-portal/") || pathname.startsWith("/api/auth/");
    if (host === "legal-portal") return pathname.startsWith("/api/legal-portal/") || pathname.startsWith("/api/auth/");
    if (host === "forms") return pathname.startsWith("/api/public/marketing/") || /^\/api\/public\/holdings\/[^/]+\/media-publications\/[^/]+\/content$/.test(pathname);
    if (host === "account") return pathname.startsWith("/api/auth/") || pathname.startsWith("/api/v1/account/")
      || pathname === "/api/v1/account-invitations" || pathname === "/api/v1/join";
    // Front retains its independent daily parser after this transport boundary.
    // Do not expose Office API families merely because Front is a product host.
    if (product(host)?.shell === "front") {
      if (productFrontBackofficeAllowed(host, pathname, method)) return true;
      if (pathname === "/api/v1/front-access/quick-access") return true;
      if (pathname === "/api/v1/account/work-context") return method === "GET";
      if (pathname === "/api/v1/workspace-preference") return method === "POST";
      if (pathname === "/api/auth/get-session") return method === "GET" || method === "HEAD";
      if (pathname === "/api/auth/sign-out") return method === "POST";
      return /^\/api\/v1\/front\/[^/]+\/[^/]+\/(pos|pms)(?:\/|$)/.exec(pathname)?.[1] === host;
    }
    return officeApiAllowed(host, pathname, method);
  }
  if (host === "booking") return /^\/book\/[^/]+(?:\/|$)/.test(pathname);
  if (host === "portal") return /^\/customer-portal\/[^/]+(?:\/|$)/.test(pathname) || pathname === "/login";
  if (host === "legal-portal") return /^\/legal-portal\/[^/]+(?:\/|$)/.test(pathname) || pathname === "/login";
  if (host === "forms") return /^\/forms\/[^/]+\/[^/]+\/?$/.test(pathname);
  if (host === "account") return ["/login", "/signup", "/activate", "/join", "/forgot-password", "/verify-email", "/reset-password", "/my-access", "/security", "/my-profile", "/account/launch"].includes(pathname);
  if (pathname === "/applications") return host === "apps";
  if (pathname === "/login") return true;
  if (product(host)?.shell === "front") return pathname.startsWith("/holdings/")
    ? productFrontBackofficeAllowed(host, pathname, method) : true; // Native daily paths retain their separate parser.
  const parts = pathname.split("/").filter(Boolean);
  if (parts[0] !== "holdings" || !parts[1]) return false;
  let destination = parts.slice(2);
  if (destination[0] === "branches") destination = destination.slice(2);
  if (destination[0] === "departments") destination = destination.slice(2);
  if (host === "admin") return destination[0] === "administration"
    || parts[1] === "erpx" && ["erpx-management", "manage-erpx"].includes(parts[2]);
  const ownerPath = destination.join("/");
  if (host === "apps" && (socialPostSourceEntry(ownerPath) || workforceSelfServiceEntry(ownerPath))) return true;
  return destinationProduct(ownerPath) === host;
}

/** Only the new product Front hosts share their original owner backoffice.
 * Legacy Front hosts still use their unchanged daily-command allowlist. */
export function productFrontBackofficeAllowed(host: ProductHostKey | null, pathname: string, method = "GET") {
  if (host !== "pos" && host !== "pms") return false;
  const api = pathname.startsWith("/api/v1/holdings/");
  const initialStock = posInitialPhysicalStockApiAllowed(host, pathname, method);
  if (initialStock !== null) return initialStock;
  const startup = posStartupApiAllowed(host, pathname, method);
  if (startup !== null) return startup;
  const prefix = api ? "/api/v1/holdings/" : "/holdings/";
  if (!pathname.startsWith(prefix)) return false;
  const parts = pathname.slice(prefix.length).split("/").filter(Boolean);
  if (!parts.shift()) return false;
  if (parts[0] === "branches") parts.splice(0, 2);
  if (parts[0] === "departments") parts.splice(0, 2);
  const destination = parts.join("/");
  if (!api) return destination === host || destination.startsWith(host + "/");
  if (destination === host || destination.startsWith(host + "/")) return true;
  if (host === "pms" && /^(?:hospitality\/setup|hospitality-tax-documents)(?:\/|$)/.test(destination)) return true;
  // Shared transport endpoints retain their own current IAM and record policy.
  // No Finance, provider, Organization or IAM management routes are exposed.
  return /^(?:navigation|search|push|assignments\/active|records\/search)$/.test(destination)
    || /^(?:money|attachment-sets|attachment-links|attachments|uploads|profiles|assistant)(?:\/|$)/.test(destination)
    || new RegExp("^flows/" + host + "(?:_|/)").test(destination)
    || new RegExp("^records/" + host + "(?:/|$)").test(destination);
}
