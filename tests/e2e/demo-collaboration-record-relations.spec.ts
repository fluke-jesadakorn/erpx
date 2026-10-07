import {createEngineeringBrowserContext} from './helpers/engineering-browser-evidence';
import { expect, test, type BrowserContext, type Page } from "@playwright/test";
import { readFile } from "node:fs/promises";
import { resolve } from "node:path";

type DemoUser = {
  email: string;
  role: string;
  roles?: string[];
  userId: string;
};
type Credentials = { password: string; users: DemoUser[] };
type DemoGuide = {
  scenarios: Array<{ id: string; sourceRecord?: { id: string } }>;
};

async function signInRole(
  context: BrowserContext,
  page: Page,
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
  await page.goto(`/holdings/simulation/collaboration?refresh=${Date.now()}`, {
    waitUntil: "domcontentloaded",
  });
  await expect(page.locator("[data-work-stale]")).toHaveAttribute(
    "data-work-stale",
    "false",
  );
  return user!;
}

test("isolated demo links a support case to its sales source and records removal", async ({
  browser,
}, testInfo) => {
  const run = process.env.ERPX_DEMO_RUN;
  test.skip(!run, "Set ERPX_DEMO_RUN to an exact prepared browser-simulation run.");
  const directory = resolve(`.runtime/browser/${run}`);
  const credentials = JSON.parse(
    await readFile(resolve(directory, "credentials.json"), "utf8"),
  ) as Credentials;
  const guide = JSON.parse(
    await readFile(resolve(directory, "demo-guide.json"), "utf8"),
  ) as DemoGuide;
  const ownerContext = await createEngineeringBrowserContext(browser,testInfo);
  try {
    const ownerPage = await ownerContext.newPage();
    const salesScenario = guide.scenarios.find(
      (scenario) => scenario.id === "sales-to-collection",
    );
    expect(salesScenario?.sourceRecord?.id, "demo sales quote source").toBeTruthy();
    const supportAgent = credentials.users.find((user) =>
      user.roles?.includes("support_agent"),
    );
    expect(supportAgent, "support case owner").toBeTruthy();

    await signInRole(ownerContext, ownerPage, credentials, "holding_owner");
    const quoteId = salesScenario!.sourceRecord!.id;
    const title = "Customer equipment issue linked to quotation";
    await ownerPage.goto(
      `/holdings/simulation/support?orderId=${encodeURIComponent(quoteId)}`,
      { waitUntil: "domcontentloaded" },
    );
    await expect(ownerPage).toHaveURL(url => url.pathname === "/holdings/simulation/customers" && url.searchParams.get("view") === "support" && url.searchParams.get("new") === "1" && url.searchParams.get("orderId") === quoteId);
    const supportDrawer = ownerPage.getByRole("dialog", { name: "New service case", exact: true });
    await expect(supportDrawer).toBeVisible();
    const supportForm = supportDrawer.locator("form").filter({
      has: supportDrawer.getByRole("heading", { name: "Record customer issue from order", exact: true }),
    });
    await expect(supportForm).toHaveCount(1);
    await supportForm.locator('input[name="title"]').fill(title);
    await supportForm
      .locator('textarea[name="issue"]')
      .fill("Customer needs a replacement delivery date confirmed by Support.");
    await supportForm.locator('select[name="owner"]').selectOption(supportAgent!.userId);
    await supportForm.locator('input[name="due"]').fill("2026-10-05");
    const createResponse = ownerPage.waitForResponse(
      (response) =>
        response.request().method() === "POST" &&
        response.url().endsWith("/api/v1/holdings/simulation/support"),
    );
    await supportForm
      .getByRole("button", { name: "Create linked support case", exact: true })
      .click();
    const createdSupport = await createResponse;
    expect(createdSupport.status()).toBe(200);
    const createdTicketId = (await createdSupport.json()).data.id as string;
    expect(createdTicketId, "actual created Support command result").toBeTruthy();
    await expect(ownerPage).toHaveURL(url => url.pathname === "/holdings/simulation/customers" && url.searchParams.get("detailKind") === "support_ticket" && url.searchParams.get("detailId") === createdTicketId);
    await expect(supportDrawer).not.toBeVisible();
    const supportRecord = ownerPage.getByTestId("record-detail-drawer");
    await expect(supportRecord).toHaveCount(1);
    await expect(supportRecord.locator("[data-record-drawer-content]").getByRole("heading", { name: title, exact: true })).toBeVisible();
    const supportFullPage = supportRecord.getByRole("link", { name: "Open full page ↗", exact: true });
    await expect(supportFullPage).toHaveAttribute("href", `/holdings/simulation/support/${createdTicketId}`);
    await supportFullPage.click();
    await expect(ownerPage).toHaveURL(new RegExp(`/support/${createdTicketId}$`));
    await expect(ownerPage.locator("#workspace-content:visible").getByRole("heading", { name: title, exact: true })).toBeVisible();
    const ticketId = new URL(ownerPage.url()).pathname.split("/").pop();
    expect(ticketId, "created support ticket id").toBeTruthy();
    expect(ticketId).toBe(createdTicketId);

    await ownerPage.goto("/holdings/simulation/collaboration", {
      waitUntil: "domcontentloaded",
    });
    await expect(ownerPage.getByRole("heading", { name: "Cross-record collaboration" })).toBeVisible();
    await ownerPage.locator("#workspace-content:visible summary").filter({ hasText: /^Link records$/ }).click();
    const relationDrawer = ownerPage.getByRole("dialog", { name: "Link records", exact: true });
    const relationForm = relationDrawer.locator("form");
    await expect(relationDrawer).toBeVisible();
    await expect(relationForm).toHaveCount(1);
    await relationForm.locator('select[name="sourceType"]').selectOption("commerce_order");
    await relationForm.locator('input[name="sourceId"]').fill(quoteId);
    await relationForm.locator('select[name="targetType"]').selectOption("support_ticket");
    await relationForm.locator('input[name="targetId"]').fill(ticketId!);
    await relationForm.locator('select[name="relation"]').selectOption("related");
    await relationForm
      .locator('textarea[name="reason"]')
      .fill("Support case originated from this customer quotation.");
    const linkResponse = ownerPage.waitForResponse(
      (response) =>
        response.request().method() === "POST" &&
        response.url().endsWith("/api/v1/holdings/simulation/collaboration"),
    );
    await relationForm.getByRole("button", { name: "Create typed relation" }).click();
    expect((await linkResponse).status()).toBe(200);
    await relationDrawer.getByRole("button", { name: "Close dialog", exact: true }).click();
    await expect(relationDrawer).not.toBeVisible();
    const relationCard = ownerPage.locator("#workspace-content:visible form").filter({ hasText: "Remove relation" });
    await expect(relationCard).toContainText(`commerce_order ${quoteId}`);
    await expect(relationCard).toContainText(`support_ticket ${ticketId}`);

    await relationCard
      .locator('textarea[name="reason"]')
      .fill("The customer issue was linked to the wrong source and is being corrected.");
    const removeResponse = ownerPage.waitForResponse(
      (response) =>
        response.request().method() === "POST" &&
        response.url().endsWith("/api/v1/holdings/simulation/collaboration"),
    );
    await relationCard.getByRole("button", { name: "Remove relation" }).click();
    expect((await removeResponse).status()).toBe(200);
    await expect(ownerPage.locator("#workspace-content:visible form").filter({ hasText: "Remove relation" })).toHaveCount(0);
  } finally {
    await ownerContext.close();
  }
});
