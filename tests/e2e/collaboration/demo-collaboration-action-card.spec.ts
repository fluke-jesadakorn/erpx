import { captureInvestorCheckpoint } from "../../support/helpers/investor-capture";
import {createEngineeringBrowserContext} from "../../support/browser/engineering-browser-evidence";
import { expect, test, type BrowserContext, type Page } from "@playwright/test";
import { readFile } from "node:fs/promises";
import { resolve } from "node:path";

type DemoUser = { email: string; role: string; roles?: string[]; label: string; userId: string };
type Credentials = { password: string; users: DemoUser[] };
type DemoGuide = { scenarios: Array<{ id: string; sourceRecord?: { id: string } }> };

async function signInRole(context: BrowserContext, page: Page, credentials: Credentials, role: string) {
  const user = credentials.users.find(item => item.roles?.includes(role) || item.role === role);
  expect(user, `${role} demo account`).toBeTruthy();
  const login = await context.request.post("/api/auth/sign-in/email", {
    headers: { Origin: (process.env.ERPX_E2E_BASE_URL ?? "http://127.0.0.1:3102") },
    data: { email: user!.email, password: credentials.password },
  });
  expect(login.ok(), await login.text()).toBe(true);
  const endpoint = "/api/v1/holdings/simulation/assignments/active";
  const active = (await (await context.request.get(endpoint)).json()).data;
  const assignment = active.options.find((item: { role: string }) => item.role === role);
  expect(assignment, `${role} active assignment`).toBeTruthy();
  const selected = await context.request.post(endpoint, {
    headers: { Origin: (process.env.ERPX_E2E_BASE_URL ?? "http://127.0.0.1:3102") },
    data: { assignmentId: assignment.id, revision: active.revision },
  });
  expect(selected.ok(), await selected.text()).toBe(true);
  await page.goto(`/holdings/simulation/collaboration?refresh=${Date.now()}`, { waitUntil: "domcontentloaded" });
  await expect(page.locator("[data-work-stale]")).toHaveAttribute("data-work-stale", "false");
  return user!;
}

async function openCardDisclosure(page: Page, title: string) {
  const details = page.locator("#workspace-content:visible details").filter({ has: page.locator("summary").getByText(title, { exact: true }) });
  await expect(details).toHaveAttribute("data-client-managed-ready", "true");
  await details.locator("summary").click();
}

