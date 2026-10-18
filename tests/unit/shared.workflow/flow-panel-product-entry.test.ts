import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { renderToStaticMarkup } from "react-dom/server";
import type { WorkContext } from "@/server/modules/identity_access/public/authorization";
import type { FlowView } from "@/lib/shared/flow";
const pagePath="/holdings/acme/finance/accounting/journals/j1/overview?returnTo=%2Fholdings%2Facme%2Ffinance%2Faccounting%2Fjournals&tag=one&tag=two";
const request = vi.hoisted(() => ({ host: "finance-dev-erpx.jesadakorn.com", path: "" }));
vi.mock("next/headers", () => ({ headers: async () => new Headers({ host: request.host, "x-erpx-page-path": request.path }) }));
vi.mock("next/navigation", () => ({ useRouter: () => ({ refresh: vi.fn() }), usePathname: () => "/holdings/acme/finance/accounting/journals/j1/overview", useSearchParams: () => new URLSearchParams(), forbidden: () => { throw Error("FORBIDDEN"); }, notFound: () => { throw Error("NOT_FOUND"); }, redirect: (href: string) => { throw Error("REDIRECT:" + href); }, unstable_rethrow: () => {} }));
vi.mock("@/server/modules/workflow/public/document-flow", () => ({ documentFlow: () => { throw Error("Unexpected owner read with provided flow"); } }));
import { FlowPanel } from "@/components/workplace/flow-panel";

const actor: WorkContext = { userId: "user", name: "User", email: "user@example.test", assignmentId: "selected", role: "finance_approver", companyId: "company", branchId: null, departmentId: null, holdingId: "holding", holdingSlug: "acme", holdingName: "Acme", scope: { kind: "holding", holdingId: "holding", holdingSlug: "acme", holdingName: "Acme", companyId: "company" }, assignmentScope: "holding", inherited: false, modules: ["finance"], workspaces: [], assignments: [
  { id: "other", role: "employee", label: "Other role", companyId: "company", branchId: null, modules: [], scopeKind: "holding", scopeNodeId: "company" },
  { id: "selected", role: "finance_approver", label: "Selected reviewer", companyId: "company", branchId: null, modules: ["finance"], scopeKind: "holding", scopeNodeId: "company" },
] };
const flow: FlowView = { kind: "journal", recordId: "j1", recordVersion: 2, version: 3, defaultsRevision: 1, state: "draft", steps: [], current: { key: "review", action: "Approve", role: "finance_approver", teamName: "Finance", primary: { userId: "user", name: "User", teamId: null, teamName: "Finance" }, candidates: [], status: "current", blocker: null }, next: null, canAct: false, canManage: false, blocker: null, history: [] };
beforeEach(() => { vi.stubEnv("ERPX_ENVIRONMENT", "local"); vi.stubEnv("ERPX_PRODUCT_ENVIRONMENT", "development"); vi.stubEnv("ERPX_PRODUCT_ROUTING", "enabled"); vi.stubEnv("ERPX_PRODUCT_SSO", "enabled"); request.host = "finance-dev-erpx.jesadakorn.com"; request.path=pagePath; });
afterEach(() => vi.unstubAllEnvs());
async function markup(value = flow) { return renderToStaticMarkup(await FlowPanel({ actor, kind: "journal", id: "j1", base: "/holdings/acme", flow: value })); }
it("uses the product role chooser with the current exact document and repeated query instead of the Console-only role route", async () => {
  const html = await markup();
  const href = /href="([^"]+)"/.exec(html)![1].replaceAll("&amp;", "&");
  const chooser = new URL(href, "https://finance-dev-erpx.jesadakorn.com");
  expect(chooser.pathname).toBe("/choose-work-role"); expect(chooser.searchParams.get("companyId")).toBe("company"); expect(chooser.searchParams.get("next")).toBe(request.path);
  expect(html).toContain('data-product-switch="true"'); expect(html).toContain("Selected reviewer"); expect(html).not.toContain("Other role");
});
it("preserves the legacy Console recovery route", async () => {
  request.host = "console-dev-erpx.jesadakorn.com";
  expect(await markup()).toContain("/work-role?from=");
});
it("recovers invalid or foreign destination hints through Apps without creating a role grant or Console dependency", async () => {
  for(const path of ["https://foreign.test/holdings/acme/finance/accounting/journals/j1/overview", "/holdings/acme/people-organization/people/directory/current"]){
    request.path=path;const html=await markup();
    expect(html).toContain("https://apps-dev-erpx.jesadakorn.com/sso/start?next=%2F");
    expect(html).not.toContain("foreign.test");expect(html).not.toContain("/work-role?");
  }
});
it("does not create a role-switch action for another person's step or an already actionable current role", async () => {
  for (const value of [{ ...flow, canAct: true }, { ...flow, current: { ...flow.current!, primary: { ...flow.current!.primary!, userId: "other" } } }]) expect(await markup(value)).not.toContain("Choose a role for this step");
});
