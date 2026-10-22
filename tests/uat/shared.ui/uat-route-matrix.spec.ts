import { expect, test, type Page, type TestInfo } from "@playwright/test";
import { createHash } from "node:crypto";
import { appendFileSync, mkdirSync, readdirSync, readFileSync } from "node:fs";
import { relative, resolve } from "node:path";
import { uatAuthProfiles, type UatProfileId } from "../../../scripts/testing/uat-auth";
import { assertUatRouteSources } from "../../../scripts/testing/uat-required-identities";

type RouteEntry = {
  source: string;
  template: string;
  area: string;
  signedOut: "public" | "login-or-explicit-denial";
  primaryActor: string;
  workspaceScopes: string[];
  dynamicParameters: string[];
  detailRoute: boolean;
};
type Inventory = { sourceFingerprint: string; pageRouteCount: number; entries: RouteEntry[] };
type Fixture = {
  companyId: string;
  branchId?: string;
  departmentId?: string;
  customerId?: string;
  customerDuplicateIds?: string[];
  quotationId?: string;
  crmLeadId?: string;
  purchaseRequestId?: string;
  qualityDueSoonHoldId?: string;
  openingJournalId?: string;
  controlledPaymentInvoiceId?: string;
  hospitality: { companyId: string; propertyId: string; roomIds?: Record<string, string>; payrollPersonIds?: Record<string, string> };
  users: Array<{ userId: string; role: string; email: string; scope?: string; scopeNode?: string; scopeParentNode?: string }>;
};

const root = resolve(process.cwd());
function collectPageFiles(directory: string): string[] {
  return readdirSync(directory, { withFileTypes: true }).flatMap((entry) => {
    const path = resolve(directory, entry.name);
    return entry.isDirectory() ? collectPageFiles(path) : /\/page\.(?:tsx|ts|jsx|js)$/.test(path) && !relative(resolve(root,"src/app"),path).split("/").some(segment=>segment.startsWith("_")||segment.startsWith("@")) ? [path] : [];
  }).sort();
}
const sourcePageFiles = collectPageFiles(resolve(root, "src/app"));
const currentSourceHash = createHash("sha256");
for (const path of sourcePageFiles) currentSourceHash.update(relative(root, path)).update("\0").update(readFileSync(path)).update("\0");
const run = process.env.ERPX_BROWSER_SIMULATION_RUN ?? "";
if (!/^[a-f0-9]{12}$/.test(run)) throw new Error("Set ERPX_BROWSER_SIMULATION_RUN to the exact UAT simulation run.");
const inventory = JSON.parse(readFileSync(resolve(root, "tests/catalogue/uat-route-inventory.json"), "utf8")) as Inventory;
const state = JSON.parse(readFileSync(resolve(root, ".runtime/browser", run, "state.json"), "utf8")) as { fixture: Fixture; origin: string; port?: number };
if ((state.port ?? 3102) !== 3104 || state.origin !== (process.env.ERPX_E2E_BASE_URL ?? "http://127.0.0.1:3104"))
  throw new Error("UAT_ROUTE_TEST_REQUIRES_ITS_OWN_3104_SIMULATION");
const fixture = state.fixture;
const authDirectory = resolve(process.env.ERPX_UAT_AUTH_DIR ?? resolve(root, ".scratch/uat", run));
const evidenceFile = resolve(process.env.ERPX_UAT_EVIDENCE_FILE ?? resolve(authDirectory, "route-results.jsonl"));
const pmsOrigin = "http://dev-pms.localhost:3104";
const posOrigin = "http://dev-pos.localhost:3104";
const pageErrors = (page: Page) => {
  const errors: string[] = [];
  page.on("pageerror", (error) => errors.push(error.name));
  page.on("response", (response) => {
    if (response.status() >= 500 && !response.url().includes("/_next/")) {
      try { errors.push(`http-${response.status()}-${new URL(response.url()).pathname}`); } catch { errors.push(`http-${response.status()}`); }
    }
  });
  return errors;
};

const demoGuide = JSON.parse(readFileSync(resolve(root, ".runtime/browser", run, "demo-guide.json"), "utf8")) as unknown;
const hospitalityGuide = JSON.parse(readFileSync(resolve(root, ".runtime/browser", run, "hospitality-guide.json"), "utf8")) as unknown;
function collectLinks(value: unknown, output: string[] = []): string[] {
  if (typeof value === "string" && /^https?:\/\//.test(value)) output.push(value);
  else if (Array.isArray(value)) for (const child of value) collectLinks(child, output);
  else if (value && typeof value === "object") for (const child of Object.values(value)) collectLinks(child, output);
  return output;
}
const guidePaths = [...collectLinks(demoGuide), ...collectLinks(hospitalityGuide)].flatMap((href) => {
  try {
    const url = new URL(href);
    return url.origin === state.origin ? [{ pathname: url.pathname, search: url.search }] : [];
  } catch { return []; }
});

function escapeRegex(value: string) { return value.replace(/[.*+?^${}()|[\]\\]/g, "\\$&"); }
function matchesTemplate(template: string, pathname: string) {
  if (template === "/") return pathname === "/";
  let expression = "^";
  for (const segment of template.split("/").filter(Boolean)) {
    if (/^\[\[\.\.\.[^\]]+\]\]$/.test(segment)) expression += "(?:/[^/]+(?:/[^/]+)*)?";
    else if (/^\[\.\.\.[^\]]+\]$/.test(segment)) expression += "/.+";
    else if (/^\[[^\]]+\]$/.test(segment)) expression += "/[^/]+";
    else expression += `/${escapeRegex(segment)}`;
  }
  return new RegExp(`${expression}/?$`).test(pathname);
}

function preferredHolding(route: string) {
  return /\/(?:pms|pos)(?:\/|$)/.test(route) || route.startsWith("/book/") ? "riverside-lantern" : "simulation";
}
function knownId(route: string, name: string): string | undefined {
  const normalized = route.replaceAll("[holdingSlug]", preferredHolding(route));
  if (name === "propertyId") return fixture.hospitality.propertyId;
  if (name === "reportId") return undefined;
  if (name !== "id") return undefined;
  if (normalized.includes("/commerce/orders/")) return fixture.quotationId;
  if (normalized.includes("/crm/")) return fixture.crmLeadId;
  if (normalized.includes("/customer-duplicates/")) return fixture.customerDuplicateIds?.[0];
  if (normalized.includes("/customer-access/")) return fixture.customerId;
  if (normalized.includes("/customers/")) return fixture.customerId;
  if (normalized.includes("/finance/invoices/")) return fixture.controlledPaymentInvoiceId;
  if (normalized.includes("/finance/journals/")) return fixture.openingJournalId;
  if (normalized.includes("/people/clearance/")) return undefined;
  if (normalized.includes("/requests/")) return fixture.purchaseRequestId;
  if (normalized.includes("/quality/")) return fixture.qualityDueSoonHoldId;
  if (normalized.includes("/people/")) {
    const owner = fixture.users.find((user) => user.role === "holding_owner");
    return owner ? `person-${owner.userId}` : undefined;
  }
  if (normalized.includes("/work/routing/")) return fixture.quotationId;
  return undefined;
}

function buildPath(route: RouteEntry, options: { scope?: string; invalidId?: boolean; app?: "pms" | "pos" } = {}): string {
  const template = route.template;
  if (template === "/") return "/";
  if (template === "/work-role") return "/work-role?from=%2Fholdings%2Fsimulation%2Fhome";
  if (template.startsWith("/front-access")) {
    if (template === "/front-access") return "/";
    return template.replace(/^\/front-access/, "") || "/";
  }
  if (template.startsWith("/front/")) {
    const holding = "riverside-lantern";
    const company = fixture.hospitality.companyId;
    const app = options.app ?? "pms";
    return `/front/${holding}/${company}/${app}/holding`;
  }
  const routeSuffix = template.startsWith("/holdings/[holdingSlug]")
    ? template.replace(/^\/holdings\/\[holdingSlug\]/, "")
    : template;
  const values: Record<string, string> = {
    holdingSlug: preferredHolding(template),
    companySlug: fixture.hospitality.companyId,
    app: options.app ?? (/\/pos(?:\/|$)/.test(template) ? "pos" : "pms"),
    destination: "home",
    kind: template.includes("/work/department-approvals/") ? "expense" : "order",
    propertyId: fixture.hospitality.propertyId,
    path: template.includes("/platform/tasks/") ? "today" : "overview",
    section: template.includes("/pms/") ? "reservations" : "members",
  };
  for (const name of route.dynamicParameters) {
    if (name === "id" || name === "reportId" || name === "propertyId") {
      const sample = knownId(template, name);
      values[name] = options.invalidId || !sample
        ? `uat-missing-${name.toLowerCase()}`
        : sample;
    }
  }
  const replaced = routeSuffix.split("/").filter(Boolean).flatMap((segment) => {
    const optional = /^\[\[\.\.\.([^\]]+)\]\]$/.exec(segment);
    if (optional) return values[optional[1]] ? String(values[optional[1]]).split("/") : [];
    const catchAll = /^\[\.\.\.([^\]]+)\]$/.exec(segment);
    if (catchAll) return String(values[catchAll[1]] ?? "overview").split("/");
    const dynamic = /^\[([^\]]+)\]$/.exec(segment);
    return [dynamic ? String(values[dynamic[1]] ?? `uat-${dynamic[1]}`) : segment];
  });
  const suffix = `/${replaced.join("/")}`.replace(/\/$/, "");
  if (!template.startsWith("/holdings/[holdingSlug]")) return template === "/" ? "/" : suffix || "/";
  const holding = values.holdingSlug;
  const scope = options.scope ?? "holding";
  const user = fixture.users;
  const branch = user.find((entry) => entry.scope === "branch")?.scopeNode ?? "uat-branch";
  const department = user.find((entry) => entry.scope === "department")?.scopeNode ?? "uat-department";
  const departmentBranch = user.find((entry) => entry.scope === "department")?.scopeParentNode ?? branch;
  const base = scope === "branch"
    ? `/holdings/${holding}/branches/${branch}`
    : scope === "department"
      ? `/holdings/${holding}/branches/${departmentBranch}/departments/${department}`
    : scope === "branch-department"
        ? `/holdings/${holding}/branches/${departmentBranch}/departments/${department}`
        : `/holdings/${holding}`;
  return `${base}${suffix}` || base;
}

