import {createEngineeringBrowserContext} from './helpers/engineering-browser-evidence';
import { expect, test, type BrowserContext, type Page } from "@playwright/test";
import { readFile } from "node:fs/promises";
import { resolve } from "node:path";

type DemoUser = {
  email: string;
  role: string;
  roles?: string[];
  label: string;
  userId: string;
};
type Credentials = { password: string; users: DemoUser[] };
type DemoGuide = { scenarios: Array<{ id: string; sourceRecord?: { id: string } }> };

async function selectRole(
  context: BrowserContext,
  credentials: Credentials,
  role: string,
) {
  const user = credentials.users.find(
    (item) => item.roles?.includes(role) || item.role === role,
  );
  expect(user, `${role} demo account`).toBeTruthy();
  const login = await context.request.post("/api/auth/sign-in/email", {
    headers: { Origin: (process.env.ERPX_E2E_BASE_URL ?? "http://127.0.0.1:3102") },
    data: { email: user!.email, password: credentials.password },
  });
  expect(login.ok(), await login.text()).toBe(true);
  const endpoint = "/api/v1/holdings/simulation/assignments/active";
  const active = (await (await context.request.get(endpoint)).json()).data;
  const assignment = active.options.find(
    (item: { role: string }) => item.role === role,
  );
  expect(assignment, `${role} active assignment`).toBeTruthy();
  const selected = await context.request.post(endpoint, {
    headers: { Origin: (process.env.ERPX_E2E_BASE_URL ?? "http://127.0.0.1:3102") },
    data: { assignmentId: assignment.id, revision: active.revision },
  });
  expect(selected.ok(), await selected.text()).toBe(true);
  return user!;
}

async function signInRole(
  context: BrowserContext,
  page: Page,
  credentials: Credentials,
  role: string,
  path: string,
) {
  const user = await selectRole(context, credentials, role);
  await page.goto(`${path}${path.includes("?") ? "&" : "?"}refresh=${Date.now()}`, {
    waitUntil: "commit",
  });
  await expect(page.locator("[data-work-stale]")).toHaveAttribute(
    "data-work-stale",
    "false",
  );
  return user!;
}

async function submitAction(page: Page, heading: string, button: string, reason: string) {
  const form = page.locator("#workspace-content:visible form").filter({ has: page.getByRole("heading", { name: heading, exact: true }) });
  await expect(form).toBeVisible();
  await form.getByLabel("Evidence and reason", { exact: true }).fill(reason);
  const response = page.waitForResponse(
    (item) =>
      item.request().method() === "POST" &&
      item.url().endsWith("/api/v1/holdings/simulation/support"),
  );
  await form.getByRole("button", { name: button, exact: true }).click();
  const result = await response;
  expect(result.status(), await result.text()).toBe(200);
  await expect(page.locator("#workspace-content:visible").getByText(`Version ${(await result.json()).data.version} ·`, { exact: false })).toBeVisible();
}

