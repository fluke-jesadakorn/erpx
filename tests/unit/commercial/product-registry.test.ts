import { describe, expect, it, vi } from "vitest";
import { moduleRegistry } from "@erpx/catalogue/module-registry";
import { appDefinition } from "@erpx/contracts/apps";
import { developmentProductOrigin, destinationProduct, futureProductOrigin, modulePlacement, productIds, products, sharedHostKeys, validateProductRegistry } from "@erpx/contracts/product-registry";
import { configuredProductOrigin, configuredProductHandoff, productHostConfiguration, productHostForHost, trustedProductOrigin } from "@erpx/feature-runtime/lib/product-hosts";
import { productPageAllowed } from "@erpx/feature-runtime/lib/product-route-policy";
import { officeApiAllowed, pmsProductOperationAllowed, calendarProductActionAllowed } from "@erpx/feature-runtime/lib/product-api-policy";
import { canonicalUiHref, destinations } from "@erpx/feature-work_projection/lib/destination-registry";
import { existsSync } from "node:fs";
import { resolve } from "node:path";

const enabled = { routing: true, sso: true, environment: "development" as const };
it("maps Sales posting setup through its registry without altering Finance settings or scope/query identity", () => {
  for (const scope of ["", "branches/main/", "departments/team/", "branches/main/departments/team/"]) {
    const base = `/holdings/acme/${scope}`;
    expect(canonicalUiHref(`${base}sales?view=settings&setup=posting&tag=one&tag=two#defaults`)).toBe(`${base}customers-sales/sales/settings/posting?tag=one&tag=two#defaults`);
    expect(destinationProduct("customers-sales/sales/settings/posting")).toBe("sales");
    expect(productPageAllowed("sales", `${base}customers-sales/sales/settings/posting`)).toBe(true);
    expect(productPageAllowed("supply", `${base}customers-sales/sales/settings/posting`)).toBe(false);
    expect(canonicalUiHref(`${base}finance/sales/settings?tag=one#defaults`)).toBe(`${base}finance/documents/sales/settings?tag=one#defaults`);
  }
});
describe("product registry", () => {
  it("re-enters Apps and provider navigation through central SSO while preserving the owned destination query", () => {
    expect(configuredProductHandoff("apps", "/", enabled)).toBe("https://apps-dev-erpx.jesadakorn.com/sso/start?next=%2F");
    const href = new URL(configuredProductHandoff("admin", "/provider/commercial?companyId=c2&holding=h2", enabled)!);
    expect(href.pathname).toBe("/sso/start");
    expect(href.searchParams.get("next")).toBe("/provider/commercial?companyId=c2&holding=h2");
    expect(configuredProductHandoff("account", "/", enabled)).toBe("https://account-dev-erpx.jesadakorn.com/");
    expect(configuredProductHandoff("apps", "https://elsewhere.test", enabled)).toBe("https://apps-dev-erpx.jesadakorn.com/sso/start?next=%2F");
    expect(configuredProductHandoff("apps", "/", { ...enabled, routing: false })).toBeNull();
  });
  it("maps all 33 original modules and 22 exact hosts without granting provider authority", () => {
    expect(moduleRegistry).toHaveLength(33);
    expect(validateProductRegistry(moduleRegistry.map(module => module.id))).toEqual([]);
    expect([...productIds, ...sharedHostKeys]).toHaveLength(21);
    expect(modulePlacement.platform_administration).toBe("provider");
    expect(modulePlacement.manage_erpx).toBe("provider");
  });
  it("keeps every Office landing on an existing canonical page", () => {
    for (const product of products.filter(product => product.shell === "office")) {
      expect(existsSync(resolve(appDefinition(product.id).directory, "src/app/holdings/[holdingSlug]", product.landing, "page.tsx")), product.id).toBe(true);
    }
  });
  it("uses the most specific product destination and preserves record-owner paths", () => {
    expect(destinationProduct("knowledge/learning/courses")).toBe("learn");
    expect(destinationProduct("knowledge/handbook/published")).toBe("apps");
    expect(destinationProduct("customers-sales/customers/service/cases/records/ticket/t1/summary")).toBe("support");
    expect(destinationProduct("services/legal/mine")).toBe("legal");
    expect(destinationProduct("erpx-management/billing")).toBeUndefined();
    expect(destinationProduct("insights/executive/reports/original")).toBe("apps");
    for (const destination of destinations.filter(item => !item.path.startsWith("administration") && !item.path.startsWith("erpx-management")))
      expect(destinationProduct(destination.path), destination.path).toBeDefined();
  });
  it("keeps included conversation and own workforce routes on Work without opening owner collections", () => {
    for (const path of ["posts/post1", "leave", "leave/new", "overtime", "overtime/new", "overtime/ot1/overview", "tasks/people_payroll/leave1/overview"]) {
      for (const scope of ["", "branches/main/", "branches/main/departments/it/"]) {
        expect(productPageAllowed("apps", `/holdings/acme/${scope}${path}`)).toBe(true);
        expect(productPageAllowed("finance", `/holdings/acme/${scope}${path}`)).toBe(false);
      }
    }
    for (const path of ["posts", "posts/new", "posts/post1/moderation", "tasks/people_payroll", "tasks/people_payroll/leave1/private", "overtime/ot1/approve", "leave/private"])
      expect(productPageAllowed("apps", `/holdings/acme/${path}`)).toBe(false);
  });
  it("trusts owned exact development origins only after both independent flags", () => {
    expect(developmentProductOrigin("apps")).toBe("https://apps-dev-erpx.jesadakorn.com");
    for (const key of [...productIds, ...sharedHostKeys]) {
      expect(configuredProductOrigin(key, enabled)).toBe(developmentProductOrigin(key));
      expect(productHostForHost(new URL(developmentProductOrigin(key)).host, enabled)).toBe(key);
      expect(trustedProductOrigin(futureProductOrigin(key, "development"), enabled)).toBeNull();
      expect(trustedProductOrigin(futureProductOrigin(key, "production"), enabled)).toBeNull();
      expect(trustedProductOrigin(`https://${key}.dev-erpx.jesadakorn.com`, enabled)).toBeNull();
    }
    for (const config of [{ ...enabled, routing: false }, { ...enabled, sso: false }, { ...enabled, environment: null }]) expect(configuredProductOrigin("finance", config)).toBeNull();
    for (const origin of ["http://finance-dev-erpx.jesadakorn.com", "https://finance-dev-erpx.jesadakorn.com.evil.test", "https://finance-dev-erpx.jesadakorn.com:444", "https://user@finance-dev-erpx.jesadakorn.com", "https://finance-dev-erpx.jesadakorn.com/callback", "https://shop-dev-erpx.jesadakorn.com"]) expect(trustedProductOrigin(origin, enabled)).toBeNull();
  });
  it("cannot activate development host trust in an installed environment", () => {
    vi.stubEnv("ERPX_PRODUCT_ROUTING", "enabled"); vi.stubEnv("ERPX_PRODUCT_SSO", "enabled"); vi.stubEnv("ERPX_PRODUCT_ENVIRONMENT", "development");
    try {
      for (const environment of ["production", "staging"]) {
        vi.stubEnv("ERPX_ENVIRONMENT", environment);
        expect(productHostConfiguration().environment).toBeNull();
        expect(configuredProductOrigin("account")).toBeNull();
      }
    } finally { vi.unstubAllEnvs(); }
  });
  it("keeps external hosts out of employee and provider workspaces", () => {
    for (const key of ["booking", "portal", "legal-portal", "forms"] as const) {
      expect(productPageAllowed(key, "/holdings/acme/finance/documents")).toBe(false);
      expect(productPageAllowed(key, "/api/v1/holdings/acme/finance")).toBe(false);
    }
    expect(productPageAllowed("finance", "/holdings/acme/branches/main/finance/documents")).toBe(true);
    expect(productPageAllowed("finance", "/holdings/acme/people-organization/people/directory/current")).toBe(false);
    expect(productPageAllowed("admin", "/holdings/acme/administration/brand/identity")).toBe(true);
    // Admin now hosts the original provider controller; the controller retains
    // its Platform/ERPX Management checks and never gains Company authority.
    expect(productPageAllowed("admin", "/api/v1/platform/operations/provision")).toBe(true);
    expect(productPageAllowed("admin", "/provider/commercial")).toBe(true);
    expect(productPageAllowed("admin", "/provider/commercial/packages")).toBe(true);
    expect(productPageAllowed("admin", "/api/v1/platform/commercial/legacy-grants/apply")).toBe(true);
    expect(productPageAllowed("finance", "/provider/commercial")).toBe(false);
    for (const key of [...productIds, ...sharedHostKeys].filter(key => key !== "admin")) {
      expect(productPageAllowed(key, "/provider/commercial/packages")).toBe(false);
      expect(productPageAllowed(key, "/api/v1/platform/commercial/packages/apply")).toBe(false);
    }
    expect(productPageAllowed("finance", "/api/v1/platform/commercial/legacy-grants/apply")).toBe(false);
    expect(productPageAllowed("apps", "/api/v1/platform/commercial")).toBe(false);
    expect(productPageAllowed("finance", "/api/v1/holdings/erpx/manage-erpx/iam")).toBe(false);
    // Invitation signup now belongs to the central Account cookie origin.
    // The Legal Portal GET entry continues to recover old invitation links.
    expect(productPageAllowed("legal-portal", "/api/legal-portal/enroll")).toBe(false);
  });
  it("opens exact own Account pages without requiring a Company workspace",()=>{
    for(const path of ["/my-access","/security","/my-profile","/account/launch"]){
      expect(productPageAllowed("account",path)).toBe(true);
      for(const key of [...productIds,...sharedHostKeys].filter(key=>key!=="account"))expect(productPageAllowed(key,path)).toBe(false);
    }
    for(const key of [...productIds,...sharedHostKeys])expect(productPageAllowed(key,"/public/launch")).toBe(key==="portal"||key==="legal-portal");
    expect(productPageAllowed("account","/legal-portal/enroll")).toBe(true);
    expect(productPageAllowed("legal-portal","/legal-portal/enroll")).toBe(true);
    expect(productPageAllowed("account","/legal-portal/continue")).toBe(true);
    expect(productPageAllowed("legal-portal","/legal-portal/continue")).toBe(false);
    expect(productPageAllowed("account","/api/legal-portal/enroll")).toBe(true);
    expect(productPageAllowed("legal-portal","/api/legal-portal/enroll")).toBe(false);
    for(const path of ["/my-profile/another-user","/security/unknown","/account/unknown","/holdings/acme/people-organization/people/directory/current"])
      expect(productPageAllowed("account",path)).toBe(false);
  });
  it("allows Forms to retrieve only its original public publication content route",()=>{
    expect(productPageAllowed("forms","/api/public/marketing/acme/form")).toBe(true);
    expect(productPageAllowed("forms","/api/public/holdings/acme/media-publications/published/content")).toBe(true);
    for(const path of ["/api/public/holdings/acme/media-publications/published","/api/public/holdings/acme/media-publications/published/content/extra","/api/public/holdings/acme/uploads","/api/v1/holdings/acme/attachments/private/content"])
      expect(productPageAllowed("forms",path)).toBe(false);
    expect(productPageAllowed("account","/api/public/holdings/acme/media-publications/published/content")).toBe(false);
  });
  it("hosts Company Admin and existing provider work only on Administration",()=>{
    const adminPaths=["/admin/launch","/admin/choose-work-role","/provider","/holdings/erpx/erpx-management","/holdings/erpx/erpx-management/iam/roles","/api/v1/platform/iam/roles","/api/v1/platform/operations/migrate","/api/v1/platform/billing/usage","/api/v1/platform/executive","/api/v1/platform/assignments/active","/api/v1/platform/context","/api/v1/platform/page-layouts/provider-home"];
    for(const path of adminPaths){expect(productPageAllowed("admin",path)).toBe(true);for(const key of [...productIds,...sharedHostKeys].filter(key=>key!=="admin"))expect(productPageAllowed(key,path)).toBe(false);}
    for(const path of ["/holdings/acme/erpx-management","/holdings/erpx/branches/main/erpx-management","/holdings/erpx/departments/it/manage-erpx","/api/v1/holdings/acme/manage-erpx/iam","/api/v1/platform/unknown","/api/v1/platform/page-layouts/a/b","/provider/unknown","/admin/unknown"])
      expect(productPageAllowed("admin",path)).toBe(false);
  });
});
it("keeps Supply setup canonical across Company scope and repeated handoff filters", () => {
  for (const scope of ["", "branches/main/", "departments/team/", "branches/main/departments/team/"]) {
    const base = `/holdings/acme/${scope}`;
    const target = `${base}purchasing/setup/posting?tag=one&tag=two#posting`;
    expect(canonicalUiHref(`${base}commerce?view=purchase&setup=posting&tag=one&tag=two#posting`)).toBe(target);
    expect(canonicalUiHref(target)).toBe(target);
    expect(productPageAllowed("supply", new URL(target, "https://supply-dev-erpx.jesadakorn.com").pathname)).toBe(true);
    expect(productPageAllowed("sales", new URL(target, "https://sales-dev-erpx.jesadakorn.com").pathname)).toBe(false);
    expect(productPageAllowed("finance", new URL(target, "https://finance-dev-erpx.jesadakorn.com").pathname)).toBe(false);
  }
});
describe("product API ownership", () => {
  it("limits Sales posting setup to its own reader and original setup/review commands", () => {
    for (const scope of ["", "branches/main/", "departments/team/", "branches/main/departments/team/"]) {
      const base = `/api/v1/holdings/acme/${scope}commerce/sales-posting-setup`;
      for (const host of [...productIds, ...sharedHostKeys]) for (const method of ["GET", "HEAD", "POST", "PUT", "PATCH", "DELETE", "OPTIONS"]) {
        expect(productPageAllowed(host, base, method), `${host} ${method} setup`).toBe(host === "sales" && ["GET", "HEAD"].includes(method));
        for (const suffix of ["charts", "accounts", "periods", "rates", "defaults", "defaults/sales-defaults-existing/decide"])
          expect(productPageAllowed(host, `${base}/${suffix}`, method), `${host} ${method} ${suffix}`).toBe(host === "sales" && method === "POST");
        for (const suffix of ["prerequisites", "unknown", "charts/chart1", "defaults/decide", "defaults/id/decide/extra", "rates/id/confirm"])
          expect(productPageAllowed(host, `${base}/${suffix}`, method), `${host} ${method} ${suffix}`).toBe(false);
      }
      for (const method of ["GET", "POST"])
        for (const resource of ["finance/charts", "finance/periods", "money/rates"])
          expect(productPageAllowed("sales", `/api/v1/holdings/acme/${scope}${resource}`, method)).toBe(false);
      for (const host of [...productIds, ...sharedHostKeys])
        expect(productPageAllowed(host, `/api/v1/holdings/acme/${scope}commerce/%73ales-posting-setup/defaults`, "POST"), host).toBe(host === "sales");
    }
  });
  it("keeps Sales goods initial mapping on its exact owner reader and creation command",()=>{
    for(const scope of ["","branches/main/","departments/team/","branches/main/departments/team/"]){
      const base=`/api/v1/holdings/acme/${scope}commerce/sales-goods-posting-setup`;
      for(const host of [...productIds,...sharedHostKeys])for(const method of ["GET","HEAD","POST","PUT","PATCH","DELETE","OPTIONS"]){
        expect(productPageAllowed(host,base,method),`${host} ${method}`).toBe(host==="sales"&&["GET","HEAD","POST"].includes(method));
        for(const suffix of ["initialize","accounts","defaults","id","unknown","prerequisites","rates/confirm"])
          expect(productPageAllowed(host,`${base}/${suffix}`,method),`${host} ${method} ${suffix}`).toBe(false);
      }
      for(const host of [...productIds,...sharedHostKeys])
        expect(productPageAllowed(host,`/api/v1/holdings/acme/${scope}commerce/%73ales-goods-posting-setup`,"POST"),host).toBe(host==="sales");
    }
  });
  it("applies Supply preparation ownership to decoded route segments before dispatch", () => {
    for (const encoded of ["commerce/%70osting-setup/charts", "%63ommerce/posting-setup/charts", "commerce/posting-setup/%63harts"])
      for (const host of [...productIds, ...sharedHostKeys])
        expect(productPageAllowed(host, `/api/v1/holdings/acme/${encoded}`, "POST"), `${host} ${encoded}`).toBe(host === "supply");
    for (const encoded of ["commerce/%70osting-setup/charts/extra", "commerce/posting-setup/%63harts/import", "commerce/posting-setup/%", "commerce/posting-setup/%2e%2e", "commerce/posting-setup/%252f", "commerce/posting-setup%2fcharts", "commerce/posting-setup%5ccharts"])
      for (const host of [...productIds, ...sharedHostKeys])
        expect(productPageAllowed(host, `/api/v1/holdings/acme/${encoded}`, "POST"), `${host} ${encoded}`).toBe(false);
  });
  it("exposes only the exact Supply ledger preparation reader and four original owner commands", () => {
    for (const scope of ["", "branches/main/", "departments/team/", "branches/main/departments/team/"]) {
      const base = `/api/v1/holdings/acme/${scope}commerce/posting-setup`;
      for (const host of [...productIds, ...sharedHostKeys]) {
        for (const method of ["GET", "HEAD", "POST", "PUT", "PATCH", "DELETE", "OPTIONS"]) {
          expect(productPageAllowed(host, `${base}/prerequisites`, method), `${host} ${method} prerequisites`)
            .toBe(host === "supply" && ["GET", "HEAD"].includes(method));
          for (const action of ["charts", "accounts", "periods", "rates"])
            expect(productPageAllowed(host, `${base}/${action}`, method), `${host} ${method} ${action}`)
              .toBe(host === "supply" && method === "POST");
          for (const suffix of ["prerequisites/records", "charts/chart1", "accounts/account1/status", "periods/period1/close", "rates/rate1", "unknown", "charts/import"])
            expect(productPageAllowed(host, `${base}/${suffix}`, method), `${host} ${method} ${suffix}`).toBe(false);
        }
      }
      for (const resource of ["finance/charts", "finance/charts/chart1/accounts", "finance/periods", "money/rates"])
        for (const method of ["GET", "POST"])
          expect(productPageAllowed("supply", `/api/v1/holdings/acme/${scope}${resource}`, method), `${method} ${resource}`).toBe(false);
    }
  });
  it("keeps initial Supply posting setup on its exact owner host and supported transport methods", () => {
    for (const scope of ["", "branches/main/", "departments/team/", "branches/main/departments/team/"]) {
      const path = `/api/v1/holdings/acme/${scope}commerce/posting-setup`;
      for (const host of [...productIds, ...sharedHostKeys]) {
        for (const method of ["GET", "HEAD", "POST"])
          expect(productPageAllowed(host, path, method), `${host} ${method} ${path}`).toBe(host === "supply");
        for (const method of ["PUT", "PATCH", "DELETE", "OPTIONS"])
          expect(productPageAllowed(host, path, method), `${host} ${method} ${path}`).toBe(false);
        for (const suffix of ["/initialize", "/unknown", "/record"])
          expect(productPageAllowed(host, path + suffix, "POST"), `${host} ${path + suffix}`).toBe(false);
      }
      expect(productPageAllowed("supply", `/api/v1/holdings/acme/${scope}commerce/orders/order1/fulfill`, "POST")).toBe(true);
      expect(productPageAllowed("sales", `/api/v1/holdings/acme/${scope}commerce/pricing`, "POST")).toBe(true);
    }
  });
  const offices = products.filter(entry => entry.shell === "office").map(entry => entry.id);
  const scopes = ["", "branches/main/", "departments/team/", "branches/main/departments/team/"];
  it("denies unrelated owner commands on every Office surface before dispatch, including unknown and public endpoints", () => {
    const commands = {apps:"operations",finance:"finance/journals",people:"people/payroll/runs",sales:"crm",supply:"stock-counts",assets:"maintenance",projects:"projects",recruit:"talent",learn:"learning",support:"support/incidents",service:"field-service",legal:"legal",manufacturing:"production",admin:"agents"} as const;
    for (const [owner, command] of Object.entries(commands)) for (const scope of scopes) for (const host of [...offices, "admin"] as const) {
      const path = `/api/v1/holdings/acme/${scope}${command}`;
      expect(productPageAllowed(host, path, "POST"), `${host}: ${path}`).toBe(host === owner);
    }
    for (const host of [...offices, "admin"] as const) for (const path of ["/api/unknown", "/api/v1/holdings/acme/unknown", "/api/v1/onboarding", "/api/customer-portal/acme", "/api/legal-portal/acme", "/api/public/marketing/acme/form", "/api/v1/front/acme/company/pos/sales"]) {
      expect(productPageAllowed(host, path, "POST"), `${host}: ${path}`).toBe(false);
    }
  });
  it("preserves actual same-origin dependencies without exposing their management collections", () => {
    const dependencies = [
      ["apps", "leave-requests/leave1/approve"], ["apps", "overtime-requests/request1/transition"], ["apps", "expenses/expense1/approve"],
      ["projects", "staff-tasks/task1/transition"], ["projects", "finance/project-cost-allocations"], ["projects", "finance/project-cost-allocations/cost1/review"],
      ["sales", "finance/sales/customers"], ["sales", "commerce/pricing"], ["supply", "commerce/orders/order1/fulfill"],
      ["recruit", "people/hire-handoffs/consume"], ["people", "iam/assignments"],
      ["admin", "people/organization/position-moves/submit"], ["admin", "people/lifecycle/placements/submit"], ["admin", "pos/connection/oauth"],
      ["admin", "people/lifecycle/position/preview"], ["admin", "people/lifecycle/position/bind"], ["people", "calendar"], ["people", "calendar/tasks"], ["people", "calendar/shifts"],
      ["finance", "pms"], ["finance", "pos/catalog/save"], ["finance", "commerce/products"],
    ] as const;
    for (const [host, resource] of dependencies) for (const scope of scopes)
      expect(productPageAllowed(host, `/api/v1/holdings/acme/${scope}${resource}`, "POST"), `${host}: ${resource}`).toBe(true);
    for (const [host, resource] of [["projects", "finance/journals"], ["recruit", "people/payroll/runs"], ["apps", "leave/admin"], ["admin", "pos/sale/pay"], ["finance", "pos/sale/pay"], ["finance", "pms/reports"], ["admin", "people/payroll/runs"]] as const)
      expect(productPageAllowed(host, `/api/v1/holdings/acme/${resource}`, "POST"), `${host}: ${resource}`).toBe(false);
    expect(productPageAllowed("finance", "/api/v1/holdings/acme/hospitality/setup", "GET")).toBe(true);
    expect(productPageAllowed("finance", "/api/v1/holdings/acme/pms", "GET")).toBe(false);
  });
  it("retains included Files, personal context and display preferences while keeping authentication on Account", () => {
    for (const host of [...offices, "admin"] as const) for (const scope of scopes) for (const resource of ["uploads", "attachment-sets/set1/bind", "attachments/file1/content", "profiles/me", "money/preference", "work/review", "requests/request1/actions"])
      expect(productPageAllowed(host, `/api/v1/holdings/acme/${scope}${resource}`, "POST"), `${host}: ${resource}`).toBe(true);
    for (const host of offices) {
      expect(productPageAllowed(host, "/api/auth/get-session", "GET")).toBe(true);
      expect(productPageAllowed(host, "/api/auth/sign-out", "POST")).toBe(true);
      expect(productPageAllowed(host, "/api/auth/sign-in/email", "POST")).toBe(false);
      expect(productPageAllowed(host, "/api/v1/account/work-context", "GET")).toBe(true);
      expect(productPageAllowed(host, "/api/v1/account/profile", "PATCH")).toBe(false);
      expect(productPageAllowed(host, "/api/v1/account/time-preference", "GET")).toBe(true);
      expect(productPageAllowed(host, "/api/v1/account/time-preference", "PATCH")).toBe(false);
      expect(productPageAllowed(host, "/api/v1/holdings/acme/money/scales", "POST")).toBe(host === "finance");
    }
  });
  it("keeps native record collections on their registered product and preserves personal Work history", () => {
    for (const [module, host] of [["finance", "finance"], ["people_payroll", "people"], ["quality", "supply"], ["planning", "manufacturing"], ["learning", "learn"], ["organization", "admin"]] as const) {
      expect(productPageAllowed(host, `/api/v1/holdings/acme/records/${module}`, "POST")).toBe(true);
      expect(productPageAllowed("sales", `/api/v1/holdings/acme/records/${module}`, "POST")).toBe(false);
    }
    expect(productPageAllowed("apps", "/api/v1/holdings/acme/records/people_payroll/leave1", "GET")).toBe(true);
    expect(productPageAllowed("apps", "/api/v1/holdings/acme/records/people_payroll", "GET")).toBe(false);
    expect(productPageAllowed("apps", "/api/v1/holdings/acme/records/people_payroll/leave1/transition", "POST")).toBe(false);
    expect(productPageAllowed("finance", "/api/v1/holdings/acme/records/unknown", "POST")).toBe(false);
    for (const path of ["/api/v1/holdings/acme/branches", "/api/v1/holdings/acme/branches/main/departments", "/api/v1/holdings/acme/branches//finance/journals", "/api/v1/holdings/acme/finance%2fjournals"])
      expect(officeApiAllowed("finance", path, "POST")).toBe(false);
  });
  it("does not allow the Finance Hotel dependency to dispatch a sale, guest-account or reservation command", () => {
    for (const operation of ["payment.verify_transfer", "accounting.bootstrap", "property.create", "property.save", "room_type.save", "room.save", "rate.save"])
      expect(pmsProductOperationAllowed("finance", operation)).toBe(true);
    for (const operation of ["reservation.check_in", "guest_account.close", "property.delete", "room.save.extra", "", null, {}, ["room.save"]])
      expect(pmsProductOperationAllowed("finance", operation)).toBe(false);
    expect(pmsProductOperationAllowed("pms", "reservation.check_in")).toBe(true);
    expect(pmsProductOperationAllowed(null, "reservation.check_in")).toBe(true);
  });
  it("keeps Company Executive and its scoped MCP on Work while retaining separate provider Executive authority", () => {
    for (const host of [...productIds, ...sharedHostKeys]) {
      for (const path of ["/api/v1/holdings/acme/executive/reports", "/api/v1/holdings/acme/executive/connections", "/api/mcp/executive"])
        expect(productPageAllowed(host, path, "POST"), `${host}: ${path}`).toBe(host === "apps");
      expect(productPageAllowed(host, "/api/v1/platform/executive", "POST")).toBe(host === "admin");
    }
    for (const action of ["task.plan", "task.complete", "shift.reschedule"]) expect(calendarProductActionAllowed("people", action)).toBe(true);
    for (const action of ["appointment.save", "announcement.publish", "task.new", null, {}]) expect(calendarProductActionAllowed("people", action)).toBe(false);
    expect(calendarProductActionAllowed("apps", "appointment.save")).toBe(true);
    expect(productPageAllowed("people", "/api/v1/holdings/acme/calendar/appointments", "POST")).toBe(false);
  });
  it("retains only the original Admin Commercial MFA verification dependency without opening Account security management",()=>{
    for(const host of [...productIds,...sharedHostKeys])expect(productPageAllowed(host,"/api/v1/account/iam/mfa/verify","POST"),host).toBe(host==="account"||host==="admin");
    for(const path of ["/api/v1/account/iam/mfa/verify","/api/v1/account/iam/mfa/enroll","/api/v1/account/iam/mfa/disable","/api/v1/account/iam/sessions"])
      expect(productPageAllowed("admin",path,"GET")).toBe(false);
    for(const path of ["/api/v1/account/iam/mfa/enroll","/api/v1/account/iam/mfa/disable"])
      expect(productPageAllowed("admin",path,"POST")).toBe(false);
  });
});
it('opens only canonical Work Routine collection and native overview under Operations readiness without buying or releasing Projects',async()=>{
 const {productDestinationAvailable}=await import('@erpx/server-work_projection/public/rules/product-landings'),context={role:'custom_original_routine',permissions:['records.read'],scopeKind:'holding' as const,modules:[],readyModules:['operations'],capabilities:['work.advanced-routines'],beta:false};
 for(const path of['projects-routines/routines','projects-routines/routines/routine-original','projects-routines/routines/routine-original/overview']){expect(productDestinationAvailable('apps',path,context)).toBe(true);expect(productDestinationAvailable('apps',path,{...context,beta:true})).toBe(false);expect(productDestinationAvailable('apps',path,{...context,permissions:[]})).toBe(false);expect(productDestinationAvailable('apps',path,{...context,readyModules:['projects']})).toBe(false);expect(productDestinationAvailable('apps',path,{...context,capabilities:['work.actions']})).toBe(false);}
 for(const path of['projects-routines/routines/new','projects-routines/routines/routine-original/private','projects-routines/routines/routine-original/overview/more','projects-routines/routines/../operations','projects-routines/operations','projects-routines/projects'])expect(productDestinationAvailable('apps',path,context)).toBe(false);
});
it('classifies only exact native conversation and personal workforce pages without a broad posts or generic-task product prefix',async()=>{
 const {productEntryCapability}=await import('@erpx/server-commercial/public/rules/product-entry-capability'),{productDestinationAvailable}=await import('@erpx/server-work_projection/public/rules/product-landings');
 for(const path of ['posts/actual-post','leave','leave/new','overtime','overtime/new','overtime/actual-id/overview','tasks/people_payroll/actual-id/overview'])expect(destinationProduct(path)).toBe('apps');
 for(const path of ['posts','posts/new','posts/actual-id/comments','posts/../admin','tasks/people_payroll','tasks/people_payroll/new','tasks/people_payroll/actual-id/private','leave/admin'])expect(destinationProduct(path)).toBeUndefined();
 const context={role:'employee',permissions:[],scopeKind:'holding' as const,modules:[],readyModules:['operations','collaboration'],capabilities:['work.today']};expect(productEntryCapability('apps','posts/actual-post')).toBe('work.social');expect(productDestinationAvailable('apps','posts/actual-post',context)).toBe(false);expect(productDestinationAvailable('apps','posts/actual-post',{...context,capabilities:['work.social']})).toBe(true);expect(productDestinationAvailable('apps','posts/actual-post',{...context,capabilities:['work.social'],readyModules:['operations']})).toBe(false);expect(productEntryCapability('apps','leave')).toBe('people.workforce');
});