function examplePath(route: RouteEntry): { path: string; kind: "guide" | "fixture" | "placeholder" } {
  const holding = preferredHolding(route.template);
  const matches = guidePaths.filter((candidate) => matchesTemplate(route.template, candidate.pathname));
  const candidate = matches.find((entry) => entry.pathname.split("/")[2] === holding) ?? matches[0];
  if (candidate) return { path: candidate.pathname + candidate.search, kind: "guide" };
  const path = buildPath(route);
  const backed = route.dynamicParameters.some((name) => !!knownId(route.template, name));
  return { path, kind: route.detailRoute ? (backed ? "fixture" : "placeholder") : "fixture" };
}

function profileFor(route: RouteEntry): UatProfileId {
  const path = route.template;
  if (path.startsWith("/platform/")) return "hotel-owner";
  if (path.startsWith("/front/")) return "pms";
  if (/\/(?:pms)(?:\/|$)/.test(path)) return "pms-holding";
  if (/\/(?:pos)(?:\/|$)/.test(path)) return "pos-holding";
  if (/\/finance(?:\/|$)|\/supplier-bills(?:\/|$)|\/stock-counts(?:\/|$)|\/stock-transfers(?:\/|$)/.test(path)) return "finance";
  if (/\/people(?:\/|$)|\/benefits(?:\/|$)|\/talent(?:\/|$)|\/overtime(?:\/|$)/.test(path)) return "people";
  if (/\/support(?:\/|$)/.test(path)) return "support";
  if (/\/crm(?:\/|$)|\/commerce\/orders(?:\/|$)/.test(path)) return "sales";
  if (/\/procurement-rfqs(?:\/|$)|\/commerce(?:\/|$)/.test(path)) return "purchasing";
  if (/\/work(?:\/|$)|\/leave(?:\/|$)|\/calendar$|\/inbox$|\/home$/.test(path)) return "employee";
  if (route.area === "Front PMS/POS") return "pms";
  return "owner";
}

async function capture(page: Page, info: TestInfo, route: RouteEntry, accessPattern: string, path: string, kind = "fixture") {
  // An isolated webpack server may compile a route and hydrate its streamed
  // recovery screen on its first visit. Keep the content assertions intact.
  test.setTimeout(120_000);
  const errors = pageErrors(page);
  let navigationError = "";
  let status: number | undefined;
  try {
    page.setDefaultNavigationTimeout(60_000);
    const response = await page.goto(path, { waitUntil: "domcontentloaded" });
    status = response?.status();
    await page.locator("main, [role=main]").first().waitFor({ state: "visible", timeout: 45_000 });
    await page.locator("h1:visible, [role=heading][aria-level='1']:visible").first().waitFor({ state: "visible", timeout: 45_000 });
    await page.waitForFunction(() => {
      const heading = [...document.querySelectorAll("h1, [role=heading][aria-level='1']")]
        .find((element) => element.getClientRects().length > 0 && getComputedStyle(element).visibility !== "hidden");
      const text = heading?.textContent?.trim() ?? "";
      return Boolean(text) && !/^loading(?:…|\.\.\.)?$/i.test(text);
    }, null, { timeout: 45_000 });
  } catch (error) {
    navigationError = error instanceof Error ? error.name : "unknown";
  }
  const readMetrics = () => page.evaluate(() => ({
    viewportWidth: window.innerWidth,
    documentWidth: document.documentElement.scrollWidth,
    mainCount: document.querySelectorAll("main, [role=main]").length,
    headingCount: [...document.querySelectorAll("h1, [role=heading][aria-level='1']")]
      .filter((element) => element.getClientRects().length > 0 && getComputedStyle(element).visibility !== "hidden").length,
    title: document.title,
    headingText: [...document.querySelectorAll("h1, [role=heading][aria-level='1']")]
      .find((element) => element.getClientRects().length > 0 && getComputedStyle(element).visibility !== "hidden")?.textContent?.trim() ?? "",
    accessReason: document.querySelector("[data-access-reason]")?.getAttribute("data-access-reason")
      ?? (document.querySelector("[data-work-forbidden]") ? "permission" : null),
    host: window.location.host,
    pathname: window.location.pathname,
  })).catch(() => ({ viewportWidth: 0, documentWidth: 0, mainCount: 0, headingCount: 0, title: "", headingText: "", accessReason: null, host: "", pathname: "" }));
  let metrics = await readMetrics();
  if (metrics.headingCount === 0 && !navigationError) {
    await page.waitForFunction(() => [...document.querySelectorAll("h1, [role=heading][aria-level='1']")]
      .some((element) => element.getClientRects().length > 0 && getComputedStyle(element).visibility !== "hidden"), null, { timeout: 5_000 }).catch(() => {});
    metrics = await readMetrics();
  }
  const row = {
    sourceFingerprint: inventory.sourceFingerprint,
    route: route.template,
    source: route.source,
    accessPattern,
    dataCase: kind,
    requestedPath: path,
    finalHost: metrics.host,
    finalPath: metrics.pathname,
    status,
    viewport: info.project.name,
    viewportWidth: metrics.viewportWidth,
    documentWidth: metrics.documentWidth,
    mainCount: metrics.mainCount,
    headingCount: metrics.headingCount,
    headingText: metrics.headingText,
    accessReason: metrics.accessReason,
    pageErrors: errors,
    navigationError,
  };
  mkdirSync(resolve(evidenceFile, ".."), { recursive: true });
  appendFileSync(evidenceFile, `${JSON.stringify(row)}\n`, { mode: 0o600 });
  expect(navigationError, `${route.template} navigation on ${info.project.name}`).toBe("");
  expect((status ?? 0) < 500, `${route.template} returned HTTP ${status}`).toBe(true);
  expect(errors, `${route.template} browser/server errors`).toEqual([]);
  expect(metrics.mainCount, `${route.template} has one main landmark`).toBe(1);
  expect(metrics.headingCount, `${route.template} has a page heading`).toBeGreaterThan(0);
  expect(/temporarily unavailable|couldn.t load this page/i.test(metrics.headingText), `${route.template} must not land on the generic temporary-error screen`).toBe(false);
  expect(metrics.documentWidth - metrics.viewportWidth, `${route.template} horizontal overflow on ${info.project.name}`).toBeLessThanOrEqual(2);
  return { status, metrics };
}

function hrefForPublicEntry(route: RouteEntry, app: "pms" | "pos" = "pms") {
  if (route.template === "/marketing") return { baseURL: "http://dev-erpx.localhost:3104", path: "/" };
  if (route.template.startsWith("/front-access")) return { baseURL: app === "pms" ? pmsOrigin : posOrigin, path: buildPath(route) };
  if (route.template.startsWith("/front/")) return { baseURL: process.env.ERPX_E2E_BASE_URL ?? state.origin, path: buildPath(route, { app }) };
  const sample = examplePath(route);
  return { baseURL: process.env.ERPX_E2E_BASE_URL ?? state.origin, path: sample.path };
}

test.describe("UAT source inventory", uatAuthProfiles(), () => {
  test("source fingerprint matches the full current route inventory", async () => {
    assertUatRouteSources(inventory, sourcePageFiles.map(file => relative(root, file)));
    expect(inventory.pageRouteCount).toBe(sourcePageFiles.length);
    expect(inventory.pageRouteCount).toBe(inventory.entries.length);
    expect(inventory.pageRouteCount).toBeGreaterThan(0);
    expect(inventory.sourceFingerprint).toMatch(/^[a-f0-9]{64}$/);
    expect(inventory.sourceFingerprint).toBe(currentSourceHash.digest("hex"));
    expect(inventory.entries.find((entry) => entry.source === "packages/workplace/src/routes/page.tsx")?.template).toBe("/");
  });
});

const frontAccessEntries = inventory.entries.filter((entry) => entry.template.startsWith("/front-access"));
const regularEntries = inventory.entries.filter((entry) => !entry.template.startsWith("/front-access"));

