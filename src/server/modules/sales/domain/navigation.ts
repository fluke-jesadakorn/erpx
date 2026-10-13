import { hasPermission } from "@/lib/identity_access/permissions";
import type { PermissionActor } from "@/lib/identity_access/iam";

export type HubQuery = Record<string, string | string[] | undefined>;
export type SalesView = "today" | "pipeline" | "documents" | "pricing" | "reports";
export type CustomersView = "directory" | "contacts" | "support" | "reviews";
export type CustomerReview = "identity" | "duplicates" | "access";
export const salesViews = [["today", "Today"], ["pipeline", "Pipeline"], ["documents", "Sales documents"]] as const;
export const customersViews = [["directory", "Customers"], ["contacts", "Prospects & contacts"], ["support", "Service"]] as const;
export const customerReviews = [["identity", "Identity changes"], ["duplicates", "Duplicates"], ["access", "Customer access"]] as const;
export function queryValue(query: HubQuery, key: string, fallback = "") {
  return typeof query[key] === "string" ? query[key] : fallback;
}
export function hubHref(base: string, hub: "sales" | "customers", query: HubQuery = {}, changes: HubQuery = {}) {
  const params = new URLSearchParams();
  for (const [key, value] of Object.entries({ ...query, ...changes })) if (typeof value === "string") params.set(key, value);
  return `${base}/${hub}${params.size ? `?${params}` : ""}`;
}
export function salesCustomerAccess(actor: PermissionActor & { branchId?: string | null; departmentId?: string | null }, apps?: readonly string[]) {
  const enabled = (id: string) => !apps || apps.includes(id);
  const read = hasPermission(actor, "commerce.read"), crm = read && enabled("crm"), sales = read && enabled("sales");
  return {
    today: crm || sales, pipeline: crm, documents: sales,
    pricing: sales && hasPermission(actor, "commerce.sales") && !actor.branchId && !actor.departmentId,
    reports: crm,
    directory: read && (enabled("crm") || enabled("sales") || enabled("customer_360")), contacts: crm,
    support: enabled("support") && (hasPermission(actor, "support.read") || hasPermission(actor, "records.read")),
    reviews: enabled("customer_authority") && read && hasPermission(actor, "commerce.sales"),
  };
}
/** Only list routes move. Canonical owner records and commands remain stable. */
export function salesCustomerLegacyDestination(base: string, route: string, query: HubQuery) {
  const q = { ...query };
  if (route === "crm") return hubHref(base, "sales", q, { view: q.from || q.to ? "reports" : "pipeline" });
  if (route === "customer-followups") return hubHref(base, "sales", q, { view: "today", kind: "followups", owner: "team", status: "all", ...(q.orderId ? { new: "1" } : {}) });
  if (route === "support") return hubHref(base, "customers", q, { view: "support", ...(q.orderId || q.sourceId ? { new: "1" } : {}) });
  const review = ({ "customer-identity": "identity", "customer-duplicates": "duplicates", "customer-access": "access" } as const)[route as "customer-identity"];
  if (review) return hubHref(base, "customers", q, { view: "reviews", review, ...(q.orderId ? { new: "1" } : {}) });
  if (route !== "commerce") return null;
  const view = queryValue(q, "view", "sales");
  if (view === "parties" && queryValue(q, "kind", "customer") === "customer") return hubHref(base, "customers", q, { view: "directory", kind: undefined });
  if (view === "pricing") return hubHref(base, "sales", q, { view: "pricing" });
  if (view === "sales" || view === "quote") return hubHref(base, "sales", q, { view: "documents", type: view });
  return null;
}