it("keeps exact refund instruction bank evidence on Finance POST without opening alternate hosts or suffix actions",()=>{
 for(const host of [...productIds,...sharedHostKeys])for(const scope of ["","branches/main/","departments/ops/","branches/main/departments/ops/"]){
  const base=`/api/v1/holdings/acme/${scope}finance/refund-instructions`,exact=`${base}/instruction/bank-evidence`;
  for(const method of ["GET","HEAD","POST","PUT","PATCH","DELETE","OPTIONS"])expect(productPageAllowed(host,exact,method),`${host}:${scope}:${method}`).toBe(host==="finance"&&method==="POST");
  for(const suffix of ["/retry","/export","/extra/private"])expect(productPageAllowed(host,exact+suffix,"POST")).toBe(false);
  expect(productPageAllowed(host,`${base}/bank-evidence`,"POST")).toBe(false);
  expect(productPageAllowed(host,`${base}/instruction/%62ank-evidence`,"POST")).toBe(host==="finance");
 }
 expect(productPageAllowed("finance","/api/v1/holdings/acme/finance/refund-instructions/instruction/confirm","POST")).toBe(true);
 expect(productPageAllowed("finance","/api/v1/holdings/acme/finance/bank/statements/import","POST")).toBe(true);
});

it('keeps the named initial physical Count and record-only Flow on the exact Sales transport without opening manual Supply counts',async()=>{
 for(const host of [...productIds,...sharedHostKeys])for(const scope of ['', 'branches/main/','departments/ops/','branches/main/departments/ops/']){
  const base=`/api/v1/holdings/acme/${scope}stock-counts/initial-physical`;
  for(const suffix of ['','/count-original','/count-original/flow'])for(const method of ['GET','HEAD'])expect(productPageAllowed(host,base+suffix,method)).toBe(host==='sales');
  for(const suffix of ['','/count-original/submit','/count-original/approve','/count-original/post','/count-original/reject','/count-original/cancel','/count-original/flow'])expect(productPageAllowed(host,base+suffix,'POST')).toBe(host==='sales');
  for(const suffix of ['/count-original','/count-original/update','/count-original/flow/retry'])expect(productPageAllowed(host,base+suffix,'POST')).toBe(false);
  for(const method of ['PUT','PATCH','DELETE','OPTIONS'])expect(productPageAllowed(host,base,method)).toBe(false);
  expect(productPageAllowed(host,`/api/v1/holdings/acme/${scope}flows/stock_count/count-original`,'POST')).toBe(host==='supply');
 }
 const {initialPhysicalStockCountEntry:entry,initialPhysicalStockCountLanding:landing}=await import('@erpx/server-inventory/public/rules/initial-physical-stock-count');
 expect(entry(landing)).toEqual({});expect(entry(`${landing}/count-original%3Akey`)).toEqual({countId:'count-original:key'});
 for(const suffix of ['count-a%2Fb','count-a%5Cb','count-a%00','count-a/flow','count-%2561','count-%','../count-a'])expect(entry(`${landing}/${suffix}`)).toBeNull();
});