test("isolated demo completes a source-linked support case with independent confirmation", async ({
  browser,
}, testInfo) => {
  const run = process.env.ERPX_DEMO_RUN;
  test.skip(!run, "Set ERPX_DEMO_RUN to an exact prepared browser-simulation run.");
  test.setTimeout(120_000);
  const directory = resolve(`.runtime/browser/${run}`);
  const credentials = JSON.parse(
    await readFile(resolve(directory, "credentials.json"), "utf8"),
  ) as Credentials;
  const guide = JSON.parse(
    await readFile(resolve(directory, "demo-guide.json"), "utf8"),
  ) as DemoGuide;
  const quoteId = guide.scenarios.find((scenario) => scenario.id === "sales-to-collection")?.sourceRecord?.id;
  expect(quoteId, "demo customer quotation source").toBeTruthy();
  const support = credentials.users.find((user) => user.roles?.includes("support_agent"));
  const manager = credentials.users.find((user) => user.roles?.includes("support_manager"));
  expect(support, "support agent").toBeTruthy();
  expect(manager, "support manager").toBeTruthy();
  const ownerContext = await createEngineeringBrowserContext(browser,testInfo);
  const supportContext = await createEngineeringBrowserContext(browser,testInfo);
  const salesContext = await createEngineeringBrowserContext(browser,testInfo);
  const managerContext = await createEngineeringBrowserContext(browser,testInfo);
  for (const context of [ownerContext, supportContext, salesContext, managerContext]) context.setDefaultTimeout(15_000);
  try {
    const ownerPage = await ownerContext.newPage();
    await signInRole(ownerContext, ownerPage, credentials, "holding_owner", `/holdings/simulation/support?orderId=${quoteId}`);
    const title = `Customer equipment replacement request ${Date.now()}`;
    const createDrawer = ownerPage.getByRole("dialog", { name: "New service case", exact: true });
    const create = createDrawer.locator("form").filter({
      has: ownerPage.getByRole("heading", { name: "Record customer issue from order", exact: true }),
    });
    await create.locator('input[name="title"]').fill(title);
    await create.locator('textarea[name="issue"]').fill("Customer reports a faulty unit and needs a replacement delivery date.");
    await create.locator('select[name="owner"]').selectOption(support!.userId);
    await create.locator('input[name="due"]').fill("2026-10-06");
    const createResponse = ownerPage.waitForResponse(
      (item) => item.request().method() === "POST" && item.url().endsWith("/api/v1/holdings/simulation/support"),
    );
    await create.getByRole("button", { name: "Create linked support case", exact: true }).click();
    const createdResponse = await createResponse;
    expect(createdResponse.status()).toBe(200);
    const ticketId = (await createdResponse.json()).data.id as string;
    expect(ticketId).toBeTruthy();
    await expect(createDrawer).not.toBeVisible();
    const fullPage = ownerPage.getByRole("link", { name: "Open full page ↗", exact: true });
    await expect(fullPage).toHaveAttribute("href", `/holdings/simulation/support/${ticketId}`);
    await fullPage.click();
    await expect(ownerPage).toHaveURL(new RegExp(`/holdings/simulation/support/${ticketId}$`));
    expect(new URL(ownerPage.url()).pathname.split("/").at(-1)).toBe(ticketId);
    await expect(ownerPage.locator("#workspace-content:visible").getByRole("heading", { name: title, exact: true })).toBeVisible();

    const supportPage = await supportContext.newPage();
    const ticketPath = `/holdings/simulation/support/${ticketId}`;
    await signInRole(supportContext, supportPage, credentials, "support_agent", ticketPath);
    await submitAction(supportPage, "Accept case", "Accept case", "Support has accepted ownership and will inspect the reported equipment.");
    await submitAction(supportPage, "Request customer information", "Request customer information", "Waiting for the equipment serial number and error code.");
    await supportPage.reload();
    await expect(supportPage.locator("#workspace-content:visible").getByText("Waiting: Waiting for the equipment serial number and error code.", { exact: true })).toBeVisible();
    await submitAction(supportPage, "Resume after customer reply", "Resume after customer reply", "Customer supplied the serial number and error code by phone.");
    await submitAction(supportPage, "Record proposed resolution", "Record proposed resolution", "Reserve a replacement unit and confirm delivery with the customer.");
    await expect(supportPage.locator("#workspace-content:visible").getByText(/^resolved ·/)).toBeVisible();

    const managerPage = await managerContext.newPage();
    await signInRole(managerContext, managerPage, credentials, "support_manager", ticketPath);
    await submitAction(managerPage, "Return for further work", "Return for further work", "Provide the replacement date before asking for customer confirmation.");
    await supportPage.reload();
    await submitAction(supportPage, "Record proposed resolution", "Record proposed resolution", "Customer will receive the replacement unit on October 6; delivery team confirmed the slot.");
    const resolutionObservedAt = Date.now();

    // The same support colleague records the customer contact evidence through
    // their explicitly selected Sales operator assignment.
    await selectRole(salesContext, credentials, "sales_operator");
    const salesPage = await salesContext.newPage();
    await salesPage.goto(`/holdings/simulation/commerce/orders/${quoteId}/notes`, { waitUntil: "domcontentloaded" });
    const noteBody = `Customer confirmed the replacement delivery window for ${title}.`;
    await expect(salesPage.locator("#workspace-content:visible").getByRole("heading", { name: "Customer contact notes", exact: true })).toBeVisible();
    const noteForm = salesPage.locator("#workspace-content:visible form").filter({ has: salesPage.getByRole("button", { name: "Save internal note", exact: true }) });
    await expect.poll(() => Math.floor(Date.now() / 1000) * 1000).toBeGreaterThan(resolutionObservedAt);
    const contactTime = await salesPage.evaluate(() => {
      const date = new Date();
      const pad = (value: number) => String(value).padStart(2, "0");
      return `${date.getFullYear()}-${pad(date.getMonth() + 1)}-${pad(date.getDate())}T${pad(date.getHours())}:${pad(date.getMinutes())}:${pad(date.getSeconds())}`;
    });
    await noteForm.getByLabel("Contact time", { exact: true }).fill(contactTime);
    await noteForm.locator('textarea[name="body"]').fill(noteBody);
    const noteResponse = salesPage.waitForResponse(
      (item) => item.request().method() === "POST" && item.url().endsWith("/api/v1/holdings/simulation/customer-notes"),
    );
    await noteForm.getByRole("button", { name: "Save internal note", exact: true }).click();
    const savedNote = await noteResponse;
    expect(savedNote.status(), await savedNote.text()).toBe(200);
    const noteId = (await savedNote.json()).data.id as string;
    await expect(salesPage.locator("#workspace-content:visible section.ui-panel:visible > p").filter({ hasText: noteBody })).toBeVisible();
    await salesPage.reload();
    await expect(salesPage.locator("#workspace-content:visible section.ui-panel:visible > p").filter({ hasText: noteBody })).toBeVisible();

    await managerPage.reload();
    const confirmation = managerPage.locator("#workspace-content:visible form").filter({ hasText: "Record customer confirmation" });
    await expect(confirmation).toBeVisible();
    await confirmation.locator('select[name="note"]').selectOption(noteId);
    await confirmation.getByLabel("Evidence and reason", { exact: true }).fill("Independently reviewed the customer confirmation and the proposed resolution.");
    const confirmResponse = managerPage.waitForResponse(
      (item) => item.request().method() === "POST" && item.url().endsWith("/api/v1/holdings/simulation/support"),
    );
    await confirmation.getByRole("button", { name: "Confirm customer outcome and close", exact: true }).click();
    const confirmed = await confirmResponse;
    expect(confirmed.status(), await confirmed.text()).toBe(200);
    await expect(managerPage.locator("#workspace-content:visible").getByText(/Version 8 · confirm/)).toBeVisible();
    await managerPage.reload();
    await expect(managerPage.locator("#workspace-content:visible").getByText(/^closed ·/)).toBeVisible();
    await supportPage.reload();
    await expect(supportPage.locator("#workspace-content:visible").getByText(/^closed ·/)).toBeVisible();
    await submitAction(managerPage, "Reopen customer dispute", "Reopen customer dispute", "Customer asked to review the replacement date again.");
    await submitAction(managerPage, "Cancel case with reason", "Cancel case with reason", "Customer withdrew the replacement request after further contact.");
    await managerPage.reload();
    await expect(managerPage.locator("#workspace-content:visible").getByText(/^cancelled ·/)).toBeVisible();
    await expect(managerPage.locator("#workspace-content:visible").getByText(/Version 10 · cancel/)).toBeVisible();
    await testInfo.attach("support-lifecycle-result", {
      contentType: "application/json",
      body: JSON.stringify({ run, ticketId, quoteId, noteId, agentId: support!.userId, managerId: manager!.userId, confirmedVersion: 8, finalVersion: 10, finalStatus: "cancelled" }, null, 2),
    });
    await managerPage.screenshot({ path: testInfo.outputPath("support-lifecycle.png"), fullPage: true });
  } finally {
    await Promise.allSettled([ownerContext.close(), supportContext.close(), salesContext.close(), managerContext.close()]);
  }
});
