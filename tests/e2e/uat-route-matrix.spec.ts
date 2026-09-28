import { expect, test, type Page, type TestInfo } from "@playwright/test";
import { createHash } from "node:crypto";
import { appendFileSync, mkdirSync, readdirSync, readFileSync } from "node:fs";
import { relative, resolve } from "node:path";

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
    return entry.isDirectory() ? collectPageFiles(path) : /\/page\.(?:tsx|ts|jsx|js)$/.test(path) ? [path] : [];
  }).sort();
}
const sourcePageFiles = collectPageFiles(resolve(root, "src/app"));
const currentSourceHash = createHash("sha256");
for (const path of sourcePageFiles) currentSourceHash.update(relative(root, path)).update("\0").update(readFileSync(path)).update("\0");
const run = process.env.ERPX_BROWSER_SIMULATION_RUN ?? "";
if (!/^[a-f0-9]{12}$/.test(run)) throw new Error("Set ERPX_BROWSER_SIMULATION_RUN to the exact UAT simulation run.");
const inventory = JSON.parse(readFileSync(resolve(root, "docs/testing/uat-route-inventory.json"), "utf8")) as Inventory;
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
    kind: "order",
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

function profileFor(route: RouteEntry): string {
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
  const errors = pageErrors(page);
  let navigationError = "";
  let status: number | undefined;
  try {
    page.setDefaultNavigationTimeout(30_000);
    const response = await page.goto(path, { waitUntil: "domcontentloaded" });
    status = response?.status();
    await page.locator("main, [role=main]").first().waitFor({ state: "visible", timeout: 20_000 });
    await page.locator("h1:visible, [role=heading][aria-level='1']:visible").first().waitFor({ state: "visible", timeout: 20_000 });
    await page.waitForFunction(() => {
      const heading = [...document.querySelectorAll("h1, [role=heading][aria-level='1']")]
        .find((element) => element.getClientRects().length > 0 && getComputedStyle(element).visibility !== "hidden");
      const text = heading?.textContent?.trim() ?? "";
      return Boolean(text) && !/^loading(?:…|\.\.\.)?$/i.test(text);
    }, null, { timeout: 20_000 });
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
    accessReason: document.querySelector("[data-access-reason]")?.getAttribute("data-access-reason") ?? null,
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
  if (route.template.startsWith("/front-access")) return { baseURL: app === "pms" ? pmsOrigin : posOrigin, path: buildPath(route) };
  if (route.template.startsWith("/front/")) return { baseURL: process.env.ERPX_E2E_BASE_URL ?? state.origin, path: buildPath(route, { app }) };
  const sample = examplePath(route);
  return { baseURL: process.env.ERPX_E2E_BASE_URL ?? state.origin, path: sample.path };
}

test.describe("UAT source inventory", () => {
  test("source fingerprint matches the full current route inventory", async () => {
    expect(inventory.pageRouteCount).toBe(sourcePageFiles.length);
    expect(inventory.pageRouteCount).toBe(inventory.entries.length);
    expect(inventory.pageRouteCount).toBeGreaterThan(0);
    expect(inventory.sourceFingerprint).toMatch(/^[a-f0-9]{64}$/);
    expect(inventory.sourceFingerprint).toBe(currentSourceHash.digest("hex"));
    expect(inventory.entries.find((entry) => entry.source === "src/app/page.tsx")?.template).toBe("/");
  });
});

const frontAccessEntries = inventory.entries.filter((entry) => entry.template.startsWith("/front-access"));
const regularEntries = inventory.entries.filter((entry) => !entry.template.startsWith("/front-access"));

test.describe("signed-out route access", () => {
  test.use({ storageState: { cookies: [], origins: [] } });
  for (const route of regularEntries) {
    const apps = route.template.startsWith("/front/") ? (["pms", "pos"] as const) : (["pms"] as const);
    for (const app of apps) {
      const target = hrefForPublicEntry(route, app);
      test(`${route.template} · anonymous · ${app}`, async ({ page }, info) => {
        const { status, metrics } = await capture(page, info, route, "anonymous", target.path, "representative");
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

test.describe("asset register form layout", () => {
  test.use({ storageState: resolve(authDirectory, "owner.json") });
  test("register title sits above the first field on desktop and mobile", async ({ page }, info) => {
    const route = inventory.entries.find((entry) => entry.template === "/holdings/[holdingSlug]/assets")!;
    await capture(page, info, route, "signed-in:owner:layout", buildPath(route), "layout-regression");
    const heading = page.getByRole("heading", { name: "Register asset", exact: true });
    const form = page.locator("form.business-form").filter({ has: heading }).first();
    const firstLabel = form.locator("label").filter({ hasText: "Branch" }).first();
    await expect(heading).toBeVisible();
    await expect(firstLabel).toBeVisible();
    const headingBox = await heading.boundingBox();
    const formBox = await form.boundingBox();
    const labelBox = await firstLabel.boundingBox();
    expect(headingBox && labelBox && headingBox.y + headingBox.height <= labelBox.y + 2,
      "Register asset title must be above the Branch field").toBe(true);
    expect(headingBox && formBox && headingBox.x - formBox.x >= 16,
      "Register asset title must respect the card's horizontal inset").toBe(true);
  });
});

for (const app of ["pms", "pos"] as const) {
  test.describe(`signed-out Front ${app} access`, () => {
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
const grouped = new Map<string, RouteEntry[]>();
for (const route of privateEntries) {
  if (route.template.startsWith("/front/")) continue;
  const profile = profileFor(route);
  grouped.set(profile, [...(grouped.get(profile) ?? []), route]);
}

const frontRouteEntries = privateEntries.filter((route) => route.template.startsWith("/front/"));
for (const app of ["pms", "pos"] as const) {
  test.describe(`signed-in Front ${app}`, () => {
    test.use({ storageState: resolve(authDirectory, `${app}.json`) });
    for (const route of frontRouteEntries) {
      test(`${route.template} · ${app} role`, async ({ page }, info) => {
        await capture(page, info, route, `signed-in:${app}`, buildPath(route, { app }), "front-workspace");
      });
    }
  });
}

for (const [profile, routes] of grouped) {
  test.describe(`signed-in as ${profile}`, () => {
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
          }
          if (route.area === "Platform") {
            expect(finalUrl.pathname === "/access" && ["platform", "workspace", "permission"].includes(metrics.accessReason ?? ""), "holding access must be explicitly denied platform administration").toBe(true);
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
  test.describe(`holding, branch, department, and combined scope routes · ${scope}`, () => {
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
    test.describe(`${route.template} · ${profile}`, () => {
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
  test.describe("branch employee", () => {
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
  test.describe("department employee", () => {
    test.use({ storageState: resolve(authDirectory, "department.json") });
    for (const route of inventory.entries.filter((entry) => ["/holdings/[holdingSlug]/home", "/holdings/[holdingSlug]/work", "/holdings/[holdingSlug]/leave"].includes(entry.template))) {
      test(`${route.template} opens in assigned department`, async ({ page }, info) => {
        await capture(page, info, route, "department-employee:allowed", buildPath(route, { scope: "department" }), "scoped-persona");
      });
    }
  });
  test.describe("employee denied from restricted modules", () => {
    test.use({ storageState: resolve(authDirectory, "employee.json") });
    for (const template of ["/holdings/[holdingSlug]/finance", "/holdings/[holdingSlug]/people/settings", "/holdings/[holdingSlug]/settings/stripe"]) {
      const route = inventory.entries.find((entry) => entry.template === template)!;
      test(`${template} returns permission state`, async ({ page }, info) => {
        const { metrics } = await capture(page, info, route, "employee:denied", buildPath(route), "permission-denial");
        if (template === "/holdings/[holdingSlug]/people/settings") {
          await expect(page.getByText("Your selected role does not manage these settings.", { exact: true }), `${template} must explain its read-only denial`).toBeVisible();
        } else {
          expect(metrics.accessReason === "permission" || page.url().includes("/access"), `${template} denied with an explicit state`).toBe(true);
        }
      });
    }
  });
});