test.describe("signed-out route access", uatAuthProfiles(), () => {
  test.use({ storageState: { cookies: [], origins: [] } });
  for (const route of regularEntries) {
    const apps = route.template.startsWith("/front/") ? (["pms", "pos"] as const) : (["pms"] as const);
    for (const app of apps) {
      const target = hrefForPublicEntry(route, app);
      test(`${route.template} · anonymous · ${app}`, async ({ page }, info) => {
        const { status, metrics } = await capture(page, info, route, "anonymous", new URL(target.path, target.baseURL).href, "representative");
        const finalUrl = new URL(page.url());
        const isFrontLogin = ["dev-pms.localhost", "dev-pos.localhost"].includes(finalUrl.hostname) && ["/login", "/forgot-password", "/verify-email", "/reset-password"].includes(finalUrl.pathname);
        if (route.signedOut === "public") {
          const publicAuthPages = ["/login", "/forgot-password", "/verify-email", "/reset-password", "/signup", "/join", "/activate"];
          if (publicAuthPages.includes(route.template)) {
            expect(finalUrl.pathname, `${route.template} should remain on its public auth page`).toBe(route.template);
          } else {
            expect(finalUrl.pathname, `${route.template} should stay on its public page`).not.toBe("/login");
          }
          expect(finalUrl.pathname, `${route.template} should not be rewritten to Front sign-in`).not.toBe("/front-access/login");
        } else if (route.template.startsWith("/front/")) {
          expect(isFrontLogin || metrics.accessReason !== null, `${route.template} should require the matching Front session`).toBe(true);
        } else {
          expect(finalUrl.pathname === "/login" || finalUrl.pathname === "/work-role" || metrics.accessReason !== null, `${route.template} should require sign-in or show an explicit access state`).toBe(true);
        }
        expect(status).toBeDefined();
      });
    }
  }
});

test.describe("asset register form layout", uatAuthProfiles("owner"), () => {
  test.use({ storageState: resolve(authDirectory, "owner.json") });
  test("register title sits above the first field on desktop and mobile", async ({ page }, info) => {
    const route = inventory.entries.find((entry) => entry.template === "/holdings/[holdingSlug]/services/assets/register")!;
    await capture(page, info, route, "signed-in:owner:layout", buildPath(route), "layout-regression");
    await page.locator("#workspace-content:visible").getByText("Register asset", { exact: true }).click();
    const drawer = page.getByRole("dialog", { name: "Register asset", exact: true });
    const body = drawer.locator('[data-slot="dialog-body"]');
    const form = drawer.locator("form.ui-form-grid");
    const heading = form.getByRole("heading", { name: "Register asset", exact: true });
    const firstLabel = form.locator("label").filter({ has: page.locator('select[name="branchId"]') });
    await expect(form).toHaveCount(1);
    await expect(drawer).toHaveAttribute("data-presentation", "form-drawer");
    await expect(body).toHaveCount(1);
    await expect(body).toBeVisible();
    await expect(firstLabel).toHaveCount(1);
    await expect(heading).toBeVisible();
    await expect(firstLabel).toBeVisible();
    const headingBox = await heading.boundingBox();
    const bodyBox = await body.boundingBox();
    const labelBox = await firstLabel.boundingBox();
    expect(headingBox && labelBox && headingBox.y + headingBox.height <= labelBox.y + 2,
      "Register asset title must be above the Branch field").toBe(true);
    expect(headingBox && bodyBox && headingBox.x - bodyBox.x >= 16,
      "Register asset title must respect the drawer body's horizontal inset").toBe(true);
    await info.attach("register-asset-drawer-layout", { body: await page.screenshot({ fullPage: true }), contentType: "image/png" });
    await drawer.getByRole("button", { name: "Close dialog", exact: true }).click();
    await expect(drawer).not.toBeVisible();
  });
});

for (const app of ["pms", "pos"] as const) {
  test.describe(`signed-out Front ${app} access`, uatAuthProfiles(), () => {
    test.use({ baseURL: app === "pms" ? pmsOrigin : posOrigin, storageState: { cookies: [], origins: [] } });
    for (const route of frontAccessEntries) {
      test(`${route.template} · anonymous · ${app}`, async ({ page }, info) => {
        const { status } = await capture(page, info, route, "anonymous-front", buildPath(route), "public-front-access");
        expect(status).toBeDefined();
        if (route.template.endsWith("/login")) await expect(page.locator('input[type="email"]')).toBeVisible();
      });
    }
  });
}

const privateEntries = inventory.entries.filter((entry) => entry.signedOut === "login-or-explicit-denial");
const grouped = new Map<UatProfileId, RouteEntry[]>();
for (const route of privateEntries) {
  if (route.template.startsWith("/front/")) continue;
  const profile = profileFor(route);
  grouped.set(profile, [...(grouped.get(profile) ?? []), route]);
}

const frontRouteEntries = privateEntries.filter((route) => route.template.startsWith("/front/"));
for (const app of ["pms", "pos"] as const) {
  test.describe(`signed-in Front ${app}`, uatAuthProfiles(app), () => {
    test.use({ storageState: resolve(authDirectory, `${app}.json`) });
    for (const route of frontRouteEntries) {
      test(`${route.template} · ${app} role`, async ({ page }, info) => {
        await capture(page, info, route, `signed-in:${app}`, buildPath(route, { app }), "front-workspace");
      });
    }
  });
}

for (const [profile, routes] of grouped) {
  test.describe(`signed-in as ${profile}`, uatAuthProfiles(profile), () => {
    test.use({ storageState: resolve(authDirectory, `${profile}.json`) });
    for (const route of routes) {
      const cases = route.template.startsWith("/front/") ? (["pms", "pos"] as const) : ([undefined] as const);
      for (const app of cases) {
        const sample = examplePath(route);
        const path = route.template.startsWith("/front/") ? buildPath(route, { app }) : sample.path;
        test(`${route.template} · ${profile} · ${app ?? "holding"}`, async ({ page }, info) => {
          const { status, metrics } = await capture(page, info, route, `signed-in:${profile}`, path, sample.kind);
          const finalUrl = new URL(page.url());
          if (profile === "pms-holding" && route.template === "/holdings/[holdingSlug]/pms/[[...section]]") {
            expect(finalUrl.hostname).toBe("dev-pms.localhost");
            expect(finalUrl.pathname).toBe("/login");
          } else if (profile === "pos-holding" && route.template === "/holdings/[holdingSlug]/pos") {
            expect(finalUrl.hostname).toBe("dev-pos.localhost");
            expect(finalUrl.pathname).toBe("/login");
          } else {
            expect(finalUrl.pathname, `${route.template} must not stop at the work-role picker`).not.toBe("/work-role");
            expect(finalUrl.pathname, `${route.template} must preserve its signed-in session`).not.toBe("/login");
          }
          if (route.area === "Platform") {
            const legacyDenial = finalUrl.pathname === "/access" && ["platform", "workspace", "permission"].includes(metrics.accessReason ?? "");
            const managedDenial = /^\/holdings\/erpx\/manage-erpx(?:\/|$)/.test(finalUrl.pathname) && status === 403;
            expect(legacyDenial || managedDenial, "holding access must be explicitly denied platform administration").toBe(true);
            if (managedDenial) {
              await expect(page.locator("[data-work-forbidden]").first()).toBeVisible();
              await expect(page.getByRole("heading", { name: "Your role cannot open this view", exact: true })).toBeVisible();
            }
          } else if (route.detailRoute && sample.kind !== "placeholder") {
            expect(status === 404 || status === 403 || metrics.accessReason === "not_found" || metrics.accessReason === "permission" || status! < 400,
              `${route.template} valid sample should load or explain access`).toBe(true);
          }
        });
      }
    }
  });
}

const scopedRoutes = inventory.entries.filter((route) => route.workspaceScopes.includes("branch"));
for (const scope of ["branch", "department", "branch-department"] as const) {
  test.describe(`holding, branch, department, and combined scope routes · ${scope}`, uatAuthProfiles(scope === "branch" ? "owner-branch" : "owner-department"), () => {
    test.use({ storageState: resolve(authDirectory, scope === "branch" ? "owner-branch.json" : "owner-department.json") });
    for (const route of scopedRoutes) {
      const path = buildPath(route, { scope });
      test(`${route.template} · scope:${scope}`, async ({ page }, info) => {
        const leaveCalendarResponse = route.template === "/holdings/[holdingSlug]/leave"
          ? page.waitForResponse((response) => new URL(response.url()).pathname.endsWith("/leave/calendar"))
          : null;
        await capture(page, info, route, `holding-owner:${scope}`, path, "scope-route");
        if (leaveCalendarResponse) {
          expect((await leaveCalendarResponse).status(), `${route.template} leave calendar for ${scope}`).toBe(200);
        }
      });
    }
  });
}

