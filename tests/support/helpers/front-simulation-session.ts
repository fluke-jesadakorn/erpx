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
  const signIn = page.getByRole("button", { name: /^(Sign in|เข้าสู่ระบบ)$/i });
  await expect(signIn).toHaveAttribute("data-client-managed-ready", "true");
  await expect(signIn).toBeEnabled();
  await page.locator('input[type="email"]').fill(user!.email);
  await page.locator('input[type="password"]').fill(credentials.password);
  await Promise.all([
    page.waitForURL((url) => url.hostname === new URL(origin).hostname && url.pathname !== "/login", { timeout: 30_000, waitUntil: "domcontentloaded" }),
    signIn.click(),
  ]);
  await expect(page.locator(`[data-work-assignment][data-front-app="${app}"][data-front-location="holding"]`)).toBeVisible();

  const roleEndpoint = `/api/v1/front/riverside-lantern/${encodeURIComponent(companyId)}/${app}/roles`;
  let roleStateResponse: { status: number; body: { data: { revision: number } } } | null = null;
  for (let attempt = 0; attempt < 3 && !roleStateResponse; attempt += 1) {
    try {
      roleStateResponse = await page.evaluate(async (endpoint) => {
        const response = await fetch(endpoint, {
          cache: "no-store",
          headers: { "x-erpx-front-location": "holding" },
        });
        return { status: response.status, body: await response.json() as { data: { revision: number } } };
      }, roleEndpoint);
    } catch (error) {
      if (attempt === 2 || !String(error).includes("Execution context was destroyed")) throw error;
      await page.waitForLoadState("domcontentloaded").catch(() => undefined);
    }
  }
  expect(roleStateResponse, `${role} Front page navigation settles before role selection`).toBeTruthy();
  expect(roleStateResponse!.status, `${role} Front role options`).toBe(200);
  const roleState = roleStateResponse!.body.data;
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
  // The destination page is server-rendered and does not always issue another
  // roles fetch during navigation. The authenticated browser fetch and role
  // selection above validate the endpoint; these page attributes validate
  // that the selected assignment reached the rendered workspace.
  await expect(page.locator("[data-work-assignment]")).toHaveAttribute("data-work-assignment", assignment.id);
  await expect(page.locator("[data-work-stale]")).toHaveAttribute("data-work-stale", "false");
  const html = page.locator("html");
  const languageToggle = page.getByRole("button", { name: /^(EN|ไทย)$/ });
  await expect(languageToggle).toHaveAttribute("data-client-managed-ready", "true");
  await expect(languageToggle).toBeEnabled();
  const toggleLabel = await languageToggle.textContent();
  expect(toggleLabel === "EN" || toggleLabel === "ไทย").toBe(true);
  const initialLanguage = toggleLabel === "EN" ? "th" : "en";
  await expect(html).toHaveAttribute("lang", initialLanguage);
  const nextLanguage = initialLanguage === "th" ? "en" : "th";
  const changeLanguage = async (buttonName: "EN" | "ไทย", language: "en" | "th") => {
    const saved = page.waitForResponse((response) => {
      const request = response.request();
      if (request.method() !== "PATCH" || new URL(response.url()).pathname !== "/api/v1/account/preferences")
        return false;
      try {
        return JSON.parse(request.postData() ?? "{}").language === language;
      } catch {
        return false;
      }
    });
    await page.getByRole("button", { name: buttonName, exact: true }).click();
    await expect(html).toHaveAttribute("lang", language);
    const response = await saved;
    expect(response.status(), `The ${language} account preference is saved before the workflow starts`).toBe(200);
    expect((await response.json()).data?.preference?.language).toBe(language);
  };
  await changeLanguage(initialLanguage === "th" ? "EN" : "ไทย", nextLanguage);
  await changeLanguage(initialLanguage === "th" ? "ไทย" : "EN", initialLanguage);
  // The journeys use English accessible labels after proving both language
  // transitions; fictional Front accounts initially default to Thai.
  if(initialLanguage==='th') {
    await changeLanguage("EN", "en");
  }
}
