import {createEngineeringBrowserContext} from './helpers/engineering-browser-evidence';
import { expect, test, type BrowserContext, type Page } from "@playwright/test";
import { readFile } from "node:fs/promises";
import { resolve } from "node:path";

type DemoUser = { email: string; role: string; roles?: string[]; label: string; userId: string };
type Credentials = { password: string; users: DemoUser[] };
type DemoState = { fixture: { sourceWarehouseId: string; targetWarehouseId: string; transitProductId: string } };

async function signInRole(context: BrowserContext, page: Page, credentials: Credentials, role: string, user: DemoUser, path: string) {
  const login = await context.request.post("/api/auth/sign-in/email", {
    headers: { Origin: (process.env.ERPX_E2E_BASE_URL ?? "http://127.0.0.1:3102") },
    data: { email: user.email, password: credentials.password },
  });
  expect(login.ok(), await login.text()).toBe(true);
  const endpoint = "/api/v1/holdings/simulation/assignments/active";
  const active = (await (await context.request.get(endpoint)).json()).data;
  const assignment = active.options.find((item: { role: string }) => item.role === role);
  expect(assignment, `${user.label} ${role} assignment`).toBeTruthy();
  const selected = await context.request.post(endpoint, {
    headers: { Origin: (process.env.ERPX_E2E_BASE_URL ?? "http://127.0.0.1:3102") },
    data: { assignmentId: assignment.id, revision: active.revision },
  });
  expect(selected.ok(), await selected.text()).toBe(true);
  await page.goto(`${path}${path.includes("?") ? "&" : "?"}refresh=${Date.now()}`, { waitUntil: "domcontentloaded" });
  await expect(page.locator("[data-work-stale]")).toHaveAttribute("data-work-stale", "false");
}

async function command(page: Page, endpoint: string, button: string) {
  await expect(page.getByRole("button", { name: button, exact: true })).toBeVisible();
  const response = page.waitForResponse((item) => item.request().method() === "POST" && item.url().includes(endpoint));
  await page.getByRole("button", { name: button, exact: true }).click();
  const result = await response;
  expect(result.status(), await result.text()).toBeLessThan(400);
  await page.waitForTimeout(700);
}