const invalidEntries = inventory.entries.filter((route) => route.detailRoute);
test.describe("dynamic record invalid-ID recovery", () => {
  for (const route of invalidEntries) {
    const profile = profileFor(route);
    test.describe(`${route.template} · ${profile}`, uatAuthProfiles(profile), () => {
      test.use({ storageState: resolve(authDirectory, `${profile}.json`) });
      test(`invalid id is handled explicitly on both viewports`, async ({ page }, info) => {
        const path = buildPath(route, { invalidId: true });
        const { status, metrics } = await capture(page, info, route, `invalid-id:${profile}`, path, "invalid-id");
        const mainText = await page.locator("body").innerText();
        const notFoundPage = /404\s*·\s*Not found/i.test(mainText) || /couldn.t find this page/i.test(mainText);
        expect(status === 404 || status === 403 || ["not_found", "permission", "invalid", "workspace"].includes(metrics.accessReason ?? "") || notFoundPage,
          `${route.template} must show not-found or an access boundary for an invalid ID`).toBe(true);
      });
    });
  }
});

test.describe("scoped personas and permission denial", () => {
  test.describe("branch employee", uatAuthProfiles("branch"), () => {
    test.use({ storageState: resolve(authDirectory, "branch.json") });
    for (const route of inventory.entries.filter((entry) => ["/holdings/[holdingSlug]/home", "/holdings/[holdingSlug]/work", "/holdings/[holdingSlug]/leave"].includes(entry.template))) {
      test(`${route.template} opens in assigned branch`, async ({ page }, info) => {
        await capture(page, info, route, "branch-employee:allowed", buildPath(route, { scope: "branch" }), "scoped-persona");
      });
    }
    test("holding-wide request from branch-only employee is denied", async ({ page }, info) => {
      const route = inventory.entries.find((entry) => entry.template === "/holdings/[holdingSlug]/home")!;
      const { metrics } = await capture(page, info, route, "branch-employee:outside-scope", buildPath(route), "scope-denial");
      expect(["scope", "no_access", "permission"].includes(metrics.accessReason ?? "") || page.url().includes("/work-role"), "branch-only role must not get a holding-wide session").toBe(true);
    });
  });
  test.describe("department employee", uatAuthProfiles("department"), () => {
    test.use({ storageState: resolve(authDirectory, "department.json") });
    for (const route of inventory.entries.filter((entry) => ["/holdings/[holdingSlug]/home", "/holdings/[holdingSlug]/work", "/holdings/[holdingSlug]/leave"].includes(entry.template))) {
      test(`${route.template} opens in assigned department`, async ({ page }, info) => {
        await capture(page, info, route, "department-employee:allowed", buildPath(route, { scope: "department" }), "scoped-persona");
      });
    }
  });
  test.describe("employee denied from restricted modules", uatAuthProfiles("employee"), () => {
    test.use({ storageState: resolve(authDirectory, "employee.json") });
    for (const template of ["/holdings/[holdingSlug]/finance", "/holdings/[holdingSlug]/people/settings", "/holdings/[holdingSlug]/settings/stripe"]) {
      const route = inventory.entries.find((entry) => entry.template === template)!;
      test(`${template} returns permission state`, async ({ page }, info) => {
        const { metrics } = await capture(page, info, route, "employee:denied", buildPath(route), "permission-denial");
        if (template === "/holdings/[holdingSlug]/people/settings") {
          if (new URL(page.url()).pathname.endsWith("/people")) {
            await expect(page.getByText("Read-only organization explorer", { exact: true }), `${template} must explain its read-only access`).toBeVisible();
            await expect(page.getByRole("button", { name: "HR settings", exact: true })).toHaveCount(0);
            await expect(page.getByRole("button", { name: "+ Add", exact: true })).toHaveCount(0);
            expect(new URL(page.url()).searchParams.get("panel")).toBe("settings");
          } else {
            await expect(page.getByText("Your selected role does not manage these settings.", { exact: true }), `${template} must explain its read-only denial`).toBeVisible();
          }
        } else {
          expect(metrics.accessReason === "permission" || page.url().includes("/access"), `${template} denied with an explicit state`).toBe(true);
        }
      });
    }
  });
});


// A real, deliberately partial observation producer. Existing cases/assertions above remain intact.
import { execFileSync as assetEvidenceExecFileSync } from "node:child_process";
import { existsSync as assetEvidenceExists, lstatSync as assetEvidenceStat, realpathSync as assetEvidenceRealpath, writeFileSync as assetEvidenceWrite } from "node:fs";
import { dirname as assetEvidenceDirname, isAbsolute as assetEvidenceAbsolute } from "node:path";
import { runtimeSourceFingerprint as assetRuntimeFingerprint, verificationSourceFingerprint as assetVerificationFingerprint } from "../../../scripts/local/source-fingerprint";