it('closes Manufacturing posting setup and its original Finance continuations before the general Production family',async()=>{
 const base='/api/v1/holdings/acme/production/posting-setup';
 const allowed=(suffix:string,method:string)=>suffix===''&&['GET','HEAD','POST'].includes(method)||suffix==='/prerequisites'&&['GET','HEAD'].includes(method)||['/charts','/accounts','/periods','/rates'].includes(suffix)&&method==='POST'||suffix==='/profiles'&&['GET','HEAD','POST'].includes(method)||suffix==='/profiles/original/decide'&&method==='POST';
 for(const host of [...productIds,...sharedHostKeys])for(const suffix of ['','/prerequisites','/charts','/accounts','/periods','/rates','/profiles','/profiles/original/decide','/extra','/charts/extra','/profiles/original','/profiles/original/decide/extra'])for(const method of ['GET','HEAD','POST','PUT','PATCH','DELETE','OPTIONS'])expect(productPageAllowed(host,base+suffix,method),`${host}:${suffix}:${method}`).toBe(host==='manufacturing'&&allowed(suffix,method));
 const {manufacturingPostingSetupLanding:landing,manufacturingPostingSetupEntry:entry}=await import('@erpx/server-manufacturing/public/rules/posting-setup-entry');expect(entry(landing)).toBe(true);
 for(const suffix of ['/','/extra','?profile=other','#profile','%2Fextra','\\extra'])expect(entry(landing+suffix)).toBe(false);
 expect(entry('production/manufacturing')).toBe(false);
});


