import {createEngineeringBrowserContext} from './helpers/engineering-browser-evidence';
import { expect, test, type BrowserContext, type Page } from "@playwright/test";
import { readFile } from "node:fs/promises";
import { resolve } from "node:path";

type DemoUser = { email: string; role: string; roles?: string[] };
type Credentials = { password: string; users: DemoUser[] };
type DemoState = { fixture?: { customerId?: string } };

async function signInRole(context: BrowserContext, page: Page, credentials: Credentials, role: string, path: string) {
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
  const operatorContext = await createEngineeringBrowserContext(browser,testInfo);
  const managerContext = await createEngineeringBrowserContext(browser,testInfo);
  const operatorPage = await operatorContext.newPage();
  const managerPage = await managerContext.newPage();
  const path = `/holdings/simulation/customers/${customerId}`;
  try {
    await signInRole(operatorContext, operatorPage, credentials, "sales_operator", path);
    await expect(operatorPage.getByRole("heading", { name: "Simulation customer only", exact: true })).toBeVisible();
    await expect(operatorPage.getByRole("heading", { name: "Visible source activity", exact: true })).toBeVisible();
    await expect(operatorPage.locator("#workspace-content").getByText(/quote ·/i).first()).toBeVisible();
    await expect(operatorPage.locator("#workspace-content").getByText(/Sales quotations\/orders/).first()).toBeVisible();
    await expect(operatorPage.locator("#workspace-content").getByText(/payment beneficiary|contact details|ต้นทุน/i)).toHaveCount(0);
    await expect(operatorPage.getByRole("heading", { name: "Record contact", exact: true })).toBeVisible();
    const contactTime = operatorPage.locator('#workspace-content').getByLabel("Contact time", { exact: true });
    await expect(contactTime).toBeEnabled();
    const instant = Math.floor(Date.now() / 60_000) * 60_000 - 120_000;
    const timeField = operatorPage.locator("#workspace-content .preferred-date-time-field").filter({ has: operatorPage.getByLabel("Contact time", { exact: true }) });
    const timeZone = (await timeField.locator("small").first().textContent())?.match(/^Timezone: ([^\s·]+)/)?.[1];
    expect(timeZone, "the contact editor displays its resolved timezone").toBeTruthy();
    const parts = Object.fromEntries(new Intl.DateTimeFormat("en-GB", { timeZone, year: "numeric", month: "2-digit", day: "2-digit", hour: "2-digit", minute: "2-digit", hourCycle: "h23" }).formatToParts(instant).map(part => [part.type, part.value]));
    await contactTime.fill(`${parts.year}-${parts.month}-${parts.day}T${parts.hour}:${parts.minute}`);
    await expect(timeField.locator('input[name="occurred"]')).toHaveValue(new Date(instant).toISOString());
    await operatorPage.getByRole("textbox", { name: "Contact summary", exact: true }).fill("Demo customer asked for the updated service terms by email.");
    const [saved] = await Promise.all([
      operatorPage.waitForResponse((response) => response.url().includes("/api/v1/holdings/simulation/customer-notes") && response.request().method() === "POST"),
      operatorPage.getByRole("button", { name: "Save internal note", exact: true }).click(),
    ]);
    expect(saved.ok(), await saved.text()).toBe(true);
    expect(saved.request().postDataJSON().occurredAt).toBe(instant);
    const noteId = (await saved.json()).data.id;
    await expect(operatorPage.getByRole("paragraph").filter({ hasText: "Demo customer asked for the updated service terms by email." }).first()).toBeVisible();
    const source = operatorPage.locator("#workspace-content section.card").filter({ hasText: "Visible source activity" }).locator('a[href*="/commerce/orders/"]').first();
    await expect(source).toHaveAttribute("href", /\/commerce\/orders\//);
    await source.click();
    await expect(operatorPage).toHaveURL(/\/commerce\/orders\//);
    await signInRole(managerContext, managerPage, credentials, "sales_manager", path);
    await expect(managerPage.getByRole("heading", { name: "Visible source activity", exact: true })).toBeVisible();
    await expect(managerPage.locator("#workspace-content").getByText(/quote ·/i).first()).toBeVisible();
    await expect(managerPage.getByRole("paragraph").filter({ hasText: "Demo customer asked for the updated service terms by email." }).first()).toBeVisible();
    const notes = await managerContext.request.get(`/api/v1/holdings/simulation/customer-notes?customerId=${customerId}`);
    expect(notes.ok(), await notes.text()).toBe(true);
    expect((await notes.json()).data.notes).toContainEqual(expect.objectContaining({ id: noteId, occurred_at: instant, body: "Demo customer asked for the updated service terms by email." }));
    await expect(managerPage.locator("#workspace-content").getByText(/payment beneficiary|contact details|ต้นทุน/i)).toHaveCount(0);
    await testInfo.attach("customer-timeline-result", { contentType: "application/json", body: JSON.stringify({ run, customerId, roles: ["sales_operator", "sales_manager"], sourceLinkScoped: true, standaloneNoteCreatedAndReloaded: true, restrictedFieldsHidden: true }, null, 2) });
    await managerPage.screenshot({ path: testInfo.outputPath("customer-timeline.png"), fullPage: true });
  } finally {
    await Promise.allSettled([operatorContext.close(), managerContext.close()]);
  }
});