test.describe("asset register UI observations", uatAuthProfiles("owner"), () => {
  test.use({ storageState: resolve(authDirectory, "owner.json"), deviceScaleFactor: 1 });
  test("native drawer geometry, focus and Escape at five widths and all appearance modes", async ({ page, context }, info) => {
    test.setTimeout(180_000);
    const hash = (bytes: Uint8Array | string) => createHash("sha256").update(bytes).digest("hex");
    type Surface = { component: string; line: number; props: Record<string, unknown>; conditions: string[] };
    type Source = { source: string; fingerprint: string; surfaces: Surface[] };
    type Route = { source: string; route: string; fingerprint: string; closureIds: number[]; styles: string[]; dependencies: string[]; routeContext: { kind: string } };
    type NativeState = { run: string; prefix: string; sourceFingerprint: string; softwareVersion: string; origin: string; port: number; ready: boolean; diagnostic?: boolean; pid: number; jobsPid: number; jobsPaused?: boolean; webEntrypoint: string; jobsEntrypoint: string; release: { id: string; fileManifestSha256: string }; fixture: { companyId: string } };
    const originalStateBytes = readFileSync(resolve(root, ".runtime/browser", run, "state.json"));
    const nativeState = JSON.parse(originalStateBytes.toString()) as NativeState;
    expect(nativeState).toMatchObject({ run, prefix: `erpx_browser_${run}`, origin: "http://127.0.0.1:3104", port: 3104, ready: true });
    expect(nativeState.diagnostic).toBeFalsy(); expect(nativeState.jobsPaused).not.toBe(true);
    expect(Number.isInteger(nativeState.pid) && nativeState.pid > 0).toBe(true);
    expect(Number.isInteger(nativeState.jobsPid) && nativeState.jobsPid > 0).toBe(true);
    expect(nativeState.webEntrypoint).toBe(resolve(root, ".runtime/browser", run, "release/server.js"));
    expect(nativeState.jobsEntrypoint).toBe(resolve(root, ".runtime/browser", run, "release/tools/jobs.mjs"));
    const runtimeFingerprint = await assetRuntimeFingerprint(root), verificationFingerprint = await assetVerificationFingerprint(root, runtimeFingerprint);
    const candidate = assetEvidenceExecFileSync("git", ["rev-parse", "HEAD"], { cwd: root, encoding: "utf8" }).trim();
    expect(candidate).toMatch(/^[a-f0-9]{40}$/); expect(nativeState.sourceFingerprint).toBe(runtimeFingerprint);
    expect(nativeState.softwareVersion).toBe(`release-${runtimeFingerprint}`);
    expect(nativeState.release.id).toMatch(/^\d{4}-\d{2}-\d{2}T\d{2}-\d{2}-\d{2}-\d{3}Z$/);
    const releaseDirectory = resolve(root, ".runtime/releases", nativeState.release.id);
    expect(assetEvidenceRealpath.native(releaseDirectory)).toBe(releaseDirectory);
    const release = JSON.parse(readFileSync(resolve(releaseDirectory, "release.json"), "utf8")) as { id: string; node: string; postgres: string; sourceFingerprint: string; fileManifestSha256: string };
    expect(release).toMatchObject({ ...nativeState.release, node: "26.11.1", postgres: "18.6", sourceFingerprint: runtimeFingerprint });
    expect(hash(readFileSync(resolve(releaseDirectory, "release-files.json")))).toBe(nativeState.release.fileManifestSha256);
    const coverageBytes = readFileSync(resolve(root, "docs/ux-ui/standard-coverage.json"));
    const coverage = JSON.parse(coverageBytes.toString()) as { closurePaths: string[]; sources: Source[]; routes: Route[] };
    const routeSource = "packages/feature-assets/src/routes/holdings/[holdingSlug]/services/assets/register/page.tsx", targetSource = "packages/feature-assets/src/components/asset-editor.tsx";
    const routes = coverage.routes.filter(row => row.source === routeSource), sources = coverage.sources.filter(row => row.source === targetSource);
    expect(routes).toHaveLength(1); expect(sources).toHaveLength(1);
    const route = routes[0], source = sources[0];
    expect(route.route).toBe("/holdings/[holdingSlug]/services/assets/register"); expect(route.routeContext.kind).toBe("children");
    const closure = route.closureIds.map(index => coverage.closurePaths[index]);
    expect(route.dependencies).toContain(targetSource); expect(closure).toContain(routeSource);
    expect(source.fingerprint).toBe(hash(readFileSync(resolve(root, targetSource))));
    const actualRouteHash = hash([...closure, ...route.styles].map(path => `${path}:${hash(readFileSync(resolve(root, path)))}`).sort().join("\n"));
    expect(route.fingerprint).toBe(actualRouteHash);
    const surfaces = source.surfaces.filter(surface => surface.component === "DraftFormDrawer");
    expect(surfaces).toHaveLength(1);
    const surface = surfaces[0];
    expect(surface.props).toMatchObject({ title: "{title}", open: "{open}" });
    const surfaceId = `surface:${targetSource}:${hash(JSON.stringify([surface.component, surface.line, surface.props, surface.conditions]))}`;
    // Never copy uiTargetRequirements(): only these interactions are actually asserted below.
    const checks = ["loaded", "keyboard", "focus", "escape", "portalled popup/draft focus", surfaceId];
    const canonical = `/holdings/simulation/services/assets/register`;
    const writes: string[] = [];
    page.on("request", request => { const url = new URL(request.url()); if (!["GET", "HEAD"].includes(request.method()) && url.pathname.startsWith("/api/v1/holdings/simulation/assets")) writes.push(url.pathname); });
    for (const [theme, scheme] of [["light", "light"], ["dark", "dark"], ["system", "light"], ["system", "dark"]] as const) {
      await context.addCookies([{ name: "erpx_appearance", value: theme, url: nativeState.origin }]);
      await page.emulateMedia({ colorScheme: scheme });
      for (const width of [320, 390, 768, 1024, 1440]) {
        await page.setViewportSize({ width, height: 900 });
        await page.goto(nativeState.origin + canonical);
        await expect(page).toHaveURL(nativeState.origin + canonical);
        await expect(page.locator("#workspace-content:visible").getByRole("heading", { name: "Assets & maintenance", level: 1, exact: true })).toBeVisible();
        await expect(page.locator("html")).toHaveAttribute("data-theme", theme);
        const resolvedTheme = await page.locator("html").evaluate(element => getComputedStyle(element).colorScheme);
        expect(resolvedTheme).toBe(scheme);
        expect(await page.evaluate(() => ({ width: innerWidth, height: innerHeight, scale: devicePixelRatio }))).toEqual({ width, height: 900, scale: 1 });
        const authority = await context.request.get(nativeState.origin + "/api/v1/holdings/simulation/assignments/active");
        expect(authority.status()).toBe(200);
        const active = (await authority.json()).data as { valid: boolean; companyId: string; userId: string; assignmentId: string; revision: number; options: { id: string; role: string }[] };
        await authority.dispose();
        expect(active.valid).toBe(true); expect(active.companyId).toBe(nativeState.fixture.companyId);
        expect(active.options.find(option => option.id === active.assignmentId)?.role).toBe("holding_owner");
        const stamp = page.locator("[data-work-assignment]");
        await expect(stamp).toHaveCount(1); await expect(stamp).toHaveAttribute("data-work-stale", "false");
        await expect(stamp).toHaveAttribute("data-work-company", active.companyId); await expect(stamp).toHaveAttribute("data-work-user", active.userId);
        await expect(stamp).toHaveAttribute("data-work-assignment", active.assignmentId); await expect(stamp).toHaveAttribute("data-work-revision", String(active.revision));
        const opener = page.locator("#workspace-content:visible").getByRole("button", { name: "Register asset", exact: true });
        await expect(opener).toBeEnabled(); await opener.focus(); await expect(opener).toBeFocused(); await page.keyboard.press("Enter");
        const drawer = page.getByRole("dialog", { name: "Register asset", exact: true });
        await expect(drawer).toBeVisible(); await expect(drawer).toHaveAttribute("data-presentation", "form-drawer");
        expect(await drawer.evaluate(element => !document.querySelector("#workspace-content")?.contains(element))).toBe(true);
        const body = drawer.locator('[data-slot="dialog-body"]'), form = drawer.locator("form.ui-form-grid"), heading = form.getByRole("heading", { name: "Register asset", exact: true });
        const firstLabel = form.locator("label").filter({ has: page.locator('select[name="branchId"]') }), branch = form.getByRole("combobox", { name: "Branch", exact: true });
        await expect(form).toHaveCount(1); await expect(body).toBeVisible(); await expect(heading).toBeVisible(); await expect(firstLabel).toBeVisible(); await expect(branch).toBeFocused();
        const headingBox = await heading.boundingBox(), bodyBox = await body.boundingBox(), labelBox = await firstLabel.boundingBox(), drawerBox = await drawer.boundingBox();
        expect(headingBox && labelBox && headingBox.y + headingBox.height <= labelBox.y + 2, "Native Register asset title is above Branch").toBe(true);
        expect(headingBox && bodyBox && headingBox.x - bodyBox.x >= 16, "Native drawer keeps its body inset").toBe(true);
        expect(drawerBox && drawerBox.x >= -1 && drawerBox.x + drawerBox.width <= width + 1 && drawerBox.y >= -1 && drawerBox.y + drawerBox.height <= 901, "Native drawer fits the actual viewport").toBe(true);
        if (width < 640) expect(drawerBox && Math.abs(drawerBox.width - width) <= 1, "Native small-screen drawer spans the viewport").toBe(true);
        expect(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth + 1)).toBe(true);
        const controls = drawer.locator('button:not([disabled]),a[href],input:not([type="hidden"]):not([disabled]),select:not([disabled]),textarea:not([disabled]),[tabindex="0"]').filter({ visible: true });
        expect(await controls.count()).toBeGreaterThan(1);
        await controls.last().focus(); await page.keyboard.press("Tab"); await expect(controls.first()).toBeFocused();
        await controls.first().focus(); await page.keyboard.press("Shift+Tab"); await expect(controls.last()).toBeFocused();
        await page.keyboard.press("Escape"); await expect(drawer).not.toBeVisible(); await expect(opener).toBeFocused();
        await page.keyboard.press("Enter"); await expect(drawer).toBeVisible(); await expect(branch).toBeFocused();
        const label = `asset-register-${width}-${theme}-${scheme}`, imagePath = info.outputPath(label + ".png");
        const output = relative(resolve(root, ".scratch/dev/test-suites/browser"), imagePath).split("/");
        expect(output.length).toBeGreaterThan(3); expect(output[1]).toBe(run); expect(output[2]).toBe("artifacts");
        expect(output.every(part => part && part !== "." && part !== "..")).toBe(true);
        expect(assetEvidenceRealpath.native(assetEvidenceDirname(imagePath))).toBe(assetEvidenceDirname(imagePath)); expect(assetEvidenceExists(imagePath)).toBe(false);
        const image = await page.screenshot({ path: imagePath, type: "png", fullPage: false, scale: "css" });
        expect(image.readUInt32BE(16)).toBe(width); expect(image.readUInt32BE(20)).toBe(900);
        const imageAttachmentIndex = info.attachments.length;
        await info.attach("asset-register-ui-image", { path: imagePath, contentType: "image/png" });
        expect(info.attachments).toHaveLength(imageAttachmentIndex + 1);
        const imageAttachment = info.attachments[imageAttachmentIndex];
        expect(imageAttachment).toMatchObject({ name: "asset-register-ui-image", contentType: "image/png" });
        if (!imageAttachment.path) throw new Error("ORIGINAL_PLAYWRIGHT_IMAGE_ATTACHMENT_REQUIRED");
        // attach(path) retains its own copy. Bind that public reported path, never guess its filename.
        const retainedImagePath = imageAttachment.path, retainedImage = readFileSync(retainedImagePath);
        expect(retainedImage.equals(image)).toBe(true);
        expect(retainedImagePath.startsWith(assetEvidenceDirname(imagePath) + "/")).toBe(true);
        expect(assetEvidenceRealpath.native(retainedImagePath)).toBe(retainedImagePath);
        expect(assetEvidenceStat(retainedImagePath).isFile()).toBe(true);
        const imageBinding = { path: relative(root, retainedImagePath).replaceAll("\\", "/"), sha256: hash(retainedImage), bytes: retainedImage.length };
        const ownerContext = JSON.stringify({ companyId: active.companyId, userId: active.userId, assignmentId: active.assignmentId, revision: active.revision });
        for (const target of [{ kind: "route", source: routeSource, fingerprint: route.fingerprint }, { kind: "source", source: targetSource, fingerprint: source.fingerprint }]) {
          const observationPath = info.outputPath(label + "-" + target.kind + ".json");
          expect(assetEvidenceAbsolute(observationPath) && observationPath.startsWith(assetEvidenceDirname(imagePath) + "/")).toBe(true);
          const observation = { schema: "erpx-ui-browser-observation/v1", result: "passed", run, candidate, sourceFingerprint: runtimeFingerprint, verificationFingerprint, releaseId: release.id, releaseManifestSha256: release.fileManifestSha256, targetSource: target.source, targetKind: target.kind, targetFingerprint: target.fingerprint, routeSource, url: page.url(), ownerContext, authorityBranch: "allowed:actual-native-Company-holding_owner", authentication: "normal-native-owner", viewport: { width, height: 900 }, deviceScaleFactor: 1, theme, resolvedTheme, checks, screenshot: imageBinding };
          assetEvidenceWrite(observationPath, JSON.stringify(observation, null, 2) + "\n", { flag: "wx", mode: 0o600 });
          expect(assetEvidenceStat(observationPath).isFile()).toBe(true);
          await info.attach("erpx-ui-observation", { path: observationPath, contentType: "application/json" });
        }
        await page.keyboard.press("Escape"); await expect(drawer).not.toBeVisible(); await expect(opener).toBeFocused();
      }
    }
    expect(writes).toEqual([]);
    expect(readFileSync(resolve(root, ".runtime/browser", run, "state.json")).equals(originalStateBytes)).toBe(true);
    expect(readFileSync(resolve(root, "docs/ux-ui/standard-coverage.json")).equals(coverageBytes)).toBe(true);
    expect(await assetRuntimeFingerprint(root)).toBe(runtimeFingerprint);
    expect(await assetVerificationFingerprint(root, runtimeFingerprint)).toBe(verificationFingerprint);
  });
});