test("isolated demo routes a record-linked action card to its owner", async ({ browser }, testInfo) => {
  const run = process.env.ERPX_DEMO_RUN;
  test.skip(!run, "Set ERPX_DEMO_RUN to an exact prepared browser-simulation run.");
  const directory = resolve(`.runtime/browser/${run}`);
  const credentials = JSON.parse(await readFile(resolve(directory, "credentials.json"), "utf8")) as Credentials;
  const guide = JSON.parse(await readFile(resolve(directory, "demo-guide.json"), "utf8")) as DemoGuide;
  const ownerContext = await createEngineeringBrowserContext(browser,testInfo);
  const employeeContext = await createEngineeringBrowserContext(browser,testInfo);
  try {
    const ownerPage = await ownerContext.newPage();
    const employeePage = await employeeContext.newPage();
    const employee = credentials.users.find(user => user.roles?.includes("employee"));
    expect(employee, "employee collaboration owner").toBeTruthy();
    const salesScenario = guide.scenarios.find(scenario => scenario.id === "sales-to-collection");
    expect(salesScenario?.sourceRecord?.id, "demo sales quote source").toBeTruthy();
    await signInRole(ownerContext, ownerPage, credentials, "holding_owner");
    const title = "Confirm customer installation window";
    await openCardDisclosure(ownerPage, "Create action card");
    const actionDrawer = ownerPage.getByRole("dialog", { name: "Create action card", exact: true });
    const actionForm = actionDrawer.locator("form");
    await expect(actionDrawer).toBeVisible();
    await expect(actionForm).toHaveCount(1);
    await actionForm.locator('select[name="sourceType"]').selectOption("commerce_order");
    await actionForm.locator('input[name="sourceId"]').fill(salesScenario!.sourceRecord!.id);
    await actionForm.locator('input[name="title"]').fill(title);
    await actionForm.locator('select[name="ownerId"]').selectOption(employee!.userId);
    await actionForm.getByLabel("Due", { exact: true }).fill("2026-10-01T12:00");
    await expect(actionForm.locator('input[type="hidden"][name="dueAt"]')).toHaveValue("2026-10-01T05:00:00.000Z");
    await actionForm.locator('textarea[name="reason"]').fill("Sales and Support need one accountable next action");
    const createResponse = ownerPage.waitForResponse(response => response.request().method() === "POST" && response.url().endsWith("/api/v1/holdings/simulation/collaboration"));
    await actionForm.getByRole("button", { name: "Create action card", exact: true }).click();
    const created=await createResponse;
    expect(created.status()).toBe(200);
    const createdCard=(await created.json()).data as {id:string;version:number;status:string};
    expect(createdCard).toMatchObject({version:1,status:"open"});
    await actionDrawer.getByRole("button", { name: "Close dialog", exact: true }).click();
    await expect(actionDrawer).not.toBeVisible();
    await openCardDisclosure(ownerPage, title);
    const ownerCardDrawer = ownerPage.getByRole("dialog", { name: title, exact: true });
    await expect(ownerCardDrawer.locator("form").getByRole("heading", { name: title, exact: true })).toBeVisible();
    const sourceLink = ownerCardDrawer.getByRole("link", { name: /QT-/ });
    await expect(sourceLink).toHaveAttribute("href", `/holdings/simulation/commerce/orders/${salesScenario!.sourceRecord!.id}`);
    await expect(ownerCardDrawer.locator("time")).toHaveAttribute("datetime", "2026-10-01T05:00:00.000Z");
    await captureInvestorCheckpoint(ownerPage, "action-created", ownerCardDrawer);
    const ownerAction = ownerCardDrawer.getByRole("combobox", { name: "Action", exact: true });
    await ownerAction.selectOption("reassign");
    await expect(ownerCardDrawer.getByLabel("New due", { exact: true })).toBeVisible();
    await expect(ownerCardDrawer.getByLabel("New due", { exact: true })).toHaveAttribute("required", "");
    await expect(ownerCardDrawer.getByLabel("Completion evidence", { exact: true })).toHaveCount(0);
    await ownerAction.selectOption("complete");
    await expect(ownerCardDrawer.getByLabel("New due", { exact: true })).toHaveCount(0);
    await ownerCardDrawer.getByRole("button", { name: "Close dialog", exact: true }).click();
    await expect(ownerCardDrawer).not.toBeVisible();

    await signInRole(employeeContext, employeePage, credentials, "custom_demo_sales_collaboration_owner");
    await openCardDisclosure(employeePage, title);
    const cardDrawer = employeePage.getByRole("dialog", { name: title, exact: true });
    const cardForm = cardDrawer.locator("form");
    await expect(cardDrawer).toBeVisible();
    await expect(cardForm).toHaveCount(1);
    await expect(cardForm.getByRole("heading", { name: title, exact: true })).toBeVisible();
    await expect(cardForm.getByText(/open/).first()).toBeVisible();
    await expect(cardForm.getByLabel("New due", { exact: true })).toHaveCount(0);
    await expect(cardForm.locator('select[name="ownerId"]')).toHaveCount(0);
    await captureInvestorCheckpoint(employeePage, "action-owner", cardDrawer);
    await cardForm.locator('select[name="action"]').selectOption("complete");
    await cardForm.locator('textarea[name="evidence"]').fill("Customer confirmed Tuesday at 10:00 by recorded phone contact.");
    await cardForm.locator('textarea[name="reason"]').fill("Action completed with source evidence");
    const completeResponse = employeePage.waitForResponse(response => response.request().method() === "POST" && response.url().endsWith("/api/v1/holdings/simulation/collaboration"));
    await cardForm.getByRole("button", { name: "Record action", exact: true }).click();
    const completed=await completeResponse;
    expect(completed.status()).toBe(200);
    expect((await completed.json()).data).toMatchObject({id:createdCard.id,version:2,status:"completed"});
    await expect(cardForm.getByText(/completed/).first()).toBeVisible();
    await employeePage.reload();
    await openCardDisclosure(employeePage, title);
    await expect(cardForm.getByRole("heading",{name:title,exact:true})).toBeVisible();
    const retained=await employeeContext.request.get("/api/v1/holdings/simulation/collaboration");
    expect(retained.status()).toBe(200);
    expect((await retained.json()).data.cards.find((card:{id:string})=>card.id===createdCard.id)).toMatchObject({version:2,status:"completed",source_type:"commerce_order",source_id:salesScenario!.sourceRecord!.id,owner_id:employee!.userId});
    await expect(cardForm.getByText(/completed/).first()).toBeVisible();
    await expect(cardForm.getByRole("region", { name: "Completion evidence", exact: true })).toContainText("Customer confirmed Tuesday at 10:00 by recorded phone contact.");
    await captureInvestorCheckpoint(employeePage, "action-completed", cardDrawer);
  } finally {
    await ownerContext.close();
    await employeeContext.close();
  }
});
