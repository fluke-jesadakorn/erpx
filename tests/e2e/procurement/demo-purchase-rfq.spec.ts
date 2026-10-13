import {createEngineeringBrowserContext} from "../../support/browser/engineering-browser-evidence";
import { expect, test, type BrowserContext, type Page } from "@playwright/test";
import { readFile } from "node:fs/promises";
import { resolve } from "node:path";

type DemoUser = { email: string; role: string; roles?: string[]; label: string };
type Credentials = { password: string; users: DemoUser[] };

async function signInAsPurchasingManager(
  context: BrowserContext,
  page: Page,
  credentials: Credentials,
  user: DemoUser,
  path: string,
) {
  const login = await context.request.post("/api/auth/sign-in/email", {
    data: { email: user.email, password: credentials.password },
  });
  expect(login.ok()).toBe(true);
  const endpoint = "/api/v1/holdings/simulation/assignments/active";
  let active = (await (await context.request.get(endpoint)).json()).data;
  const assignment = active.options.find(
    (item: { role: string }) => item.role === "purchasing_manager",
  );
  expect(assignment, `${user.label} purchasing manager assignment`).toBeTruthy();
  const selected = await context.request.post(endpoint, {
    data: { assignmentId: assignment.id, revision: active.revision },
  });
  expect(selected.ok()).toBe(true);
  active = (await (await context.request.get(endpoint)).json()).data;
  expect(active.assignmentId).toBe(assignment.id);
  await page.goto(path, { waitUntil: "domcontentloaded" });
  const workspace = page.locator("#workspace-content");
  await expect(workspace).toHaveCount(1);
  await expect(workspace).toBeVisible();
  await expect(page.locator("[data-work-stale]")).toHaveAttribute(
    "data-work-stale",
    "false",
  );
}

function actionSelect(page: Page) {
  return page.locator("#workspace-content label").filter({ hasText: "Action" }).locator("select");
}