import {assetRegisterPath,assetUiCapture,assetUiDiscard,assetUiFill,assetUiHeaders,assetUiWork,assetUiWorkspace} from '../../support/browser/asset-ui-observation';

test.describe('asset register native state observations',uatAuthProfiles('owner'),()=>{
 test.use({storageState:resolve(authDirectory,'owner.json'),deviceScaleFactor:1});
 test('real native records, filters, validation, pending and draft guards across the appearance matrix',async({page,context},info)=>{
  test.setTimeout(600_000);
  const proof=await assetUiCapture(page,info,state.origin),tag=`UI-${run}-${info.project.name}-${crypto.randomUUID().slice(0,8)}`,today=new Date().toISOString().slice(0,10),thai='เครื่องตรวจสอบความปลอดภัยประจำอาคารและพื้นที่ปฏิบัติงานสำหรับเจ้าหน้าที่ผู้รับผิดชอบ '.repeat(2).slice(0,150);
  let active=await assetUiWork(context,state.origin,proof.companyId);expect(active.valid).toBe(true);expect(active.options.find(option=>option.id===active.assignmentId)?.role).toBe('holding_owner');
  const before=await assetUiWorkspace(context,state.origin,active);expect(before.canCreate).toBe(true);expect(fixture.branchId).toEqual(expect.any(String));expect(before.nodes.filter(node=>node.id===fixture.branchId&&['branch','headquarters'].includes(node.kind))).toHaveLength(1);
  const created:{id:string;code:string;name:string}[]=[];
  // These are new records in THIS disposable native cohort, created through
  // the actual public command with normal owner authority and Money snapshot.
  for(let index=0;index<20;index++){
   const key=crypto.randomUUID(),code=`${tag}-${index}`,name=`${thai.slice(0,140)} ${index}`;
   const response=await context.request.post(state.origin+'/api/v1/holdings/simulation/assets',{headers:{...assetUiHeaders(active),Origin:state.origin,'Idempotency-Key':key},data:{branchId:fixture.branchId,code,name,category:tag,currency:'THB',cost:'123456.78',acquiredDate:today}});
   try{expect(response.status()).toBe(200);expect((await response.json()).data).toEqual({id:`asset-${key}`,version:1});created.push({id:`asset-${key}`,code,name});}finally{await response.dispose();}
  }
  const populated=await assetUiWorkspace(context,state.origin,active);
  for(const record of created)expect(populated.items.find(item=>item.id===record.id)).toMatchObject({...record,version:1,category:tag,currency:'THB',cost_minor:12345678});
  let commandPosts=0;page.on('request',request=>{if(request.method()==='POST'&&new URL(request.url()).pathname==='/api/v1/holdings/simulation/assets')commandPosts++;});
  for(const [theme,scheme] of [['light','light'],['dark','dark'],['system','light'],['system','dark']] as const){
   await context.addCookies([{name:'erpx_appearance',value:theme,url:state.origin}]);await page.emulateMedia({colorScheme:scheme});
   for(const width of [320,390,768,1024,1440]){
    const label=`asset-native-${width}-${theme}-${scheme}`;await page.setViewportSize({width,height:900});await page.goto(state.origin+assetRegisterPath);
    const content=page.locator('#workspace-content:visible'),search=content.getByRole('searchbox',{name:'Search this view',exact:true}),searchForm=content.getByRole('search'),table=content.getByRole('table');
    await expect(content.getByRole('heading',{name:'Assets & maintenance',level:1,exact:true})).toBeVisible();await expect(page.locator('html')).toHaveAttribute('data-theme',theme);expect(await page.locator('html').evaluate(element=>getComputedStyle(element).colorScheme)).toBe(scheme);
    await search.fill(tag);await searchForm.getByRole('button',{name:'Search',exact:true}).click();await expect(page).toHaveURL(url=>url.pathname===assetRegisterPath&&url.searchParams.get('q')===tag);await expect(table.locator('tbody tr')).toHaveCount(20);
    for(const record of created)await expect(table.getByRole('link',{name:`${record.code} · ${record.name}`,exact:true})).toBeVisible();
    expect(await table.evaluate(element=>element.getBoundingClientRect().width>0&&element.closest('[data-slot="table-frame"]')!==null)).toBe(true);expect(await page.evaluate(()=>document.documentElement.scrollWidth<=innerWidth+1)).toBe(true);
    await proof.observe(label+'-dense',['loaded','dense-data','long-thai'],active);
    await search.fill(tag+'-missing');await searchForm.getByRole('button',{name:'Search',exact:true}).click();await expect(table.locator('tbody tr')).toHaveCount(0);await expect(content.getByText('No records match this view in your current scope.',{exact:true})).toBeVisible();await proof.observe(label+'-empty',['empty'],active);
    await content.getByRole('link',{name:'Clear filters',exact:true}).click();await expect(search).toHaveValue('');await expect(page).toHaveURL(state.origin+assetRegisterPath);await expect(table.locator('tbody tr')).toHaveCount(populated.items.length);await proof.observe(label+'-reset',['pagination/search/reset','print if document'],active);
    // Create a genuine preceding history entry using the native GET filter.
    await search.fill(tag);await searchForm.getByRole('button',{name:'Search',exact:true}).click();await expect(page).toHaveURL(url=>url.searchParams.get('q')===tag);
    const sourceUrl=page.url(),opener=content.getByRole('button',{name:'Register asset',exact:true});await opener.click();const drawer=page.getByRole('dialog',{name:'Register asset',exact:true}),form=drawer.locator('form.ui-form-grid'),name=form.getByRole('textbox',{name:'Name',exact:true});await expect(drawer).toBeVisible();await name.fill(thai);
    const reloadDialog=page.waitForEvent('dialog'),reloading=page.reload({waitUntil:'domcontentloaded'}).then(()=>null,error=>error);const warning=await reloadDialog;expect(warning.type()).toBe('beforeunload');await warning.dismiss();await reloading;await expect(page).toHaveURL(sourceUrl);await expect(name).toHaveValue(thai);await proof.observe(label+'-reload',['dirty-reload',proof.drawerSurface],active);
    const traversing=page.goBack({waitUntil:'commit',timeout:5_000}).catch(error=>{expect(String(error)).toMatch(/ERR_ABORTED|interrupted|Timeout/);});
    const historyWarning=page.getByRole('dialog',{name:'Leave unsaved Asset work?',exact:true});await expect(historyWarning).toBeVisible();await historyWarning.getByRole('button',{name:'Keep editing',exact:true}).click();await traversing;await expect(page).toHaveURL(sourceUrl);await expect(name).toHaveValue(thai);await proof.observe(label+'-history',['history',proof.historySurface],active);await assetUiDiscard(page);
    await opener.click();await expect(drawer).toBeVisible();const initialPosts=commandPosts;await form.getByRole('button',{name:'Register asset',exact:true}).click();await expect(form.locator('input[name="code"]')).toBeFocused();expect(await form.evaluate(element=>!(element as HTMLFormElement).checkValidity())).toBe(true);expect(commandPosts).toBe(initialPosts);await proof.observe(label+'-validation',['validation'],active);
    const code=`FORM-${run}-${info.project.name}-${width}-${theme}-${scheme}`;await assetUiFill(form,code,thai,tag,today);await form.getByRole('textbox',{name:'Currency',exact:true}).fill('ZZZ');const rejected=page.waitForResponse(response=>response.request().method()==='POST'&&new URL(response.url()).pathname==='/api/v1/holdings/simulation/assets');await form.getByRole('button',{name:'Register asset',exact:true}).click();const failure=await rejected;expect(failure.status()).toBe(422);expect((await failure.json()).error).toMatchObject({code:'VALIDATION_FAILED',message:'Use a current ISO currency.'});await expect(drawer.getByRole('alert')).toHaveText('Use a current ISO currency.');await expect(name).toHaveValue(thai);await expect(form.getByRole('textbox',{name:'Asset code',exact:true})).toHaveValue(code);expect((await assetUiWorkspace(context,state.origin,active)).items.some(item=>item.code===code)).toBe(false);await proof.observe(label+'-error',['error',proof.errorSurface],active);
    await form.getByRole('textbox',{name:'Currency',exact:true}).fill('THB');await expect(form.getByRole('textbox',{name:'Currency',exact:true})).toHaveValue('THB');await expect(form.getByRole('textbox',{name:'Acquisition cost',exact:true})).toHaveValue('123456.78');
    let release!:()=>void,started!:()=>void;const held=new Promise<void>(done=>release=done),dispatched=new Promise<void>(done=>started=done),url=state.origin+'/api/v1/holdings/simulation/assets';
    const hold=async(route:import('@playwright/test').Route)=>{expect(route.request().method()).toBe('POST');started();await held;await route.continue();};await page.route(url,hold);const accepted=page.waitForResponse(response=>response.url()===url&&response.request().method()==='POST');
    try{await form.getByRole('button',{name:'Register asset',exact:true}).click();await dispatched;await expect(form.getByRole('button',{name:'Register asset',exact:true})).toBeDisabled();await expect(form.getByRole('textbox',{name:'Name',exact:true})).toBeDisabled();await expect(drawer.getByRole('button',{name:'Close dialog',exact:true})).toBeDisabled();await expect(form.getByRole('textbox',{name:'Currency',exact:true})).toHaveValue('THB');await expect(form.getByRole('textbox',{name:'Acquisition cost',exact:true})).toHaveValue('123456.78');await proof.observe(label+'-pending',['loading','disabled/read-only','currency','long-thai',proof.drawerSurface],active);}finally{release();await page.unroute(url,hold);}
    const success=await accepted;expect(success.status()).toBe(200);const receipt=(await success.json()).data,key=await success.request().headerValue('Idempotency-Key');expect(key).toBeTruthy();expect(receipt).toEqual({id:`asset-${key}`,version:1});await expect(drawer).not.toBeVisible();const saved=await assetUiWorkspace(context,state.origin,active);expect(saved.items.find(item=>item.id===receipt.id)).toMatchObject({code,name:thai,category:tag,currency:'THB',cost_minor:12345678,version:1});populated.items.push(saved.items.find(item=>item.id===receipt.id)!);
    // A second real browser window changes the ORIGINAL server selection, then
    // actual focus triggers revalidation. No data attribute/event is invented.
    await page.goto(state.origin+assetRegisterPath);active=await assetUiWork(context,state.origin,proof.companyId);const alternative=active.options.find(option=>option.id!==active.assignmentId);expect(alternative).toBeTruthy();if(!alternative)throw Error('EXISTING_NATIVE_ALTERNATE_ASSIGNMENT_REQUIRED');const auxiliary=await context.newPage();await auxiliary.goto('about:blank');let changed=false;
    try{const response=await context.request.post(state.origin+'/api/v1/holdings/simulation/assignments/active',{headers:{Origin:state.origin,'x-erpx-context-company':active.companyId},data:{assignmentId:alternative.id,revision:active.revision}});try{expect(response.status()).toBe(200);expect((await response.json()).data.assignmentId).toBe(alternative.id);changed=true;}finally{await response.dispose();}await page.bringToFront();await expect(page.locator('[data-work-assignment]')).toHaveAttribute('data-work-stale','true');await expect(page.getByRole('status').filter({hasText:'Work role changed. Your open form has not been submitted.'})).toBeVisible();await expect(opener).not.toBeVisible();const current=await assetUiWork(context,state.origin,proof.companyId);await proof.observe(label+'-stale',['stale'],current,'changed:actual-native-selected-assignment');}finally{if(changed){const current=await assetUiWork(context,state.origin,proof.companyId),response=await context.request.post(state.origin+'/api/v1/holdings/simulation/assignments/active',{headers:{Origin:state.origin,'x-erpx-context-company':active.companyId},data:{assignmentId:active.assignmentId,revision:current.revision}});try{expect(response.status()).toBe(200);expect((await response.json()).data.assignmentId).toBe(active.assignmentId);}finally{await response.dispose();}}await auxiliary.close();}active=await assetUiWork(context,state.origin,proof.companyId);
   }
  }
  expect(commandPosts).toBe(40);await proof.stable();
 });
});

