import { expect, test, type BrowserContext, type Page } from "@playwright/test";
import { readFile } from "node:fs/promises";
import { resolve } from "node:path";

type DemoUser = { email: string; role: string; roles?: string[] };
type Credentials = { password: string; users: DemoUser[] };
type DemoState = { fixture?: { customerId?: string } };

async function signInRole(context: BrowserContext, page: Page, credentials: Credentials, role: string, path: string) {
  const user = credentials.users.find((item) => item.roles?.includes(role) || item.role === role);
  expect(user, `${role} demo account`).toBeTruthy();
  const headers = { Origin: "http://127.0.0.1:3102" };
  const login = await context.request.post("/api/auth/sign-in/email", { headers, data: { email: user!.email, password: credentials.password } });
  expect(login.ok(), await login.text()).toBe(true);
  const endpoint = "/api/v1/holdings/simulation/assignments/active";
  const active = (await (await context.request.get(endpoint)).json()).data;
  const assignment = active.options.find((item: { role: string }) => item.role === role);
  expect(assignment, `${role} active assignment`).toBeTruthy();
  const selected = await context.request.post(endpoint, { headers, data: { assignmentId: assignment.id, revision: active.revision } });
  expect(selected.ok(), await selected.text()).toBe(true);
  await page.goto(path, { waitUntil: "commit" });
  await expect(page.locator("[data-work-stale]")).toHaveAttribute("data-work-stale", "false");
}

test("isolated demo exposes a scoped Customer 360 timeline to independent Sales roles", async ({ browser }, testInfo) => {
  const run = process.env.ERPX_DEMO_RUN;
  test.skip(!run, "Set ERPX_DEMO_RUN to an exact prepared browser-simulation run.");
  test.setTimeout(120_000);
  const directory = resolve(`.runtime/browser/${run}`);
  const credentials = JSON.parse(await readFile(resolve(directory, "credentials.json"), "utf8")) as Credentials;
  const state = JSON.parse(await readFile(resolve(directory, "state.json"), "utf8")) as DemoState;
  const customerId = state.fixture?.customerId;
  expect(customerId).toBeTruthy();
  const operatorContext = await browser.newContext(testInfo.project.use);
  const managerContext = await browser.newContext(testInfo.project.use);
  const operatorPage = await operatorContext.newPage();
  const managerPage = await managerContext.newPage();
  const path = `/holdings/simulation/customers/${customerId}`;
  try {
    await signInRole(operatorContext, operatorPage, credentials, "sales_operator", path);
    await expect(operatorPage.getByRole("heading", { name: "Simulation customer only", exact: true })).toBeVisible();
    await expect(operatorPage.getByRole("heading", { name: "Visible source activity", exact: true })).toBeVisible();
    await expect(operatorPage.getByText(/quote ·/i).first()).toBeVisible();
    await expect(operatorPage.getByText(/Sales quotations\/orders/).first()).toBeVisible();
    await expect(operatorPage.getByText(/payment beneficiary|contact details|ต้นทุน/i)).toHaveCount(0);
    await expect(operatorPage.getByRole("heading", { name: "Record contact", exact: true })).toBeVisible();
    await operatorPage.locator('input[name="occurred"]').fill(new Date(Date.now() - 60_000).toISOString().slice(0, 16));
    await operatorPage.getByRole("textbox", { name: "Contact summary", exact: true }).fill("Demo customer asked for the updated service terms by email.");
    await Promise.all([
      operatorPage.waitForResponse((response) => response.url().includes("/api/v1/holdings/simulation/customer-notes") && response.request().method() === "POST"),
      operatorPage.getByRole("button", { name: "Save internal note", exact: true }).click(),
    ]);
    await expect(operatorPage.getByRole("paragraph").filter({ hasText: "Demo customer asked for the updated service terms by email." }).first()).toBeVisible();
    const source = operatorPage.locator("section.card").filter({ hasText: "Visible source activity" }).getByRole("link").first();
    await expect(source).toHaveAttribute("href", /\/commerce\/orders\//);
    await source.click();
    await expect(operatorPage).toHaveURL(/\/commerce\/orders\//);
    await signInRole(managerContext, managerPage, credentials, "sales_manager", path);
    await expect(managerPage.getByRole("heading", { name: "Visible source activity", exact: true })).toBeVisible();
    await expect(managerPage.getByText(/quote ·/i).first()).toBeVisible();
    await expect(managerPage.getByRole("paragraph").filter({ hasText: "Demo customer asked for the updated service terms by email." }).first()).toBeVisible();
    await expect(managerPage.getByText(/payment beneficiary|contact details|ต้นทุน/i)).toHaveCount(0);
    await testInfo.attach("customer-timeline-result", { contentType: "application/json", body: JSON.stringify({ run, customerId, roles: ["sales_operator", "sales_manager"], sourceLinkScoped: true, standaloneNoteCreatedAndReloaded: true, restrictedFieldsHidden: true }, null, 2) });
    await managerPage.screenshot({ path: testInfo.outputPath("customer-timeline.png"), fullPage: true });
  } finally {
    await Promise.allSettled([operatorContext.close(), managerContext.close()]);
  }
});
