import type { RequiredCaseIdentity } from "./required-case-inventory";

type MatrixRoute = { template: string; area: string; signedOut: "public" | "login-or-explicit-denial"; workspaceScopes: readonly string[]; detailRoute: boolean };
const matrixFile = "tests/e2e/uat-route-matrix.spec.ts";

/** A matching route count cannot hide a replaced or duplicated required source page. */
export function assertUatRouteSources(inventory: { pageRouteCount: number; entries: readonly { source: string; template: string }[] }, currentSources: readonly string[]): void {
  const expected = new Set(currentSources.map(source => source.replaceAll("\\", "/")));
  const actual = inventory.entries.map(entry => entry.source.replaceAll("\\", "/"));
  if (!expected.size || expected.size !== currentSources.length || inventory.pageRouteCount !== currentSources.length || actual.length !== currentSources.length || new Set(actual).size !== actual.length || actual.some(source => !expected.has(source)) || inventory.entries.some(entry => entry.template !== (entry.source.replaceAll("\\", "/").replace(/^src\/app/, "").replace(/\/page\.(?:tsx|ts|jsx|js)$/, "") || "/")))
    throw new Error("UAT_ROUTE_SOURCE_INVENTORY_MISMATCH");
}

/** Retained UAT actor policy, independent of Playwright's discovered test loops. */
export function uatMatrixActor(route: MatrixRoute): string {
  const path = route.template;
  if (path.startsWith("/platform/")) return "hotel-owner";
  if (path.startsWith("/front/")) return "pms";
  if (/\/pms(?:\/|$)/.test(path)) return "pms-holding";
  if (/\/pos(?:\/|$)/.test(path)) return "pos-holding";
  if (/\/finance(?:\/|$)|\/supplier-bills(?:\/|$)|\/stock-counts(?:\/|$)|\/stock-transfers(?:\/|$)/.test(path)) return "finance";
  if (/\/people(?:\/|$)|\/benefits(?:\/|$)|\/talent(?:\/|$)|\/overtime(?:\/|$)/.test(path)) return "people";
  if (/\/support(?:\/|$)/.test(path)) return "support";
  if (/\/crm(?:\/|$)|\/commerce\/orders(?:\/|$)/.test(path)) return "sales";
  if (/\/procurement-rfqs(?:\/|$)|\/commerce(?:\/|$)/.test(path)) return "purchasing";
  if (/\/work(?:\/|$)|\/leave(?:\/|$)|\/calendar$|\/inbox$|\/home$/.test(path)) return "employee";
  return route.area === "Front PMS/POS" ? "pms" : "owner";
}

/** Exact retained source/actor/scope identities cannot be regenerated from observed collection. */
export function requiredUatMatrixCases(routes: readonly MatrixRoute[]): RequiredCaseIdentity[] {
  if (!routes.length || new Set(routes.map(route => route.template)).size !== routes.length || routes.some(route => !route.template.startsWith("/") || !["public", "login-or-explicit-denial"].includes(route.signedOut) || !Array.isArray(route.workspaceScopes) || typeof route.detailRoute !== "boolean")) throw new Error("INVALID_UAT_MATRIX_POLICY_ROUTES");
  const titles = [
    "UAT source inventory source fingerprint matches the full current route inventory",
    "asset register form layout register title sits above the first field on desktop and mobile",
    "scoped personas and permission denial branch employee holding-wide request from branch-only employee is denied",
    ...["/holdings/[holdingSlug]/finance", "/holdings/[holdingSlug]/people/settings", "/holdings/[holdingSlug]/settings/stripe"].map(template => `scoped personas and permission denial employee denied from restricted modules ${template} returns permission state`),
  ];
  for (const route of routes) {
    const { template } = route;
    if (template.startsWith("/front-access")) {
      for (const app of ["pms", "pos"]) titles.push(`signed-out Front ${app} access ${template} · anonymous · ${app}`);
    } else {
      for (const app of template.startsWith("/front/") ? ["pms", "pos"] : ["pms"]) titles.push(`signed-out route access ${template} · anonymous · ${app}`);
    }
    if (route.signedOut === "login-or-explicit-denial") {
      if (template.startsWith("/front/")) {
        for (const app of ["pms", "pos"]) titles.push(`signed-in Front ${app} ${template} · ${app} role`);
      } else {
        const actor = uatMatrixActor(route);
        titles.push(`signed-in as ${actor} ${template} · ${actor} · holding`);
      }
    }
    if (route.workspaceScopes.includes("branch")) {
      for (const scope of ["branch", "department", "branch-department"]) titles.push(`holding, branch, department, and combined scope routes · ${scope} ${template} · scope:${scope}`);
    }
    if (route.detailRoute) titles.push(`dynamic record invalid-ID recovery ${template} · ${uatMatrixActor(route)} invalid id is handled explicitly on both viewports`);
    if (["/holdings/[holdingSlug]/home", "/holdings/[holdingSlug]/work", "/holdings/[holdingSlug]/leave"].includes(template)) {
      for (const scope of ["branch", "department"]) titles.push(`scoped personas and permission denial ${scope} employee ${template} opens in assigned ${scope}`);
    }
  }
  if (new Set(titles).size !== titles.length) throw new Error("DUPLICATE_UAT_MATRIX_REQUIRED_IDENTITY");
  return ["desktop", "mobile"].flatMap(project => titles.map(title => ({ runner: "playwright" as const, file: matrixFile, title, project: project as "desktop" | "mobile" })));
}