test.describe('asset register native denial observations',uatAuthProfiles('employee'),()=>{
 test.use({storageState:resolve(authDirectory,'employee.json'),deviceScaleFactor:1});
 test('the existing Employee role cannot read or mount the Asset owner at any required width or appearance',async({page,context},info)=>{
  test.setTimeout(240_000);const proof=await assetUiCapture(page,info,state.origin),work=await assetUiWork(context,state.origin,proof.companyId);expect(work.valid).toBe(true);expect(work.options.find(option=>option.id===work.assignmentId)?.role).toBe('employee');
  for(const [theme,scheme] of [['light','light'],['dark','dark'],['system','light'],['system','dark']] as const){await context.addCookies([{name:'erpx_appearance',value:theme,url:state.origin}]);await page.emulateMedia({colorScheme:scheme});for(const width of [320,390,768,1024,1440]){
   await page.setViewportSize({width,height:900});await page.goto(state.origin+assetRegisterPath);await expect(page).toHaveURL(state.origin+assetRegisterPath);await expect(page.locator('main [data-work-forbidden]')).toBeVisible();await expect(page.getByRole('button',{name:'Register asset',exact:true})).toHaveCount(0);await expect(page.locator('input[name="code"]')).toHaveCount(0);await expect(page.locator('table')).toHaveCount(0);
   const response=await context.request.get(state.origin+'/api/v1/holdings/simulation/assets',{headers:assetUiHeaders(work)});try{expect(response.status()).toBe(403);expect((await response.json()).error.code).toBe('MODULE_FORBIDDEN');}finally{await response.dispose();}await proof.observe(`asset-denied-${width}-${theme}-${scheme}`,['denied'],work,'denied:actual-native-employee-module-assets');
  }}await proof.stable();
 });
});