it('closes Manufacturing physical Counts, Flow and original master constructors to their exact product transport',async()=>{
 for(const host of [...productIds,...sharedHostKeys])for(const scope of ['','branches/main/','departments/ops/','branches/main/departments/ops/']){
  const base=`/api/v1/holdings/acme/${scope}production/initial-physical-stock`;
  for(const suffix of ['','/count-original','/count-original/flow'])for(const method of ['GET','HEAD'])expect(productPageAllowed(host,base+suffix,method)).toBe(host==='manufacturing');
  for(const suffix of ['','/count-original/submit','/count-original/approve','/count-original/post','/count-original/reject','/count-original/cancel','/count-original/flow','/masters/products','/masters/warehouses'])expect(productPageAllowed(host,base+suffix,'POST')).toBe(host==='manufacturing');
  for(const suffix of ['/masters','/masters/products','/masters/warehouses','/masters/products/extra'])for(const method of ['GET','HEAD'])expect(productPageAllowed(host,base+suffix,method)).toBe(false);
  for(const suffix of ['/masters','/masters/parties','/masters/products/id','/count-original','/count-original/update','/count-original/flow/retry'])expect(productPageAllowed(host,base+suffix,'POST')).toBe(false);
  for(const method of ['PUT','PATCH','DELETE','OPTIONS'])expect(productPageAllowed(host,base,method)).toBe(false);
  expect(productPageAllowed(host,`/api/v1/holdings/acme/${scope}flows/stock_count/count-original`,'POST')).toBe(host==='supply');
 }
 const {manufacturingInitialPhysicalStockCountEntry:entry,manufacturingInitialPhysicalStockCountLanding:landing}=await import('@erpx/server-inventory/public/rules/manufacturing-initial-physical-stock-count');expect(entry(landing)).toEqual({});expect(entry(`${landing}/count-real%3Akey`)).toEqual({countId:'count-real:key'});
 for(const suffix of ['count-a%2Fb','count-a%5Cb','count-a%00','count-a%7f','count-a/flow','count-%2561','count-%','../count-a'])expect(entry(`${landing}/${suffix}`)).toBeNull();
 expect(entry('customers-sales/sales/settings/initial-stock/count-real')).toBeNull();
});


it("closes POS startup transports before the broad Front owner family for every scoped path",()=>{
 for(const prefix of ['','branches/main/','departments/team/','branches/main/departments/team/']){
  const root=`/api/v1/holdings/acme/${prefix}pos/startup`;
  for(const verb of ['GET','HEAD'])expect(productPageAllowed('pos',root,verb)).toBe(true);
  for(const action of ['charts','accounts','periods','rates','mapping','warehouses','products']){
   const path=`${root}/${action}`;expect(productPageAllowed('pos',path,'POST')).toBe(true);
   for(const verb of ['GET','HEAD','PUT','PATCH','DELETE'])expect(productPageAllowed('pos',path,verb)).toBe(false);
   expect(productPageAllowed('pms',path,'POST')).toBe(false);expect(productPageAllowed('finance',path,'POST')).toBe(false);
   expect(productPageAllowed('pos',`${path}/arbitrary`,'POST')).toBe(false);
  }
  for(const verb of ['POST','PUT','PATCH','DELETE'])expect(productPageAllowed('pos',root,verb)).toBe(false);
  expect(productPageAllowed('pos',`${root}/arbitrary`,'POST')).toBe(false);
  expect(productPageAllowed('pos',root.replace('startup','%73tartup')+'/mapping','DELETE')).toBe(false);
 }
});


it('registers POS startup links with their original source and preserves scoped filters and fragments',async()=>{
 const {sourcePath,selectionForPath}=await import('@erpx/feature-work_projection/lib/destination-registry');
 for(const scope of ['','branches/main/','departments/team/','branches/main/departments/team/']){
  const base=`/holdings/acme/${scope}`,target=`${base}pos/settings/startup?tag=one&tag=two#drawer`;
  expect(canonicalUiHref(`${base}pos?tab=settings&setup=startup&tag=one&tag=two#drawer`)).toBe(target);
  expect(sourcePath('pos/settings/startup')).toBe('pos');expect(selectionForPath(new URL(target,'https://path.invalid').pathname)).toEqual({tab:'settings',setup:'startup'});expect(destinationProduct('pos/settings/startup')).toBe('pos');
 }
});


