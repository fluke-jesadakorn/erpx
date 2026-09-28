import { expect, type BrowserContext, type Page } from "@playwright/test";

export type SimulatedFrontUser = {
  email: string;
  role: string;
  roles?: string[];
  userId: string;
};

export type SimulatedFrontCredentials = {
  password: string;
  users: SimulatedFrontUser[];
};

export function frontOrigin(app: "pms" | "pos") {
  const root = new URL(process.env.ERPX_E2E_BASE_URL ?? "http://127.0.0.1:3100");
  return `http://dev-${app}.localhost:${root.port}`;
}

export function frontWorkspacePath(companyId: string, section = "") {
  const suffix = section.replace(/^\/+/, "");
  return `/riverside-lantern/${encodeURIComponent(companyId)}/holding${suffix ? `/${suffix}` : "/"}`;
}

export async function signInFrontRole(
  context: BrowserContext,
  page: Page,
  credentials: SimulatedFrontCredentials,
  role: string,
  app: "pms" | "pos",
  companyId: string,
  destination: string,
) {
  const rootOrigin = process.env.ERPX_E2E_BASE_URL ?? "http://127.0.0.1:3100";
  const origin = frontOrigin(app);
  const user = credentials.users.find(
    (item) => item.email.endsWith("@riverside-lantern.example") && (item.role === role || item.roles?.includes(role)),
  );
  expect(user, `${role} Riverside account`).toBeTruthy();

  // Keep a holding session for read-only fixture checks, then use the real
  // Front sign-in page so the browser receives the app-specific session cookie.
  const rootLogin = await context.request.post(new URL("/api/auth/sign-in/email", rootOrigin).toString(), {
    headers: { Origin: rootOrigin },
    data: { email: user!.email, password: credentials.password },
  });
  expect(rootLogin.ok(), `${role} holding session`).toBe(true);
  const assignmentEndpoint = new URL("/api/v1/holdings/riverside-lantern/assignments/active", rootOrigin).toString();
  const activeResponse = await context.request.get(assignmentEndpoint);
  expect(activeResponse.ok(), await activeResponse.text()).toBe(true);
  const active = (await activeResponse.json()).data;
  const assignment = active.options.find((item: { role: string }) => item.role === role);
  expect(assignment, `${role} holding assignment`).toBeTruthy();
  const assignmentResponse = await context.request.post(assignmentEndpoint, {
    headers: { Origin: rootOrigin },
    data: { assignmentId: assignment.id, revision: active.revision },
  });
  expect(assignmentResponse.ok(), await assignmentResponse.text()).toBe(true);

  await page.goto(`${origin}/login?next=${encodeURIComponent(destination)}`, { waitUntil: "domcontentloaded" });
  await page.waitForLoadState("networkidle");
  await page.locator('input[type="email"]').fill(user!.email);
  await page.locator('input[type="password"]').fill(credentials.password);
  await Promise.all([
    page.waitForURL((url) => url.hostname === new URL(origin).hostname && url.pathname !== "/login", { timeout: 30_000 }),
    page.getByRole("button", { name: "เข้าสู่ระบบ", exact: true }).click(),
  ]);
  await page.waitForLoadState("networkidle");

  const roleEndpoint = `/api/v1/front/riverside-lantern/${encodeURIComponent(companyId)}/${app}/roles`;
  const roleStateResponse = await page.evaluate(async (endpoint) => {
    const response = await fetch(endpoint, {
      cache: "no-store",
      headers: { "x-erpx-front-location": "holding" },
    });
    return { status: response.status, body: await response.json() as { data: { revision: number } } };
  }, roleEndpoint);
  expect(roleStateResponse.status, `${role} Front role options`).toBe(200);
  const roleState = roleStateResponse.body.data;
  const selected = await page.evaluate(async ({ endpoint, assignmentId, revision }) => {
    const response = await fetch(endpoint, {
      method: "POST",
      cache: "no-store",
      headers: { "Content-Type": "application/json", "x-erpx-front-location": "holding" },
      body: JSON.stringify({ location: "holding", assignmentId, revision }),
    });
    return { status: response.status };
  }, { endpoint: roleEndpoint, assignmentId: assignment.id, revision: roleState.revision });
  expect(selected.status, `${role} Front role selection`).toBe(200);

  await page.goto(`${origin}${destination}`, { waitUntil: "domcontentloaded" });
  await page.waitForLoadState("networkidle");
  // The destination page is server-rendered and does not always issue another
  // roles fetch during navigation. The authenticated browser fetch and role
  // selection above validate the endpoint; these page attributes validate
  // that the selected assignment reached the rendered workspace.
  await expect(page.locator("[data-work-assignment]")).toHaveAttribute("data-work-assignment", assignment.id);
  await expect(page.locator("[data-work-stale]")).toHaveAttribute("data-work-stale", "false");
  await page.getByRole("button", { name: "EN", exact: true }).click();
  await expect(page.locator("html")).toHaveAttribute("lang", "en");
  await expect(page.getByRole("button", { name: "ไทย", exact: true })).toBeVisible();
}