test("isolated demo carries an approved warehouse transfer through dispatch and partial receiving", async ({ browser }, testInfo) => {
  const run = process.env.ERPX_DEMO_RUN;
  test.skip(!run, "Set ERPX_DEMO_RUN to an exact prepared browser-simulation run.");
  test.setTimeout(120_000);
  const directory = resolve(`.runtime/browser/${run}`);
  const credentials = JSON.parse(await readFile(resolve(directory, "credentials.json"), "utf8")) as Credentials;
  const state = JSON.parse(await readFile(resolve(directory, "state.json"), "utf8")) as DemoState;
  const receiver = credentials.users.find((user) => user.role === "warehouse_manager");
  const sourceOperator = credentials.users.find((user) => user.userId !== receiver?.userId && user.roles?.includes("warehouse_manager"));
  expect(receiver, "warehouse manager demo account").toBeTruthy();
  expect(sourceOperator, "independent source warehouse manager demo account").toBeTruthy();
  const sourceContext = await createEngineeringBrowserContext(browser,testInfo);
  const receiverContext = await createEngineeringBrowserContext(browser,testInfo);
  try {
    const sourcePage = await sourceContext.newPage();
    console.log("stock-transit: source sign-in");
    await signInRole(sourceContext, sourcePage, credentials, "warehouse_manager", sourceOperator!, "/holdings/simulation/stock-transfers?new=1");
    console.log("stock-transit: transfer form loaded");
    await expect(sourcePage.getByRole("heading", { name: "Warehouse transfers", exact: true })).toBeVisible();
    const transferSelects = sourcePage.locator("#workspace-content details select");
    await transferSelects.nth(0).selectOption("transit");
    await transferSelects.nth(1).selectOption(state.fixture.sourceWarehouseId);
    await transferSelects.nth(2).selectOption(state.fixture.targetWarehouseId);
    await expect(transferSelects.nth(3).locator(`option[value="${state.fixture.transitProductId}"]`)).toHaveCount(1);
    await sourcePage.locator('#workspace-content input[name="transferDate"]').fill("2026-09-24");
    await sourcePage.locator('#workspace-content input[name="arrivalDueDate"]').fill("2026-09-26");
    await sourcePage.locator('#workspace-content input[name="reason"]').fill("Replenishment for the receiving warehouse");
    await transferSelects.nth(3).selectOption(state.fixture.transitProductId);
    await sourcePage.getByRole("button", { name: "Add transfer product", exact: true }).click();
    console.log("stock-transit: product added");
    await sourcePage.locator('#workspace-content input[type="number"]').first().fill("3");
    console.log("stock-transit: quantity filled");
    await command(sourcePage, "/stock-transfers", "Save transfer draft");
    console.log("stock-transit: draft saved");
    await sourcePage.waitForURL(/\/holdings\/simulation\/stock-transfers\/transfer-/);
    const transferPath = new URL(sourcePage.url()).pathname;
    const transferId = transferPath.split("/").at(-1)!;
    await expect(sourcePage.locator("#workspace-content").getByText("draft", { exact: true }).first()).toBeVisible();
    await sourcePage.locator("#workspace-content textarea").first().fill("Source stock and receiving route checked");
    await command(sourcePage, `/stock-transfers/${transferId}/submit`, "Submit transport for approval");
    console.log("stock-transit: submitted");

    const receiverPage = await receiverContext.newPage();
    await signInRole(receiverContext, receiverPage, credentials, "warehouse_manager", receiver!, transferPath);
    await expect(receiverPage.locator("#workspace-content").getByText("submitted", { exact: true }).first()).toBeVisible();
    await receiverPage.locator("#workspace-content textarea").first().fill("Independent receiving warehouse review authorizes the carrier dispatch");
    await command(receiverPage, `/stock-transfers/${transferId}/post`, "Approve transport for dispatch");
    console.log("stock-transit: approved");
    await receiverPage.getByRole("link", { name: "Open dispatch, receipt and discrepancies", exact: true }).click();
    await receiverPage.waitForURL(/\/holdings\/simulation\/stock-transit\/transit-/);
    const transitPath = new URL(receiverPage.url()).pathname;

    await sourcePage.goto(transitPath, { waitUntil: "domcontentloaded" });
    await expect(sourcePage.getByRole("heading", { name: "Record source dispatch", exact: true })).toBeVisible();
    await sourcePage.locator('#workspace-content input[name="date"]').first().fill("2026-09-24");
    await sourcePage.locator('#workspace-content input[name="tracking"]').fill("DEMO-TRANSIT-001");
    await sourcePage.locator("#workspace-content textarea").first().fill("Carrier collected three sealed simulation units.");
    await command(sourcePage, `/stock-transit/${transitPath.split("/").at(-1)}/dispatch`, "Confirm physical dispatch");
    console.log("stock-transit: dispatched");

    await receiverPage.goto(transitPath, { waitUntil: "domcontentloaded" });
    await expect(receiverPage.getByRole("heading", { name: "Record actual receiving outcome", exact: true })).toBeVisible();
    await receiverPage.locator('#workspace-content input[name="quantity"]').fill("1");
    await receiverPage.locator('#workspace-content input[name="date"]').fill("2026-09-25");
    await receiverPage.locator("#workspace-content textarea").first().fill("First carton counted and signed into the destination warehouse.");
    await command(receiverPage, `/stock-transit/${transitPath.split("/").at(-1)}/receive`, "Record receiving outcome");
    console.log("stock-transit: partial receipt");
    const partial = await receiverContext.request.get(`/api/v1/holdings/simulation/stock-transit/${transitPath.split("/").at(-1)}`);
    expect(partial.ok(), await partial.text()).toBe(true);
    expect((await partial.json()).data.lines[0]).toMatchObject({ received_milli: 1000, quantity_milli: 3000 });

    await receiverPage.locator('#workspace-content input[name="quantity"]').fill("2");
    await receiverPage.locator("#workspace-content textarea").first().fill("Remaining two cartons counted and signed into the destination warehouse.");
    await command(receiverPage, `/stock-transit/${transitPath.split("/").at(-1)}/receive`, "Record receiving outcome");
    console.log("stock-transit: final receipt");
    await receiverPage.reload({ waitUntil: "domcontentloaded" });
    await expect(receiverPage.locator("#workspace-content").getByText("completed", { exact: true }).first()).toBeVisible();
    const finished = await receiverContext.request.get(`/api/v1/holdings/simulation/stock-transit/${transitPath.split("/").at(-1)}`);
    expect(finished.ok(), await finished.text()).toBe(true);
    const detail = (await finished.json()).data;
    expect(detail.lines[0]).toMatchObject({ received_milli: 3000, lost_milli: 0 });
    const stock = await receiverContext.request.get("/api/v1/holdings/simulation/commerce/overview?view=stock");
    expect(stock.ok(), await stock.text()).toBe(true);
    const stockData = (await stock.json()).data;
    expect(stockData.stock.find((row: { warehouse_id: string; product_id: string }) => row.warehouse_id === state.fixture.targetWarehouseId && row.product_id === state.fixture.transitProductId)).toMatchObject({ quantity_milli: 3000, value_minor: 1000 });
  } finally {
    await Promise.all([sourceContext.close(), receiverContext.close()]);
  }
});