it('closes Assets financial constructors and decisions before the general Assets family',()=>{
 const allowed=(suffix:string,method:string)=>{
  const read=method==='GET'||method==='HEAD';
  if(suffix==='')return read||method==='POST';
  if(suffix==='/prerequisites')return read;
  if(['/charts','/accounts','/periods','/rates'].includes(suffix))return method==='POST';
  if(suffix==='/asset-real')return read;
  return method==='POST'&&['/asset-real/edit','/asset-real/cancel','/asset-real/recognize','/asset-real/disposals','/disposals/disposal-real/post','/disposals/disposal-real/cancel'].includes(suffix);
 };
 const suffixes=['','/prerequisites','/charts','/accounts','/periods','/rates','/asset-real','/asset-real/edit','/asset-real/cancel','/asset-real/recognize','/asset-real/disposals','/disposals/disposal-real/post','/disposals/disposal-real/cancel','/disposals','/disposals/disposal-real','/prerequisites/edit','/charts/import','/charts/recognize','/asset-real/journals','/asset-real/edit/extra','/disposals/disposal-real/post/extra'];
 for(const scope of ['','branches/main/','departments/ops/','branches/main/departments/ops/'])for(const host of [...productIds,...sharedHostKeys])for(const suffix of suffixes)for(const method of ['GET','HEAD','POST','PUT','PATCH','DELETE','OPTIONS']){
  const path=`/api/v1/holdings/acme/${scope}assets/financial-assets${suffix}`;
  expect(productPageAllowed(host,path,method),`${host}:${path}:${method}`).toBe(host==='assets'&&allowed(suffix,method));
 }
});

it('uses decoded Assets financial selectors without admitting reserved static IDs or foreign hosts',()=>{
 const base='/api/v1/holdings/acme/assets/financial-assets';
 for(const host of [...productIds,...sharedHostKeys]){
  expect(productPageAllowed(host,base.replace('financial-assets','%66inancial-assets')+'/asset-real/%72ecognize','POST')).toBe(host==='assets');
  for(const suffix of ['/charts/recognize','/prerequisites/cancel','/disposals/recognize','/rates/edit','/accounts/disposals','/periods/recognize','/asset-real/recognize/extra','/asset-real%2fother/recognize','/%2e%2e/recognize','/%252f/recognize','/%/recognize'])expect(productPageAllowed(host,base+suffix,'POST'),`${host}:${suffix}`).toBe(false);
 }
 expect(productPageAllowed('assets','/api/v1/holdings/acme/finance/assets','POST')).toBe(false);
 expect(productPageAllowed('assets','/api/v1/holdings/acme/assets/physical-real','GET')).toBe(true);
});


it('keeps Assets accounting entry confined to one original decoded owner record',async()=>{
 const {assetsFinancialLanding:landing,assetsFinancialEntry:entry}=await import('@erpx/server-finance/public/rules/assets-financial-entry');
 expect(entry(landing)).toEqual({});expect(entry(landing+'/asset-original:key')).toEqual({assetId:'asset-original:key'});
 for(const suffix of ['/', '/.', '/..','/asset/extra','/asset%2Fextra','/asset%252Fextra','/asset?query','/asset#part','/asset'+String.fromCharCode(127)])expect(entry(landing+suffix),suffix).toBeNull();
 expect(entry('finance/assets/asset-original')).toBeNull();
});

it('closes POS initial physical Count and Flow to their named transport without master or generic Supply access',()=>{
 for(const host of [...productIds,...sharedHostKeys])for(const scope of ['','branches/main/','departments/ops/','branches/main/departments/ops/']){
  const base=`/api/v1/holdings/acme/${scope}pos/initial-physical-stock`;
  for(const suffix of ['','/count-original','/count-original/flow'])for(const method of ['GET','HEAD'])expect(productPageAllowed(host,base+suffix,method)).toBe(host==='pos');
  for(const suffix of ['','/count-original/submit','/count-original/approve','/count-original/post','/count-original/reject','/count-original/cancel','/count-original/flow'])expect(productPageAllowed(host,base+suffix,'POST')).toBe(host==='pos');
  for(const suffix of ['/masters','/masters/products','/masters/warehouses','/count-original','/count-original/edit','/count-original/flow/retry'])expect(productPageAllowed(host,base+suffix,'POST')).toBe(false);
  for(const suffix of ['/masters','/masters/products','/masters/warehouses','/count-original/submit','/count-original/extra'])expect(productPageAllowed(host,base+suffix,'GET')).toBe(false);
  for(const method of ['PUT','PATCH','DELETE','OPTIONS'])expect(productPageAllowed(host,base,method)).toBe(false);
  expect(productPageAllowed(host,base.replace('initial-physical-stock','%69nitial-physical-stock')+'/count-original/%70ost','POST')).toBe(host==='pos');
  for(const suffix of ['/%/post','/%2e%2e/post','/count%2fother/post','/%252f/post','/count-real/%00'])expect(productPageAllowed(host,base+suffix,'POST')).toBe(false);
  expect(productPageAllowed(host,`/api/v1/holdings/acme/${scope}flows/stock_count/count-original`,'POST')).toBe(host==='supply');
 }
});


it('registers Assets accounting links with the original Assets source while retaining scoped filters and fragment',async()=>{
 const {sourcePath,selectionForPath}=await import('@erpx/feature-work_projection/lib/destination-registry');
 for(const scope of ['','branches/main/','departments/team/','branches/main/departments/team/']){
  const base=`/holdings/acme/${scope}`,target=`${base}services/assets/accounting?tag=one&tag=two#editor`;
  expect(canonicalUiHref(`${base}assets?view=accounting&tag=one&tag=two#editor`)).toBe(target);expect(sourcePath('services/assets/accounting')).toBe('assets');expect(selectionForPath(new URL(target,'https://path.invalid').pathname)).toEqual({view:'accounting'});expect(destinationProduct('services/assets/accounting')).toBe('assets');
 }
});

it('parses only exact POS initial Count destinations and retains their native Count identity',async()=>{
 const {posInitialPhysicalStockCountEntry:entry,posInitialPhysicalStockCountLanding:landing}=await import('@erpx/server-inventory/public/rules/pos-initial-physical-stock-count');
 expect(entry(landing)).toEqual({});expect(entry(landing+'/count-original%3Akey')).toEqual({countId:'count-original:key'});
 for(const suffix of ['count-a%2Fb','count-a%5Cb','count-a%00','count-a%7f','count-a/flow','count-%2561','count-%','../count-a'])expect(entry(`${landing}/${suffix}`)).toBeNull();
 expect(entry('production/manufacturing/settings/initial-stock')).toBeNull();expect(entry('customers-sales/sales/settings/initial-stock')).toBeNull();
});


it('registers POS initial physical Stock with the original POS source and preserves filters',async()=>{
 const {sourcePath}=await import('@erpx/feature-work_projection/lib/destination-registry');
 const target='/holdings/acme/branches/main/pos/settings/initial-stock?tag=one&tag=two#count';expect(canonicalUiHref('/holdings/acme/branches/main/pos?tab=settings&setup=initial-stock&tag=one&tag=two#count')).toBe(target);expect(sourcePath('pos/settings/initial-stock')).toBe('pos');
});


it('closes Service parts preparation, references and masters to their exact product and verbs',()=>{
 const allowed=(suffix:string,method:string)=>{
  const read=method==='GET'||method==='HEAD';
  if(suffix==='')return read||method==='POST';
  if(['/prerequisites','/stock'].includes(suffix))return read;
  return method==='POST'&&['/existing','/charts','/accounts','/periods','/rates','/masters/products','/masters/warehouses'].includes(suffix);
 };
 for(const scope of ['','branches/main/','departments/ops/','branches/main/departments/ops/'])for(const host of [...productIds,...sharedHostKeys])for(const suffix of ['','/prerequisites','/stock','/existing','/charts','/accounts','/periods','/rates','/masters/products','/masters/warehouses','/masters','/masters/products/extra','/accounts/import','/stock/edit','/existing/confirm','/other'])for(const method of ['GET','HEAD','POST','PUT','PATCH','DELETE','OPTIONS'])expect(productPageAllowed(host,`/api/v1/holdings/acme/${scope}field-service/parts-setup${suffix}`,method),`${host}:${scope}:${suffix}:${method}`).toBe(host==='service'&&allowed(suffix,method));
 expect(productPageAllowed('service','/api/v1/holdings/acme/%66ield-service/%70arts-setup/%61ccounts','POST')).toBe(true);
 expect(productPageAllowed('service','/api/v1/holdings/acme/finance/accounts','POST')).toBe(false);
 expect(productPageAllowed('service','/api/v1/holdings/acme/field-service/orders','GET')).toBe(true);
});

it('closes Service physical Count and record Flow without granting generic Supply or master transport',()=>{
 for(const host of [...productIds,...sharedHostKeys])for(const scope of ['','branches/main/','departments/ops/','branches/main/departments/ops/']){
  const base=`/api/v1/holdings/acme/${scope}field-service/initial-physical-stock`;
  for(const suffix of ['','/count-original','/count-original/flow'])for(const method of ['GET','HEAD'])expect(productPageAllowed(host,base+suffix,method)).toBe(host==='service');
  for(const suffix of ['','/count-original/submit','/count-original/approve','/count-original/post','/count-original/reject','/count-original/cancel','/count-original/flow'])expect(productPageAllowed(host,base+suffix,'POST')).toBe(host==='service');
  for(const suffix of ['/masters','/masters/products','/masters/warehouses','/count-original','/count-original/edit','/count-original/flow/retry'])expect(productPageAllowed(host,base+suffix,'POST')).toBe(false);
  for(const suffix of ['/masters','/masters/products','/count-original/post','/count-original/extra'])expect(productPageAllowed(host,base+suffix,'GET')).toBe(false);
  for(const method of ['PUT','PATCH','DELETE','OPTIONS'])expect(productPageAllowed(host,base,method)).toBe(false);
  for(const suffix of ['/%/post','/%2e%2e/post','/count%2fother/post','/%252f/post','/count-real/%00'])expect(productPageAllowed(host,base+suffix,'POST')).toBe(false);
  expect(productPageAllowed(host,base.replace('initial-physical-stock','%69nitial-physical-stock')+'/count-original/%70ost','POST')).toBe(host==='service');
  expect(productPageAllowed(host,`/api/v1/holdings/acme/${scope}flows/stock_count/count-original`,'POST')).toBe(host==='supply');
 }
});


