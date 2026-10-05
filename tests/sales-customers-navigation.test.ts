import { expect, it } from "vitest";
import { hubHref, salesCustomerAccess, salesCustomerLegacyDestination } from "../src/lib/sales-customers";
import { hubResultHref } from "../src/lib/hub-result";
import { workplaceNavigation, navigationTrail, resolveNavigation } from "../src/lib/workplace-navigation";
import { navigationLauncher } from "../src/lib/navigation-catalog";
import { productionPageCut } from "../src/lib/hospitality-page-cut";
const base = "/holdings/acme/branches/main";

it("moves list routes while retaining filters, cursor and creation context", () => {
  const query = { q: "Harbor & sons", cursor: "cursor==", new: "1", edit: "customer-1" };
  const url = new URL(salesCustomerLegacyDestination(base, "commerce", { ...query, view: "quote" })!, "https://example.test");
  expect(url.pathname).toBe(`${base}/sales`);
  expect(Object.fromEntries(url.searchParams)).toEqual({ ...query, view: "documents", type: "quote" });
  expect(salesCustomerLegacyDestination(base, "commerce", { view: "purchase" })).toBeNull();
  expect(salesCustomerLegacyDestination(base, "commerce", { view: "parties", kind: "supplier" })).toBeNull();
  expect(salesCustomerLegacyDestination(base, "crm/lead-1", {})).toBeNull();
  const source = new URL(salesCustomerLegacyDestination(base, "support", { sourceKind: "asset", sourceId: "asset-1", sourceVersion: "3" })!, "https://example.test");
  expect(source.searchParams.get("new")).toBe("1");
  expect(source.searchParams.get("sourceVersion")).toBe("3");
});
it("opens saved owner records in the active list without losing filters", () => {
  const result = new URL(hubResultHref(`https://example.test${base}/sales?view=documents&type=quote&q=Harbor&new=1`, base, `${base}/commerce/orders/order-1?view=quote`)!, "https://example.test");
  expect(result.pathname).toBe(`${base}/sales`);
  expect(result.searchParams.get("q")).toBe("Harbor");
  expect(result.searchParams.get("new")).toBeNull();
  expect(result.searchParams.get("detailKind")).toBe("order");
  expect(result.searchParams.get("detailId")).toBe("order-1");
  expect(hubResultHref(`https://example.test${base}/crm/lead-1`, base, `${base}/crm/lead-2`)).toBeNull();
  expect(hubHref(base, "customers", { q: "A", cursor: "old" }, { cursor: undefined, view: "support" })).toBe(`${base}/customers?q=A&view=support`);
  expect(new URL(hubHref(base, "customers", { view: "reviews", status: "" }, { review: "duplicates" }), "https://example.test").searchParams.get("status")).toBe("");
});
it("shows two workspaces and keeps every auxiliary task searchable", () => {
  const model = workplaceNavigation({ workspaceBase: base, role: "holding_owner", moduleIds: ["marketing", "ecommerce"] });
  const category = model.categories.find(category => category.id === "customers-sales")!;
  expect(category.apps.map(item => item.label)).toEqual(["Sales", "Customers"]);
  expect(category.secondaryItems?.map(item => item.id)).toEqual(["marketing", "ecommerce"]);
  for (const text of ["Follow-ups", "Pipeline", "Quotations", "Price lists", "Customer access", "Support cases", "Marketing", "Channels", "Contacts", "reports"]) expect(navigationLauncher(model, text).results.length, text).toBeGreaterThan(0);
  expect(resolveNavigation(model, base, `${base}/crm/lead-1`).sidebarId).toBe("sales-hub");
  expect(resolveNavigation(model, base, `${base}/support/ticket-1`).sidebarId).toBe("customers-hub");
  expect(navigationTrail(model, base, `${base}/sales`, new URLSearchParams("view=pipeline&detailKind=crm_lead&detailId=lead-1")).map(item => item.label)).toEqual(["Sales", "Pipeline", "CRM case"]);
});
it("allows Support-only entry without registry or sales permission", () => {
  const actor = { permissions: ["support.read"] }, access = salesCustomerAccess(actor, ["support"]);
  expect(access).toMatchObject({ support: true, directory: false, contacts: false, today: false, documents: false, reviews: false });
  const model = workplaceNavigation({ workspaceBase: base, permissions: ["support.read"], availableAppIds: ["support"] });
  expect(model.categories.find(category => category.id === "customers-sales")?.apps).toMatchObject([{ id: "customers-hub", href: `${base}/customers?view=support` }]);
  expect(model.search.some(item => item.id === "crm-pipeline")).toBe(false);
});
it("checks each production view against its source rollout", () => {
  const catalogue = [{ id: "crm", name: "CRM", state: "Coming Soon" as const }, { id: "sales", name: "Sales", state: "Coming Soon" as const }, { id: "support", name: "Support", state: "Pilot" as const }];
  expect(productionPageCut(`${base}/customers`, base, new URLSearchParams(), catalogue)).toBeNull();
  expect(productionPageCut(`${base}/customers`, base, new URLSearchParams("view=support"), catalogue)).toBeNull();
  expect(productionPageCut(`${base}/sales`, base, new URLSearchParams("view=pipeline"), catalogue)).toMatchObject({ id: "crm" });
  expect(salesCustomerAccess({ permissions: ["commerce.read", "commerce.sales"], branchId: "branch" }).pricing).toBe(false);
});
