import { expect, test } from "@playwright/test";
import { Client, StreamableHTTPClientTransport } from "@modelcontextprotocol/client";
import { ProxyAgent } from "undici";
import { lstatSync, readFileSync } from "node:fs";
import { resolve } from "node:path";
import { browserProjects } from "../../../scripts/testing/browser-results";
import { executiveWorkHostCaseTitle, executiveWorkOrigin, readExecutiveWorkHostLease } from "../../../scripts/testing/executive-work-host-cohort";
import { createWorkflowUiObserver } from "../../support/browser/workflow-ui-observation";

const accountOrigin = "https://account-dev-erpx.jesadakorn.com";
const connectionName = "Executive read-only browser proof";

type Credentials = { profile: string; run: string; companyId: string; assignmentId: string; assignmentVersion: number; email: string; password: string; token: string };

function credentialsFor(run: string): Credentials {
  const path = resolve(process.cwd(), ".runtime/browser", run, "executive-work-credentials.json");
  const stat = lstatSync(path);
  if (!stat.isFile() || stat.isSymbolicLink() || (stat.mode & 0o077) !== 0) throw new Error("EXECUTIVE_WORK_FIXTURE_CREDENTIAL_FILE_REQUIRED");
  const value = JSON.parse(readFileSync(path, "utf8")) as Credentials;
  if (value.profile !== "executive-work" || value.run !== run || !value.companyId || !value.assignmentId || value.assignmentVersion !== 1 ||
      !value.email.endsWith("@example.test") || !value.password || !/^erpx_exec_[A-Za-z0-9_-]{43}$/.test(value.token)) {
    throw new Error("EXECUTIVE_WORK_FIXTURE_CREDENTIALS_REQUIRED");
  }
  return value;
}