it('keeps owner-proven setup out of generic Maintenance and Service discovery',async()=>{
 const {destinationAvailable}=await import('@erpx/feature-work_projection/lib/destination-access');
 const accounting=destinations.find(item=>item.path==='services/assets/accounting')!;
 for(const path of ['services/field-service/settings/parts','services/field-service/settings/initial-stock'])expect(destinationAvailable({role:'custom',permissions:['records.read'],scopeKind:'holding',workspaceBase:'',moduleIds:['field_service'],availableAppIds:['field-service']},destinations.find(item=>item.path===path)!)).toBe(false);
 expect(destinationAvailable({role:'custom',permissions:['records.read'],scopeKind:'holding',workspaceBase:'',moduleIds:['maintenance'],availableAppIds:['maintenance']},accounting)).toBe(false);
});


it('registers Service setup and initial physical Count without changing its native source or query identity',async()=>{
 const {sourcePath}=await import('@erpx/feature-work_projection/lib/destination-registry'),{productEntryCapability}=await import('@erpx/server-commercial/public/rules/product-entry-capability');
 for(const scope of ['','branches/main/','departments/team/','branches/main/departments/team/'])for(const [selector,path] of [['parts-setup','services/field-service/settings/parts'],['initial-stock','services/field-service/settings/initial-stock']]){
  const base=`/holdings/acme/${scope}`;expect(canonicalUiHref(`${base}field-service?view=${selector}&tag=one&tag=two#drawer`)).toBe(`${base}${path}?tag=one&tag=two#drawer`);expect(sourcePath(path)).toBe('field-service');expect(destinationProduct(path)).toBe('service');expect(productEntryCapability('service',path)).toBe('service.parts-assets');
 }
 const {serviceInitialPhysicalStockCountEntry:entry,serviceInitialPhysicalStockCountLanding:landing}=await import('@erpx/server-inventory/public/rules/service-initial-physical-stock-count');expect(entry(landing)).toEqual({});expect(entry(landing+'/count-original%3Akey')).toEqual({countId:'count-original:key'});for(const suffix of ['count-a%2Fb','count-a%5Cb','count-a%00','count-a%7f','count-a/flow','count-%2561','count-%','../count-a'])expect(entry(`${landing}/${suffix}`)).toBeNull();
});

it('closes Projects worker preparation to the exact named transport and keeps its generic discovery unavailable',async()=>{
 const {projectWorkerStartupLanding:landing,projectWorkerStartupEntry:entry,projectWorkerPresentationAllowed:present}=await import('@erpx/server-people_payroll/public/rules/project-worker-startup-entry'),{productEntryCapability}=await import('@erpx/server-commercial/public/rules/product-entry-capability'),{productDestinationAvailable}=await import('@erpx/server-work_projection/public/rules/product-landings');
 expect(entry(landing)).toEqual({});expect(entry(landing+'/person-real%3Aoriginal')).toEqual({personId:'person-real:original'});
 for(const suffix of ['/new','/person-real/extra','/%2f','/%5c','/%252f','/%00','/%','/..'])expect(entry(landing+suffix)).toBeNull();
 const context={role:'holding_owner',scopeKind:'holding' as const,permissions:['people.manage','organization.manage'],modules:['projects'],readyModules:['projects','people_payroll'],capabilities:['projects.wbs','people.worker-reference'],beta:false};expect(present(context)).toBe(true);expect(productEntryCapability('projects',landing)).toBe('people.worker-reference');expect(productDestinationAvailable('projects',landing,context)).toBe(false);expect(productDestinationAvailable('projects',landing+'/person-real',context)).toBe(false);expect(destinations.find(row=>row.path===landing)).toMatchObject({source:'projects',selection:{view:'worker-preparation'}});
 const allowed=(suffix:string,method:string)=>{const read=method==='GET'||method==='HEAD';return suffix===''||['/accounts','/people'].includes(suffix)?read:method==='POST'&&['/prepare','/person-real/placement/preview','/person-real/placement/submit','/person-real/placements/case-real/cancel'].includes(suffix);};
 for(const host of [...productIds,...sharedHostKeys])for(const scope of ['','branches/main/','departments/team/','branches/main/departments/team/'])for(const suffix of ['','/accounts','/people','/prepare','/person-real','/person-real/placement','/person-real/placement/preview','/person-real/placement/submit','/person-real/placements/case-real/cancel','/people/placement/submit','/accounts/placement/preview','/prepare/placements/case-real/cancel','/person-real/placements/case-real','/person-real/placements/case-real/cancel/extra','/person-real/placement/approve'])for(const method of ['GET','HEAD','POST','PUT','PATCH','DELETE','OPTIONS'])expect(productPageAllowed(host,`/api/v1/holdings/acme/${scope}people/project-worker-startup${suffix}`,method),`${host}:${scope}:${suffix}:${method}`).toBe(host==='projects'&&allowed(suffix,method));
 expect(productPageAllowed('projects','/api/v1/holdings/acme/%70eople/project-worker-startup/%70repare','POST')).toBe(true);
 for(const path of ['people/directory','people/lifecycle','people/users-access','people/project-worker-startup/person%252fescape/placement/submit','people/project-worker-startup/person%5cescape/placement/submit'])expect(productPageAllowed('projects','/api/v1/holdings/acme/'+path,'POST')).toBe(false);
});

it('closes Learning worker preparation to the exact named transport and keeps its generic discovery unavailable',async()=>{
 const {learningWorkerStartupLanding:landing,learningWorkerStartupEntry:entry,learningWorkerPresentationAllowed:present}=await import('@erpx/server-people_payroll/public/rules/learning-worker-startup-entry'),{productEntryCapability}=await import('@erpx/server-commercial/public/rules/product-entry-capability'),{productDestinationAvailable}=await import('@erpx/server-work_projection/public/rules/product-landings');
 expect(entry(landing)).toEqual({});expect(entry(landing+'/person-real%3Aoriginal')).toEqual({personId:'person-real:original'});
 for(const suffix of ['/new','/person-real/extra','/%2f','/%5c','/%252f','/%00','/%','/..'])expect(entry(landing+suffix)).toBeNull();
 const context={role:'holding_owner',scopeKind:'holding' as const,permissions:['people.manage','organization.manage'],modules:['learning'],readyModules:['learning','people_payroll'],capabilities:['learn.assignments','people.learner-reference'],beta:false};expect(present(context)).toBe(true);expect(productEntryCapability('learn',landing)).toBe('people.learner-reference');expect(productDestinationAvailable('learn',landing,context)).toBe(false);expect(productDestinationAvailable('learn',landing+'/person-real',context)).toBe(false);expect(destinations.find(row=>row.path===landing)).toMatchObject({source:'knowledge',selection:{view:'worker-preparation'}});
 const allowed=(suffix:string,method:string)=>{const read=method==='GET'||method==='HEAD';return suffix===''||['/accounts','/people'].includes(suffix)?read:method==='POST'&&['/prepare','/person-real/placement/preview','/person-real/placement/submit','/person-real/placements/case-real/cancel'].includes(suffix);};
 for(const host of [...productIds,...sharedHostKeys])for(const scope of ['','branches/main/','departments/team/','branches/main/departments/team/'])for(const suffix of ['','/accounts','/people','/prepare','/person-real','/person-real/placement','/person-real/placement/preview','/person-real/placement/submit','/person-real/placements/case-real/cancel','/people/placement/submit','/accounts/placement/preview','/prepare/placements/case-real/cancel','/person-real/placements/case-real','/person-real/placements/case-real/cancel/extra','/person-real/placement/approve'])for(const method of ['GET','HEAD','POST','PUT','PATCH','DELETE','OPTIONS'])expect(productPageAllowed(host,`/api/v1/holdings/acme/${scope}people/learning-worker-startup${suffix}`,method),`${host}:${scope}:${suffix}:${method}`).toBe(host==='learn'&&allowed(suffix,method));
 expect(productPageAllowed('learn','/api/v1/holdings/acme/%70eople/learning-worker-startup/%70repare','POST')).toBe(true);
 for(const path of ['people/directory','people/lifecycle','people/users-access','people/learning-worker-startup/person%252fescape/placement/submit','people/learning-worker-startup/person%5cescape/placement/submit'])expect(productPageAllowed('learn','/api/v1/holdings/acme/'+path,'POST')).toBe(false);
});


it('keeps ticket-bound Helpdesk Portal staff commands on their exact owner host and verb',()=>{
 for(const scope of ['', 'branches/main/','departments/team/','branches/main/departments/team/']){
  const base=`/api/v1/holdings/acme/${scope}support/ticket-portal`;
  for(const [suffix,method] of [['','GET'],['','HEAD'],['','POST'],['/binding-original','GET'],['/binding-original','HEAD'],['/binding-original/decisions','POST'],['/binding-original/responses','POST']]){
   expect(productPageAllowed('support',base+suffix,method)).toBe(true);
   for(const host of ['sales','service','admin','portal','account','apps'] as const)expect(productPageAllowed(host,base+suffix,method)).toBe(false);
  }
  for(const [suffix,method] of [['','DELETE'],['/binding-original','POST'],['/binding-original/decisions','GET'],['/binding-original/responses','PATCH'],['/binding-original/close','POST'],['/binding-original/responses/extra','POST'],['//decisions','POST'],['/%2f/decisions','POST']])expect(productPageAllowed('support',base+suffix,method)).toBe(false);
 }
});

it('separates verified Support ticket transport from staff, Sales and unknown public actions',()=>{
 const base='/api/customer-portal/acme/tickets';
 for(const [suffix,method] of [['','GET'],['','HEAD'],['/binding-original/acknowledgements','POST'],['/binding-original/files/link-original/content','GET'],['/binding-original/files/link-original/content','HEAD']]){
  expect(productPageAllowed('portal',base+suffix,method)).toBe(true);
  for(const host of ['support','sales','service','admin','account','apps','legal-portal'] as const)expect(productPageAllowed(host,base+suffix,method)).toBe(false);
 }
 for(const [suffix,method] of [['','POST'],['/binding-original','GET'],['/binding-original/acknowledgements','GET'],['/binding-original/responses','POST'],['/binding-original/close','POST'],['/binding-original/files/link-original/content','POST'],['/binding-original/files/link-original/content/extra','GET'],['/%2f/acknowledgements','POST'],['/%255c/acknowledgements','POST'],['//acknowledgements','POST']])expect(productPageAllowed('portal',base+suffix,method)).toBe(false);
 expect(productPageAllowed('portal','/api/customer-portal/acme/%74ickets/binding-original/close','POST')).toBe(false);
 expect(productPageAllowed('portal','/api/customer-portal/acme/requests','POST')).toBe(true);
});