// The server owns the receipt. Only delivery of its real committed response is
// lost; no API response/status/receipt or business row is manufactured here.
test.describe('asset register original command recovery observations',uatAuthProfiles('owner'),()=>{
 test.use({storageState:resolve(authDirectory,'owner.json'),deviceScaleFactor:1});
 test('a committed response lost in transit retries its exact original request and preserves the newer native draft',async({page,context},info)=>{
  test.setTimeout(600_000);
  const proof=await assetUiCapture(page,info,state.origin),today=new Date().toISOString().slice(0,10),url=state.origin+'/api/v1/holdings/simulation/assets';
  type NativeReceipt={id:string;version:number};
  type OriginalRequest={key:string;body:string;origin:string;headers:Record<string,string|null>};
  type Delivery={ok:true;request:OriginalRequest;receipt:NativeReceipt}|{ok:false;failure:unknown};
  const evidence:{cell:string;requestBodySha256:string;key:string;receipt:NativeReceipt;work:{companyId:string;userId:string;assignmentId:string;revision:number};nativeRows:number}[]=[];
  for(const [theme,scheme] of [['light','light'],['dark','dark'],['system','light'],['system','dark']] as const){
   await context.addCookies([{name:'erpx_appearance',value:theme,url:state.origin}]);await page.emulateMedia({colorScheme:scheme});
   for(const width of [320,390,768,1024,1440]){
    const cell=`asset-recovery-${width}-${theme}-${scheme}`,code=`REC-${run}-${info.project.name}-${width}-${theme}-${scheme}`,originalName=`Original retained Asset ${width} ${theme} ${scheme}`,newerName=`Newer unsubmitted Asset ${width} ${theme} ${scheme}`;
    await page.setViewportSize({width,height:900});await page.goto(state.origin+assetRegisterPath);await expect(page).toHaveURL(state.origin+assetRegisterPath);
    await expect(page.locator('html')).toHaveAttribute('data-theme',theme);expect(await page.locator('html').evaluate(element=>getComputedStyle(element).colorScheme)).toBe(scheme);
    const work=await assetUiWork(context,state.origin,proof.companyId);expect(work.valid).toBe(true);expect(work.options.find(option=>option.id===work.assignmentId)?.role).toBe('holding_owner');
    const stamp=page.locator('[data-work-assignment]');await expect(stamp).toHaveCount(1);await expect(stamp).toHaveAttribute('data-work-stale','false');for(const [name,value]of Object.entries({company:work.companyId,user:work.userId,assignment:work.assignmentId,revision:String(work.revision)}))await expect(stamp).toHaveAttribute('data-work-'+name,value);
    const before=await assetUiWorkspace(context,state.origin,work);expect(before.canCreate).toBe(true);expect(before.items.filter(item=>item.code===code)).toHaveLength(0);expect(fixture.branchId).toEqual(expect.any(String));expect(before.nodes.filter(node=>node.id===fixture.branchId&&['branch','headquarters'].includes(node.kind))).toHaveLength(1);
    const content=page.locator('#workspace-content:visible');await expect(content.getByRole('heading',{name:'Assets & maintenance',level:1,exact:true})).toBeVisible();const opener=content.getByRole('button',{name:'Register asset',exact:true});await expect(opener).toBeEnabled();await opener.click();
    const drawer=page.getByRole('dialog',{name:'Register asset',exact:true}),form=drawer.locator('form.ui-form-grid'),name=form.getByRole('textbox',{name:'Name',exact:true}),submit=form.getByRole('button',{name:'Register asset',exact:true});await expect(drawer).toBeVisible();await form.getByRole('combobox',{name:'Branch',exact:true}).selectOption(fixture.branchId!);await assetUiFill(form,code,originalName,'Original command recovery',today);
    const originalEditVersion=await form.getAttribute('data-erpx-edit-version');expect(originalEditVersion).toBeTruthy();
    let finish!:(value:Delivery)=>void;const delivered=new Promise<Delivery>(resolve=>finish=resolve);
    const loseAfterCommit=async(route:import('@playwright/test').Route)=>{
     try{
      const request=route.request(),body=request.postData(),key=await request.headerValue('Idempotency-Key');expect(request.method()).toBe('POST');expect(request.url()).toBe(url);if(!body||!key)throw Error('ACTUAL_ORIGINAL_ASSET_REQUEST_REQUIRED');
      const origin=await request.headerValue('origin');expect(origin).toBe(state.origin);const headers:Record<string,string|null>={};for(const header of Object.keys(assetUiHeaders(work)))headers[header]=await request.headerValue(header);expect(headers).toEqual(assetUiHeaders(work));
      expect(JSON.parse(body)).toMatchObject({branchId:fixture.branchId,code,name:originalName,category:'Original command recovery',currency:'THB',cost:'123456.78',acquiredDate:today});
      // This reaches the unchanged native owner and waits for its true200.
      // maxRetries0 prevents the instrumentation from sending another command.
      const response=await route.fetch({maxRedirects:0,maxRetries:0}),receipt=await(async()=>{try{expect(response.status()).toBe(200);const payload=await response.json() as {data:NativeReceipt};expect(payload.data).toEqual({id:`asset-${key}`,version:1});return payload.data;}finally{await response.dispose();}})();
      await route.abort('failed');finish({ok:true,request:{key,body,origin:state.origin,headers},receipt});
     }catch(failure){try{await route.abort('failed');}catch{/* The original native/transport failure remains the test result. */}finish({ok:false,failure});}
    };
    await page.route(url,loseAfterCommit);
    const original=await(async()=>{try{await submit.click();const outcome=await delivered;if(!outcome.ok)throw outcome.failure;await expect(drawer.getByRole('status').filter({hasText:'The original outcome is unresolved. Keep this draft and confirm the original request.'})).toBeVisible();await expect(drawer.getByRole('button',{name:'Retry submitted change',exact:true})).toBeEnabled();await expect(drawer.getByRole('button',{name:'Close dialog',exact:true})).toBeDisabled();await expect(name).toBeEnabled();return outcome;}finally{await page.unroute(url,loseAfterCommit);}})();
    const committed=await assetUiWorkspace(context,state.origin,work);expect(committed.items.filter(item=>item.id===original.receipt.id)).toHaveLength(1);expect(committed.items.filter(item=>item.code===code)).toHaveLength(1);expect(committed.items.find(item=>item.id===original.receipt.id)).toMatchObject({code,name:originalName,version:1,currency:'THB',cost_minor:12345678});await expect(name).toHaveValue(originalName);await expect(form).toHaveAttribute('data-erpx-edit-version',originalEditVersion!);await expect(page).toHaveURL(state.origin+assetRegisterPath);
    await proof.observe(cell+'-uncertain',['loaded',proof.drawerSurface,proof.recoveryUncertainSurface],work);
    // A real newer edit must neither replace the pending command nor receive
    // its acknowledgement. The original first form edit version stays captured.
    await name.fill(newerName);const newerEditVersion=await form.getAttribute('data-erpx-edit-version');expect(newerEditVersion).toBeTruthy();expect(newerEditVersion).not.toBe(originalEditVersion);
    const attempted:OriginalRequest[]=[];const recordPost=(request:import('@playwright/test').Request)=>{if(request.method()==='POST'&&request.url()===url)attempted.push({key:request.headers()['idempotency-key']??'',body:request.postData()??'',origin:request.headers().origin??'',headers:{'x-erpx-context-company':request.headers()['x-erpx-context-company']??null,'x-erpx-context-assignment':request.headers()['x-erpx-context-assignment']??null,'x-erpx-context-revision':request.headers()['x-erpx-context-revision']??null}});};page.on('request',recordPost);
    try{
     await submit.click();await expect(drawer.getByRole('alert')).toHaveText('Retry the submitted Asset or Maintenance change first. Your newer draft is retained.');expect(attempted).toEqual([]);await expect(name).toHaveValue(newerName);await expect(drawer.getByRole('button',{name:'Retry submitted change',exact:true})).toBeEnabled();
     // Observe current native authority, never reconstruct or rebase Work. The
     // same original role/revision must still authorize the native replay.
     const current=await assetUiWork(context,state.origin,proof.companyId);expect({valid:current.valid,companyId:current.companyId,userId:current.userId,assignmentId:current.assignmentId,revision:current.revision}).toEqual({valid:true,companyId:work.companyId,userId:work.userId,assignmentId:work.assignmentId,revision:work.revision});expect(current.options.find(option=>option.id===current.assignmentId)?.role).toBe('holding_owner');expect((await assetUiWorkspace(context,state.origin,current)).canCreate).toBe(true);
     const acknowledged=page.waitForResponse(response=>response.url()===url&&response.request().method()==='POST');await drawer.getByRole('button',{name:'Retry submitted change',exact:true}).click();const response=await acknowledged;expect(response.status()).toBe(200);const replay=(await response.json()).data;expect(replay).toEqual(original.receipt);expect(await response.request().headerValue('Idempotency-Key')).toBe(original.request.key);expect(response.request().postData()).toBe(original.request.body);
     await expect(drawer.getByRole('status').filter({hasText:'Submitted change saved. Your newer draft is retained.'})).toBeVisible();await expect(drawer.getByRole('button',{name:'Retry submitted change',exact:true})).toHaveCount(0);await expect(drawer.getByRole('button',{name:'Close dialog',exact:true})).toBeEnabled();await expect(drawer).toBeVisible();await expect(name).toHaveValue(newerName);await expect(form).toHaveAttribute('data-erpx-edit-version',newerEditVersion!);await expect(page).toHaveURL(state.origin+assetRegisterPath);expect(attempted).toEqual([original.request]);
     const saved=drawer.getByRole('link',{name:original.receipt.id,exact:true});await expect(saved).toHaveCount(1);const href=await saved.getAttribute('href');expect(href).toBeTruthy();if(!href)throw Error('ORIGINAL_NATIVE_ASSET_HISTORY_LINK_REQUIRED');const target=new URL(href,state.origin);expect(target.origin).toBe(state.origin);expect(decodeURIComponent(target.pathname)).toContain(original.receipt.id);
     const after=await assetUiWorkspace(context,state.origin,current);expect(after.items.length).toBe(before.items.length+1);expect(after.items.filter(item=>item.id===original.receipt.id||item.code===code)).toHaveLength(1);expect(after.items.find(item=>item.id===original.receipt.id)).toMatchObject({code,name:originalName,version:1,currency:'THB',cost_minor:12345678});expect(after.items.some(item=>item.name===newerName)).toBe(false);await expect(stamp).toHaveAttribute('data-work-assignment',work.assignmentId);await expect(stamp).toHaveAttribute('data-work-revision',String(work.revision));await expect(stamp).toHaveAttribute('data-work-stale','false');await proof.observe(cell+'-notice',[proof.drawerSurface,proof.recoveryNoticeSurface],current);
     evidence.push({cell,requestBodySha256:createHash('sha256').update(original.request.body).digest('hex'),key:original.request.key,receipt:original.receipt,work:{companyId:work.companyId,userId:work.userId,assignmentId:work.assignmentId,revision:work.revision},nativeRows:1});
    }finally{page.off('request',recordPost);}
    // Only explicit user discard releases the newer, still-dirty native form.
    await assetUiDiscard(page);const afterDiscard=await assetUiWorkspace(context,state.origin,work);expect(afterDiscard.items.filter(item=>item.id===original.receipt.id||item.code===code)).toHaveLength(1);expect(afterDiscard.items.find(item=>item.id===original.receipt.id)?.name).toBe(originalName);
   }
  }
  expect(evidence).toHaveLength(20);await info.attach('asset-native-original-replay-proof',{body:Buffer.from(JSON.stringify({schema:'erpx-asset-native-original-replay/v1',run:proof.run,origin:state.origin,route:assetRegisterPath,committedNativeResponsesAborted:20,originalReplays:20,newerDraftPosts:0,requestBodiesRecorded:'sha256-only',normalNativeOwner:true,commandStatusEndpointUsed:false,events:evidence,acceptance:false})),contentType:'application/json'});await proof.stable();
 });
});


import {observeFinanceInvoiceDrawer} from '../../support/browser/finance-invoice-ui-observation';

test.describe('Finance invoice drawer observations',uatAuthProfiles('owner'),()=>{
 test.use({storageState:resolve(authDirectory,'owner.json'),deviceScaleFactor:1});
 test('native prepared invoice drawers retain dirty work and keyboard focus across the appearance matrix',async({page,context},info)=>{
  test.setTimeout(600_000);
  await observeFinanceInvoiceDrawer(page,context,info,state.origin);
 });
});
