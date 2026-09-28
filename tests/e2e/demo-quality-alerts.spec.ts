import { expect, test, type BrowserContext, type Page } from "@playwright/test";
import { readFile } from "node:fs/promises";
import { resolve } from "node:path";

type DemoUser = { email: string; role: string; roles?: string[] };
type Credentials = { password: string; users: DemoUser[] };
type DemoState = { fixture?: { qualityDueSoonHoldId?: string; qualityOverdueHoldId?: string } };

async function selectRole(context: BrowserContext, credentials: Credentials, role: string) {
  const user = credentials.users.find((item) => item.roles?.includes(role) || item.role === role);
  expect(user, `${role} demo account`).toBeTruthy();
  const headers = { Origin: (process.env.ERPX_E2E_BASE_URL ?? "http://127.0.0.1:3102") };
  const login = await context.request.post("/api/auth/sign-in/email", { headers, data: { email: user!.email, password: credentials.password } });
  expect(login.ok(), await login.text()).toBe(true);
  const endpoint = "/api/v1/holdings/simulation/assignments/active";
  const active = (await (await context.request.get(endpoint)).json()).data;
  const assignment = active.options.find((item: { role: string }) => item.role === role);
  expect(assignment, `${role} active assignment`).toBeTruthy();
  const selected = await context.request.post(endpoint, { headers, data: { assignmentId: assignment.id, revision: active.revision } });
  expect(selected.ok(), await selected.text()).toBe(true);
}

async function openRole(page: Page, context: BrowserContext, credentials: Credentials, role: string, path: string) {
  await selectRole(context, credentials, role);
  await page.goto(path, { waitUntil: "commit" });
  await expect(page.locator("[data-work-stale]")).toHaveAttribute("data-work-stale", "false");
}

test("isolated demo routes due-soon and overdue Quality escalations with durable acknowledgement", async ({ browser }, testInfo) => {
  const run = process.env.ERPX_DEMO_RUN;
  test.skip(!run, "Set ERPX_DEMO_RUN to an exact prepared browser-simulation run.");
  test.setTimeout(120_000);
  const credentials = JSON.parse(await readFile(resolve(`.runtime/browser/${run}/credentials.json`), "utf8")) as Credentials;
  const state = JSON.parse(await readFile(resolve(`.runtime/browser/${run}/state.json`), "utf8")) as DemoState;
  const dueSoon = state.fixture?.qualityDueSoonHoldId;
  const overdue = state.fixture?.qualityOverdueHoldId;
  expect(dueSoon).toBeTruthy();
  expect(overdue).toBeTruthy();
  const context = await browser.newContext(testInfo.project.use);
  const page = await context.newPage();
  try {
    await openRole(page, context, credentials, "quality_inspector", `/holdings/simulation/work?type=quality_alert&refresh=${Date.now()}`);
    await expect(page.locator("#workspace-content")).toContainText("Quality hold due soon");
    await expect(page.locator("#workspace-content")).toContainText("Overdue quality hold");

    for (const [holdId, heading, alertId] of [[dueSoon, "Quality hold due soon", `quality-alert-due-soon-${dueSoon}`], [overdue, "Overdue quality hold", `quality-alert-overdue-${overdue}`] ] as const) {
      await page.goto(`/holdings/simulation/quality/${holdId}?refresh=${Date.now()}`, { waitUntil: "commit" });
      await expect(page.locator("#workspace-content")).toContainText("Quality escalations");
      await expect(page.locator("#workspace-content")).toContainText(heading);
      const escalation = page.locator("#workspace-content section").filter({ hasText: heading }).last();
      await escalation.getByLabel("Follow-up evidence").fill(`Independent Quality follow-up recorded for ${heading}.`);
      const responsePromise = page.waitForResponse((response) => response.request().method() === "POST" && new URL(response.url()).pathname.endsWith(`/quality/${alertId}/acknowledge`));
      await escalation.getByRole("button", { name: "Acknowledge escalation", exact: true }).click();
      const response = await responsePromise;
      expect(response.status(), await response.text()).toBe(200);
      await expect(escalation).toContainText("Acknowledged by");
      await page.reload();
      await expect(page.locator("#workspace-content")).toContainText("Acknowledged by");
    }
    await testInfo.attach("quality-alert-result", { contentType: "application/json", body: JSON.stringify({ run, dueSoon, overdue, acknowledged: true }, null, 2) });
    await page.screenshot({ path: testInfo.outputPath("quality-alerts.png"), fullPage: true });
  } finally {
    await context.close();
  }
});