it('retains the real Helpdesk Portal binding page instead of interpreting it as a Support ticket record',async()=>{
 const {supportTicketPortalEntry:entry,supportTicketPortalLanding:landing}=await import('@erpx/server-support/public/rules/ticket-portal-entry'),{productEntryCapability}=await import('@erpx/server-commercial/public/rules/product-entry-capability'),{productDestinationAvailable}=await import('@erpx/server-work_projection/public/rules/product-landings');
 expect(entry(landing)).toEqual({});expect(entry(landing+'/support-portal-original%3Akey')).toEqual({bindingId:'support-portal-original:key'});
 const ownerContext={role:'support_agent',permissions:['support.read'],scopeKind:'holding' as const,modules:['support'],readyModules:['support'],capabilities:['support.tickets','support.portal']};
 expect(productEntryCapability('support',landing)).toBe('support.portal');expect(productEntryCapability('support',landing+'/binding-original')).toBe('support.portal');
 expect(productEntryCapability('support',landing+'/support-portal-original%3Akey')).toBe('support.portal');
 expect(productDestinationAvailable('support',landing,ownerContext)).toBe(true);
 expect(productDestinationAvailable('support',landing,{...ownerContext,capabilities:['support.tickets']})).toBe(false);
 expect(productDestinationAvailable('support',landing,{...ownerContext,permissions:[]})).toBe(false);
 expect(productEntryCapability('support','customers-sales/customers/service/cases')).toBe('support.tickets');
 for(const suffix of ['/new','/%2f','/%5c','/%252f','/%00','/%','/../binding','/binding/decisions','/binding-original/new']){
  expect(entry(landing+suffix)).toBeNull();expect(productEntryCapability('support',landing+suffix)).toBeNull();
 }
 for(const scope of ['', 'branches/main/','departments/team/','branches/main/departments/team/']){
  const base=`/holdings/acme/${scope}`,query='?ticketId=ticket-original&tag=one&tag=two#binding-original';
  expect(canonicalUiHref(base+'support/ticket-portal'+query)).toBe(base+landing+query);
  for(const path of [landing+'/support-portal-original%3Akey','support/ticket-portal/support-portal-original%3Akey'])expect(canonicalUiHref(base+path+query)).toBe(base+landing+'/support-portal-original%3Akey'+query);
  expect(destinationProduct(landing)).toBe('support');expect(productPageAllowed('support',base+landing+'/binding-original')).toBe(true);expect(productPageAllowed('sales',base+landing+'/binding-original')).toBe(false);
 }
});

it('keeps Payroll posting prerequisites on their exact canonical owner page while preserving query and fragment bytes',async()=>{
 const {payrollPostingPrerequisitesEntry:entry,payrollPostingPrerequisitesLanding:landing,payrollPostingPrerequisitesPresentationAllowed:present}=await import('@erpx/server-finance/public/rules/payroll-posting-prerequisites-entry'),query='?payGroupId=group-original&tag=one&tag=two#references';
 expect(entry(landing)).toBe(true);for(const path of [landing+'/','landing',landing+'/charts',landing+'/new',landing+'?payGroupId=forged',landing+'#references',landing.replace('/posting',''),landing.replace('/posting','/posting-extra')])expect(entry(path)).toBe(false);
 expect(canonicalUiHref('/holdings/acme/people/payroll/settings/posting'+query)).toBe('/holdings/acme/'+landing+query);expect(canonicalUiHref('/holdings/acme/'+landing+query)).toBe('/holdings/acme/'+landing+query);expect(destinationProduct(landing)).toBe('people');expect(productPageAllowed('people','/holdings/acme/'+landing+query)).toBe(true);for(const host of ['finance','apps','admin','learn','projects'] as const)expect(productPageAllowed(host,'/holdings/acme/'+landing+query)).toBe(false);
 const context={scopeKind:'holding',modules:['finance'],permissions:['finance.approve']};expect(present(context)).toBe(true);for(const patch of [{modules:[]},{modules:['people_payroll']},{permissions:['finance.read','finance.write']},{scopeKind:'branch'},{scopeKind:'department'},{beta:true}])expect(present({...context,...patch})).toBe(false);
});
it('closes Payroll prerequisite transport to the exact People reference actions without generic Finance or residual child authority',()=>{
 const base='/api/v1/holdings/acme/people/payroll/posting-prerequisites';
 for(const [suffix,method]of [['','GET'],['','HEAD'],['/charts','POST'],['/accounts','POST'],['/periods','POST'],['/rates','POST'],['/confirmations','POST']]){expect(productPageAllowed('people',base+suffix,method),suffix+':'+method).toBe(true);for(const host of [...productIds,...sharedHostKeys].filter(host=>host!=='people'))expect(productPageAllowed(host,base+suffix,method),host+':'+suffix+':'+method).toBe(false);}
 for(const [suffix,method]of [['','POST'],['','PATCH'],['','DELETE'],['/charts','GET'],['/accounts','HEAD'],['/periods','PUT'],['/rates','DELETE'],['/confirmations','GET'],['/confirmations','OPTIONS'],['/charts/record-id','POST'],['/accounts/record-id','GET'],['/groups','GET'],['/references','GET'],['/confirmations/key','POST'],['-extra/charts','POST'],['//charts','POST'],['/%2fcharts','POST'],['/%255ccharts','POST']])expect(productPageAllowed('people',base+suffix,method),suffix+':'+method).toBe(false);
 const residualNames=['/api/v1/holdings/acme/people/payroll/%2570osting-prerequisites/charts','/api/v1/holdings/acme/people/%2570ayroll/posting-prerequisites/charts'];expect(residualNames.map(path=>({path,allowed:productPageAllowed('people',path,'POST')}))).toEqual(residualNames.map(path=>({path,allowed:false})));
 expect(productPageAllowed('people','/api/v1/holdings/acme/people/payroll/%70osting-prerequisites/%63harts','POST')).toBe(true);expect(productPageAllowed('people','/api/v1/holdings/acme/people/payroll/%2570osting-prerequisites/charts','POST')).toBe(false);expect(productPageAllowed('people','/api/v1/holdings/acme/people/%2570ayroll/posting-prerequisites/charts','POST')).toBe(false);
});

import {createPayrollPostingPrerequisiteCommands as payrollReferenceCommands,payrollPostingRequest as payrollReferenceRequest,type PayrollPostingRequest as PayrollOwnerReferenceRequest} from '@erpx/feature-people_payroll/lib/payroll-posting-prerequisites-command';
import type {PayrollPostingPrerequisitesView as PayrollOwnerReferences} from '@erpx/server-finance/public/payroll-posting-prerequisites';
it('uses the actual Payroll UI command URLs for all four named constructors, native confirmation and selected-group refresh',async()=>{
 const group={id:'group-original',name:'Actual group',currency:'THB',status:'active' as const},chart={id:'chart-original',name:'Actual chart',baseCurrency:'THB'},view:PayrollOwnerReferences={companyId:'company',available:true,contextKey:'a'.repeat(64),selection:group,groups:[group],canCreateChart:true,canCreateAccount:true,canCreatePeriod:true,canConfirmRate:true,charts:[chart],accounts:[],periods:[],rates:[],unavailableReason:null},requests:PayrollOwnerReferenceRequest[]=[payrollReferenceRequest(view,'charts',{name:'Actual new chart'}),payrollReferenceRequest(view,'accounts',{chartId:chart.id,name:'Actual liability',code:'2100',purpose:'liability'}),payrollReferenceRequest(view,'periods',{name:'Actual open period',startDate:'2026-10-01',endDate:'2026-10-31'}),payrollReferenceRequest(view,'rates',{effectiveDate:'2026-10-01',currencyPerUsd:'32.10',note:'Actual confirmed rate'})],singular={charts:'chart',accounts:'account',periods:'period',rates:'rate'} as const;
 for(const request of requests){const id={charts:'chart-native-original',accounts:'acct-native-original',periods:'period-native-original',rates:'fx-native-original'}[request.kind],receipt={...request.body.input,id,...(request.kind==='rates'?{confirmedAt:123}:{companyId:'company',version:request.kind==='periods'?0:1,...(request.kind==='periods'?{state:'open'}:{})})},next:PayrollOwnerReferences={...view,charts:request.kind==='charts'?[chart,{id,name:request.body.input.name,baseCurrency:'THB'}]:[chart],accounts:request.kind==='accounts'?[{id,chartId:chart.id,code:request.body.input.code,name:request.body.input.name,category:request.body.input.category,currency:'THB',controlKind:'none'}]:[],periods:request.kind==='periods'?[{id,name:request.body.input.name,startDate:request.body.input.startDate,endDate:request.body.input.endDate,state:'open',version:0}]:[],rates:request.kind==='rates'?[{id,currency:'THB',effectiveDate:request.body.input.effectiveDate,currencyPerUsd:request.body.input.currencyPerUsd,confirmedAt:123}]:[]},seen:{url:string;method:string;body:unknown;key:string|null}[]=[];
  const command=payrollReferenceCommands(()=>({base:'/holdings/acme',view}),async(url,init)=>{const method=init?.method??'GET',key=new Headers(init?.headers).get('Idempotency-Key');seen.push({url:String(url),method,body:init?.body?JSON.parse(String(init.body)):null,key});return Response.json({data:String(url).endsWith('/confirmations')?{kind:singular[request.kind],commandId:key,payGroupId:group.id,id,status:request.kind==='rates'?'confirmed':request.kind==='periods'?'open':'active',version:request.kind==='rates'?null:request.kind==='periods'?0:1}:method==='POST'?receipt:next});});await command.submit(request);
  const base='/api/v1/holdings/acme/people/payroll/posting-prerequisites';expect(seen.map(row=>[row.method,row.url])).toEqual([['POST',base+'/'+request.kind],['POST',base+'/confirmations'],['GET',base+'?payGroupId=group-original']]);expect(seen[0].body).toEqual(request.body);expect(seen[1].body).toEqual({kind:singular[request.kind],request:request.body});expect(seen[1].key).toBe(seen[0].key);for(const row of seen){expect(productPageAllowed('people',new URL(row.url,'https://people.example.test').pathname,row.method)).toBe(true);expect(productPageAllowed('finance',new URL(row.url,'https://people.example.test').pathname,row.method)).toBe(false);}
 }
});