test(executiveWorkHostCaseTitle, browserProjects(["desktop"], "The isolated browser proof captures the official UI at a 1x desktop viewport; this case makes no mobile evidence claim."), async ({ page }, testInfo) => {
  const root = resolve(process.cwd());
  const run = process.env.ERPX_BROWSER_SIMULATION_RUN ?? process.env.ERPX_DEMO_RUN;
  const leasePath = process.env.ERPX_EXECUTIVE_WORK_HOST_LEASE;
  if (!run || !leasePath || process.env.ERPX_PRODUCT_HOST_LEASE) throw new Error("EXACT_EXECUTIVE_WORK_BROWSER_COHORT_REQUIRED");
  const fixture = credentialsFor(run);
  const lease = await readExecutiveWorkHostLease(root, leasePath);
  if (lease.run !== run || lease.commercialMfa || lease.commercialApply || lease.packageEnforcement || lease.installedWrites || lease.businessWrites) {
    throw new Error("EXECUTIVE_WORK_BROWSER_LEASE_SCOPE_REQUIRED");
  }

  await page.setViewportSize({ width: 1440, height: 900 });
  await page.emulateMedia({ colorScheme: "light" });
  const launch = new URL("/launch", executiveWorkOrigin);
  launch.searchParams.set("companyId", fixture.companyId);
  launch.searchParams.set("next", "/holdings/acme/insights/executive");
  const start = new URL("/sso/start", executiveWorkOrigin);
  start.searchParams.set("next", `${launch.pathname}${launch.search}`);
  await page.goto(start.href);
  await expect.poll(() => new URL(page.url()).origin).toBe(accountOrigin);
  await expect(page.getByRole("heading", { name: "Welcome back." })).toBeVisible();
  await page.getByLabel("Work email").fill(fixture.email);
  await page.getByLabel("Password").fill(fixture.password);
  await page.getByRole("button", { name: "Sign in", exact: true }).click();

  await expect.poll(() => new URL(page.url()).origin).toBe(executiveWorkOrigin);
  await expect(page.getByRole("heading", { name: /Choose a work role/ })).toBeVisible();
  await page.getByRole("button", { name: "Use this role", exact: true }).click();
  await expect(page).toHaveURL(`${executiveWorkOrigin}/holdings/acme/insights/executive`);
  const workContext = page.locator("[data-work-company]");
  await expect(workContext).toHaveAttribute("data-work-company", fixture.companyId);
  await expect(workContext).toHaveAttribute("data-work-assignment", fixture.assignmentId);
  await expect(page.getByRole("heading", { name: /Your business\./ })).toBeVisible();

  await page.getByRole("button", { name: "External connections" }).click();
  await expect(page.getByRole("heading", { name: "External connections" })).toBeVisible();
  const connection = page.locator("article").filter({ hasText: connectionName });
  await expect(connection).toHaveCount(1);
  await expect(connection).toContainText("active");

  const observer = await createWorkflowUiObserver(page, testInfo, {
    routeSource: "packages/feature-work/src/routes/holdings/[holdingSlug]/insights/executive/page.tsx",
    targetSource: "packages/workplace/src/routes/holdings/[holdingSlug]/_presenters/executive/screen.tsx",
    checks: [
      "Account authentication completed through its normal email and password form.",
      "The Work role chooser selected the exact seeded owner assignment and Company.",
      "The Work Executive page opened in the seeded Company context.",
      "The External connections drawer showed the exact seeded connection with active status.",
      "The official MCP client discovered read-only tools and read only the scoped headcount metric.",
      "The isolated edge refused an Executive connection write before application dispatch.",
    ],
    ownerContext: { module: "work", page: "executive", holdingSlug: "acme", companyId: fixture.companyId, assignmentId: fixture.assignmentId, role: "holding_owner" },
    authorityBranch: "Account email sign-in → Work SSO → selected Company owner assignment → Executive connection scope",
    authentication: "normal-native-owner",
  });

  const denial = await page.evaluate(async () => {
    const response = await fetch("/api/v1/holdings/acme/executive/connections", {
      method: "POST", headers: { "Content-Type": "application/json", "x-erpx-executive-denial-probe": "deny-only" }, body: "{}",
    });
    return { status: response.status, boundary: response.headers.get("x-erpx-test-boundary") };
  });
  expect(denial).toEqual({ status: 403, boundary: "executive-business-write-denied" });

  const proxy = new ProxyAgent({ uri: "http://127.0.0.1:3102", requestTls: { ca: readFileSync(lease.ca.path) } });
  const client = new Client({ name: "executive-work-browser-proof", version: "1.0.0" });
  try {
    const proxiedFetch: typeof fetch = Object.assign((input: RequestInfo | URL, init?: RequestInit) => {
      const headers = new Headers(init?.headers);
      headers.set("Origin", executiveWorkOrigin);
      return fetch(input, { ...init, headers, redirect: "error", dispatcher: proxy } as RequestInit & { dispatcher: ProxyAgent });
    }, { preconnect: () => {} });
    await client.connect(new StreamableHTTPClientTransport(new URL("/api/mcp/executive", executiveWorkOrigin), {
      authProvider: { token: async () => fixture.token }, fetch: proxiedFetch,
    }));
    const listedTools = await client.listTools();
    expect(listedTools.tools.map(tool => tool.name).sort()).toEqual(["get_dashboard", "get_dashboard_data", "list_dashboards", "list_metrics", "query_metric"]);
    expect(listedTools.tools.every(tool => tool.annotations?.readOnlyHint === true)).toBe(true);
    const catalog = await client.callTool({ name: "list_metrics", arguments: {} });
    expect(catalog.isError).toBeFalsy();
    expect(JSON.stringify(catalog.structuredContent ?? catalog.content)).toContain("headcount");
    const metric = await client.callTool({ name: "query_metric", arguments: {
      metric: "headcount", filters: { from: "2026-01-01", to: "2026-12-31", currency: "THB" }, breakdown: "default",
    } });
    expect(metric.isError).toBeFalsy();
    expect(JSON.stringify(metric.structuredContent ?? metric.content)).toContain("headcount");
    expect(JSON.stringify(metric.structuredContent ?? metric.content)).not.toContain(fixture.token);
  } finally {
    await client.close().catch(() => {});
    await proxy.close();
  }
  await observer.observe("executive-work-mcp");
  await observer.stable();
});
