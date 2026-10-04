import {createEngineeringBrowserContext} from './helpers/engineering-browser-evidence';
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
    const actionForm = ownerPage.locator("#workspace-content form").filter({ hasText: "Create action card" });
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
    await expect(ownerPage.getByRole("heading", { name: title, exact: true })).toBeVisible();

    await signInRole(employeeContext, employeePage, credentials, "custom_demo_sales_collaboration_owner");
    await expect(employeePage.getByRole("heading", { name: title, exact: true })).toBeVisible();
    const cardForm = employeePage.locator("#workspace-content form").filter({ hasText: title });
    await expect(cardForm.getByText(/open/).first()).toBeVisible();
    await cardForm.locator('select[name="action"]').selectOption("complete");
    await cardForm.locator('textarea[name="evidence"]').fill("Customer confirmed Tuesday at 10:00 by recorded phone contact.");
    await cardForm.locator('textarea[name="reason"]').fill("Action completed with source evidence");
    const completeResponse = employeePage.waitForResponse(response => response.request().method() === "POST" && response.url().endsWith("/api/v1/holdings/simulation/collaboration"));
    await cardForm.getByRole("button", { name: "Record action", exact: true }).click();
    const completed=await completeResponse;
    expect(completed.status()).toBe(200);
    expect((await completed.json()).data).toMatchObject({id:createdCard.id,version:2,status:"completed"});
    await expect(employeePage.locator("#workspace-content").getByText(/completed/).first()).toBeVisible();
    await employeePage.reload();
    await expect(employeePage.getByRole("heading",{name:title,exact:true})).toBeVisible();
    const retained=await employeeContext.request.get("/api/v1/holdings/simulation/collaboration");
    expect(retained.status()).toBe(200);
    expect((await retained.json()).data.cards.find((card:{id:string})=>card.id===createdCard.id)).toMatchObject({version:2,status:"completed",source_type:"commerce_order",source_id:salesScenario!.sourceRecord!.id,owner_id:employee!.userId});
  } finally {
    await ownerContext.close();
    await employeeContext.close();
  }
});