test("isolated demo creates, awards and hands off a supplier quotation to a draft PO", async ({ browser }, testInfo) => {
  const run = process.env.ERPX_DEMO_RUN;
  test.skip(!run, "Set ERPX_DEMO_RUN to an exact prepared browser-simulation run.");
  test.setTimeout(90_000);
  const directory = resolve(`.runtime/browser/${run}`);
  const credentials = JSON.parse(
    await readFile(resolve(directory, "credentials.json"), "utf8"),
  ) as Credentials;
  const buyer = credentials.users.find((user) => user.role === "purchasing_manager");
  const approver = credentials.users.find((user) => user.label === "Support manager");
  expect(buyer).toBeTruthy();
  expect(approver).toBeTruthy();
  const buyerContext = await createEngineeringBrowserContext(browser,testInfo);
  const approverContext = await createEngineeringBrowserContext(browser,testInfo);
  try {
    const buyerPage = await buyerContext.newPage();
    await signInAsPurchasingManager(
      buyerContext,
      buyerPage,
      credentials,
      buyer!,
      "/holdings/simulation/procurement-rfqs",
    );
    await expect(
      buyerPage.getByRole("heading", { name: "Supplier quotation requests", exact: true }),
    ).toBeVisible();
    await buyerPage.getByRole("button", { name: "Add demand product", exact: true }).click();
    await buyerPage.locator("#workspace-content").getByLabel("Demand title", { exact: true }).fill("Demo laptops for service team");
    await buyerPage.locator("#workspace-content label").filter({ hasText: "Receiving warehouse" }).locator("select").selectOption({ index: 2 });
    await buyerPage.locator("#workspace-content").getByLabel("Required date", { exact: true }).fill("2026-10-15");
    await buyerPage.locator("#workspace-content")
      .getByLabel("Demand source and justification", { exact: true })
      .fill("Approved service-team replacement demand for the isolated demo company.");
    const productSelect = buyerPage.locator("#workspace-content label").filter({ hasText: "Demand product 1" }).locator("select");
    const laptopValue = await productSelect.locator("option").filter({ hasText: "SIM-LAP-EACH" }).getAttribute("value");
    expect(laptopValue).toBeTruthy();
    await productSelect.selectOption(laptopValue!);
    await buyerPage.locator("#workspace-content").getByLabel("Purchase quantity", { exact: false }).fill("1");
    await Promise.all([
      buyerPage.waitForURL(/\/procurement-rfqs\/rfq-/),
      buyerPage.getByRole("button", { name: "Create quotation request", exact: true }).click(),
    ]);
    const rfqId = new URL(buyerPage.url()).pathname.split("/").at(-1)!;
    await expect(buyerPage.getByRole("heading", { name: "Record supplier offer", exact: true })).toBeVisible();
    await buyerPage.locator("#workspace-content").getByLabel("Supplier", { exact: false }).selectOption({ index: 1 });
    await buyerPage.locator("#workspace-content").getByLabel("Quotation reference", { exact: true }).fill("DEMO-RFQ-LAPTOP-1");
    await buyerPage.locator("#workspace-content").getByLabel("Quoted arrival", { exact: true }).fill("2026-10-10");
    await buyerPage.locator("#workspace-content").getByLabel("Quote valid through", { exact: true }).fill("2026-10-20");
    await buyerPage.locator("#workspace-content").getByLabel("Quotation evidence and terms", { exact: true }).fill("Supplier quotation with price and delivery terms.");
    await buyerPage.locator("#workspace-content").locator('input[name^="price-"]').fill("25000");
    const offerResponse = buyerPage.waitForResponse((response) =>
      response.request().method() === "POST" && response.url().includes(`/api/v1/holdings/simulation/procurement-rfqs/${rfqId}`),
    );
    await buyerPage.getByRole("button", { name: "Record quotation", exact: true }).click();
    expect((await offerResponse).ok()).toBe(true);
    await buyerPage.reload({ waitUntil: "domcontentloaded" });
    await expect(buyerPage.locator("#workspace-content").locator("td").filter({ hasText: "DEMO-RFQ-LAPTOP-1" }).first()).toBeVisible();
    await actionSelect(buyerPage).selectOption("submit");
    await buyerPage.locator("#workspace-content").getByLabel("Decision and delivery exception reason", { exact: true }).fill("Best comparable supplier offer for the approved laptop demand.");
    const submitResponse = buyerPage.waitForResponse((response) =>
      response.request().method() === "POST" && response.url().includes(`/api/v1/holdings/simulation/procurement-rfqs/${rfqId}`),
    );
    await buyerPage.getByRole("button", { name: "Record decision", exact: true }).click();
    expect((await submitResponse).ok()).toBe(true);
    await buyerPage.reload({ waitUntil: "domcontentloaded" });
    await expect(buyerPage.locator("#workspace-content").locator("p").filter({ hasText: "Required" }).first()).toContainText("review");

    const approverPage = await approverContext.newPage();
    await signInAsPurchasingManager(
      approverContext,
      approverPage,
      credentials,
      approver!,
      `/holdings/simulation/procurement-rfqs/${rfqId}`,
    );
    await expect(approverPage.locator("#workspace-content").locator("p").filter({ hasText: "Required" }).first()).toContainText("review");
    await actionSelect(approverPage).selectOption("approve");
    await approverPage.locator("#workspace-content").getByLabel("Decision and delivery exception reason", { exact: true }).fill("Independently reviewed supplier comparison and award.");
    const approveResponse = approverPage.waitForResponse((response) =>
      response.request().method() === "POST" && response.url().includes(`/api/v1/holdings/simulation/procurement-rfqs/${rfqId}`),
    );
    await approverPage.getByRole("button", { name: "Record decision", exact: true }).click();
    expect((await approveResponse).ok()).toBe(true);
    await approverPage.reload({ waitUntil: "domcontentloaded" });
    await expect(approverPage.locator("#workspace-content").locator("p").filter({ hasText: "Required" }).first()).toContainText("approved");

    await buyerPage.goto(`/holdings/simulation/procurement-rfqs/${rfqId}?refresh=${Date.now()}`, { waitUntil: "domcontentloaded" });
    await expect(buyerPage.getByRole("heading", { name: "Create purchase order", exact: true })).toBeVisible();
    await buyerPage.locator("#workspace-content").getByLabel("Purchase order date", { exact: true }).fill("2026-09-22");
    await Promise.all([
      buyerPage.waitForURL(/\/commerce\/orders\//),
      buyerPage.getByRole("button", { name: "Create awarded draft PO", exact: true }).click(),
    ]);
    const detail = await buyerContext.request.get(`/api/v1/holdings/simulation/procurement-rfqs/${rfqId}`);
    expect(detail.ok()).toBe(true);
    expect((await detail.json()).data.rfq).toMatchObject({ status: "ordered" });
  } finally {
    await Promise.all([buyerContext.close(), approverContext.close()]);
  }
});