it('keeps Expense setup on its exact Finance owner destination with unchanged query/hash and no adjacent page aliases',async()=>{
 const {expensePostingPrerequisitesEntry:entry,expensePostingPrerequisitesLanding:landing,expensePostingPrerequisitesPresentationAllowed:present}=await import('@erpx/server-finance/public/rules/expense-posting-prerequisites-entry'),query='?tag=one&tag=two#references';expect(entry(landing)).toBe(true);for(const path of [landing+'/',landing+'/new',landing+'/accounts',landing+'/mapping',landing+query,'finance/setup','finance/settings','finance/setup/expenses-extra'])expect(entry(path)).toBe(false);expect(canonicalUiHref('/holdings/acme/'+landing+query)).toBe('/holdings/acme/'+landing+query);expect(destinationProduct(landing)).toBe('finance');expect(destinations.find(row=>row.path===landing)).toMatchObject({source:'finance/setup/expenses'});expect(productPageAllowed('finance','/holdings/acme/'+landing+query)).toBe(true);for(const host of [...productIds,...sharedHostKeys].filter(host=>host!=='finance'))expect(productPageAllowed(host,'/holdings/acme/'+landing+query)).toBe(false);
 const context={scopeKind:'holding',modules:['finance'],permissions:['finance.read','finance.write']};expect(present(context)).toBe(true);expect(present({...context,permissions:['finance.read','finance.approve']})).toBe(true);for(const patch of [{modules:[]},{modules:['people_payroll']},{permissions:['finance.read']},{permissions:['finance.write']},{permissions:['finance.approve']},{scopeKind:'branch'},{scopeKind:'department'},{beta:true}])expect(present({...context,...patch})).toBe(false);
});
it('closes Expense setup transport to its exact Finance constructors/confirmation/mapping including residual encoded root and family names',()=>{
 const base='/api/v1/holdings/acme/finance/expense-setup';for(const [suffix,method]of [['','GET'],['','HEAD'],['/charts','POST'],['/accounts','POST'],['/periods','POST'],['/rates','POST'],['/confirmations','POST'],['/mapping','POST']]){expect(productPageAllowed('finance',base+suffix,method),suffix+':'+method).toBe(true);for(const host of [...productIds,...sharedHostKeys].filter(host=>host!=='finance'))expect(productPageAllowed(host,base+suffix,method),host+':'+suffix+':'+method).toBe(false);}
 for(const [suffix,method]of [['','POST'],['','PATCH'],['','DELETE'],['/charts','GET'],['/accounts','HEAD'],['/periods','PUT'],['/rates','DELETE'],['/confirmations','GET'],['/mapping','GET'],['/mapping','OPTIONS'],['/charts/record','POST'],['/accounts/record','GET'],['/references','GET'],['/confirmations/key','POST'],['-extra/charts','POST'],['//charts','POST'],['/%2fcharts','POST'],['/%255ccharts','POST']])expect(productPageAllowed('finance',base+suffix,method),suffix+':'+method).toBe(false);
 for(const path of ['%2566inance/expense-setup/charts','finance/%2565xpense-setup/charts','%2566inance/%2565xpense-setup/charts','finance/expense-setup/%2563harts','finance/expense-setup/%255cmapping','finance/expense-setup/mapping/extra'])expect(productPageAllowed('finance','/api/v1/holdings/acme/'+path,'POST'),path).toBe(false);expect(productPageAllowed('finance','/api/v1/holdings/acme/%66inance/%65xpense-setup/%63harts','POST')).toBe(true);expect(productPageAllowed('finance','/api/v1/holdings/acme/finance/expense-setup/%6dapping','POST')).toBe(true);
});
import {createExpenseSetupCommands as expenseOwnerCommands,expensePostingRequest as expenseOwnerRequest,expensePostingRoles as expenseOwnerRoles,type ExpenseSetupSnapshot as ExpenseOwnerSnapshot,type ExpenseSetupRequest as ExpenseOwnerRequest} from '@erpx/feature-finance/lib/expense-posting-setup-command';
it('uses the actual Expense UI URLs for all original reference constructors/native confirmations and distinct initial mapping without generic Finance resources',async()=>{
 const chart={id:'chart-actual',name:'Actual chart',baseCurrency:'THB'},accounts=expenseOwnerRoles.map(([field,category,controlKind],index)=>({id:field+'-actual',chartId:chart.id,code:String(1000+index),name:'Actual '+field,category,currency:'THB',controlKind})),configuration={chartId:chart.id,currency:'THB',...Object.fromEntries(expenseOwnerRoles.map(([field])=>[field,field+'-actual']))}as NonNullable<ExpenseOwnerSnapshot['mapping']['configuration']>,view:ExpenseOwnerSnapshot={prerequisites:{companyId:'company',available:true,configured:false,contextKey:'a'.repeat(64),defaultCurrency:'USD',currencies:['THB','USD'],canCreateChart:true,canCreateAccount:true,canCreatePeriod:true,canConfirmRate:true,charts:[chart],accounts,periods:[],rates:[],unavailableReason:null},mapping:{companyId:'company',available:true,configured:false,version:0,canConfigure:true,contextKey:'b'.repeat(64),configuration:null,charts:[chart],accounts,expectedSalesDefaults:null,approvedSalesDefaults:null,unavailableReason:null}},requests:ExpenseOwnerRequest[]=[expenseOwnerRequest(view,'charts',{name:'Actual new chart',baseCurrency:'THB'}),expenseOwnerRequest(view,'accounts',{chartId:chart.id,name:'Actual payable',code:'2100',purpose:'payable',currency:'THB'}),expenseOwnerRequest(view,'periods',{name:'Actual period',startDate:'2026-10-01',endDate:'2026-10-31'}),expenseOwnerRequest(view,'rates',{currency:'THB',effectiveDate:'2026-10-01',currencyPerUsd:'32.10',note:'Actual confirmed rate'}),expenseOwnerRequest(view,'mapping',{...configuration,reason:'Reviewed nine original roles'})],singular={charts:'chart',accounts:'account',periods:'period',rates:'rate'}as const;
 for(const request of requests){const id=request.kind==='mapping'?'company':{charts:'chart-native-original',accounts:'acct-native-original',periods:'period-native-original',rates:'fx-native-original'}[request.kind],receipt=request.kind==='mapping'?{id,status:'configured',version:1}:request.kind==='rates'?{id,currency:'THB',effectiveDate:request.body.input.effectiveDate,currencyPerUsd:request.body.input.currencyPerUsd,confirmedAt:123}:{...request.body.input,id,companyId:'company',version:request.kind==='periods'?0:1,...(request.kind==='periods'?{state:'open'}:{})},next:ExpenseOwnerSnapshot=request.kind==='mapping'?{prerequisites:{...view.prerequisites,configured:true,canCreateChart:false},mapping:{...view.mapping,configured:true,version:1,canConfigure:false,configuration}}:view,seen:{url:string;method:string;body:unknown;key:string|null}[]=[],commands=expenseOwnerCommands(()=>({base:'/holdings/acme',view}),async(url,init)=>{const method=init?.method??'GET',key=new Headers(init?.headers).get('Idempotency-Key');seen.push({url:String(url),method,body:init?.body?JSON.parse(String(init.body)):null,key});return Response.json({data:String(url).endsWith('/confirmations')&&request.kind!=='mapping'?{kind:singular[request.kind],commandId:key,companyId:'company',id,status:request.kind==='rates'?'confirmed':request.kind==='periods'?'open':'active',version:request.kind==='rates'?null:request.kind==='periods'?0:1}:method==='POST'?receipt:next});});await commands.submit(request);const base='/api/v1/holdings/acme/finance/expense-setup';expect(seen.map(row=>[row.method,row.url])).toEqual([['POST',base+'/'+request.kind],...(request.kind==='mapping'?[]:[['POST',base+'/confirmations']]),['GET',base]]);expect(seen[0].body).toEqual(request.body);if(request.kind!=='mapping'){expect(seen[1].body).toEqual({kind:singular[request.kind],request:request.body});expect(seen[1].key).toBe(seen[0].key);}for(const row of seen){expect(productPageAllowed('finance',new URL(row.url,'https://finance.example.test').pathname,row.method)).toBe(true);expect(productPageAllowed('people',new URL(row.url,'https://finance.example.test').pathname,row.method)).toBe(false);}
 }
});


it("isolates the service-only Sales catalogue across every product/shared host and scoped transport", () => {
  const hosts = [...productIds, ...sharedHostKeys];
  for (const scope of ["", "branches/main/", "departments/team/", "branches/main/departments/team/"]) {
    const root = `/api/v1/holdings/acme/${scope}commerce/sales-service-products`;
    for (const method of ["GET", "HEAD", "POST"]) {
      for (const host of hosts) expect(productPageAllowed(host, root, method), `${host}:${scope}:${method}`).toBe(host === "sales");
      expect(productPageAllowed("sales", root.replace("/commerce/", "/%63ommerce/").replace("/sales-service-products", "/%73ales-service-products"), method)).toBe(true);
    }
    for (const method of ["PUT", "PATCH", "DELETE", "OPTIONS", "TRACE"]) expect(productPageAllowed("sales", root, method), method).toBe(false);
    for (const suffix of ["/stock", "/create", "/items/one", "//extra", "/%2fstock", "/%252fstock"]) {
      for (const method of ["GET", "HEAD", "POST"]) for (const host of hosts) expect(productPageAllowed(host, root + suffix, method), `${host}:${suffix}:${method}`).toBe(false);
    }
    const page = `/holdings/acme/${scope}customers-sales/sales/settings/services`;
    for (const host of hosts) expect(productPageAllowed(host, page), `${host}:${page}`).toBe(host === "sales");
  }
});
