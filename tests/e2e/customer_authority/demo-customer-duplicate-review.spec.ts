import {createEngineeringBrowserContext} from "../../support/browser/engineering-browser-evidence";
import { expect, test, type BrowserContext, type Page } from "@playwright/test";
import { readFile } from "node:fs/promises";
import { resolve } from "node:path";

type DemoUser = { email: string; role: string; roles?: string[] };
type Credentials = { password: string; users: DemoUser[] };
type DemoState = {
  fixture?: {
    customerDuplicateIds?: [string, string];
    customerDuplicateQuotationId?: string;
  };
};

async function signInRole(
  context: BrowserContext,
  page: Page,
  credentials: Credentials,
  role: string,
  path: string,
) {
  const user = credentials.users.find((item) => item.roles?.includes(role) || item.role === role);
  expect(user, `${role} demo account`).toBeTruthy();
  const headers = { Origin: (process.env.ERPX_E2E_BASE_URL ?? "http://127.0.0.1:3102") };
  const login = await context.request.post("/api/auth/sign-in/email", {
    headers,
    data: { email: user!.email, password: credentials.password },
  });
  expect(login.ok(), await login.text()).toBe(true);
  const endpoint = "/api/v1/holdings/simulation/assignments/active";
  const active = (await (await context.request.get(endpoint)).json()).data;
  const assignment = active.options.find((item: { role: string }) => item.role === role);
  expect(assignment, `${role} active assignment`).toBeTruthy();
  const selected = await context.request.post(endpoint, {
    headers,
    data: { assignmentId: assignment.id, revision: active.revision },
  });
  expect(selected.ok(), await selected.text()).toBe(true);
  await page.goto(path, { waitUntil: "commit" });
  await expect(page.locator("[data-work-stale]")).toHaveAttribute("data-work-stale", "false");
}

test("isolated demo independently reviews and merges duplicate customers", async ({ browser }, testInfo) => {
  const run = process.env.ERPX_DEMO_RUN;
  test.skip(!run, "Set ERPX_DEMO_RUN to an exact prepared browser-simulation run.");
  test.setTimeout(120_000);
  const directory = resolve(`.runtime/browser/${run}`);
  const credentials = JSON.parse(await readFile(resolve(directory, "credentials.json"), "utf8")) as Credentials;
  const state = JSON.parse(await readFile(resolve(directory, "state.json"), "utf8")) as DemoState;
  const duplicateIds = state.fixture?.customerDuplicateIds;
  const quotationId = state.fixture?.customerDuplicateQuotationId;
  expect(duplicateIds).toHaveLength(2);
  expect(quotationId).toBeTruthy();

  const requesterContext = await createEngineeringBrowserContext(browser,testInfo);
  const reviewerContext = await createEngineeringBrowserContext(browser,testInfo);
  const requesterPage = await requesterContext.newPage();
  const reviewerPage = await reviewerContext.newPage();
  const base = "/holdings/simulation";
  const listPath = `${base}/customer-duplicates`;
  let reviewId = "";
  try {
    await signInRole(requesterContext, requesterPage, credentials, "holding_owner", listPath);
    const reviewWorkspace = requesterPage.locator("#workspace-content:visible");
    await expect(reviewWorkspace).toHaveCount(1);
    await expect(reviewWorkspace.getByRole("heading", { name: "Customers", exact: true, level: 1 })).toBeVisible();
    await expect(reviewWorkspace.getByRole("navigation", { name: "Customer review type", exact: true }).getByRole("link", { name: "Duplicates", exact: true })).toHaveAttribute("aria-current", "page");
    await reviewWorkspace.getByRole("link", { name: "New review request", exact: true }).click();
    const reviewDrawer = requesterPage.getByRole("dialog", { name: "New review request", exact: true });
    await expect(reviewDrawer).toBeVisible();
    await reviewDrawer.getByLabel("First customer").selectOption(duplicateIds![0]);
    await reviewDrawer.getByLabel("Second customer").selectOption(duplicateIds![1]);
    await reviewDrawer.getByLabel("Evidence and reason").fill(
      "Matching simulated tax registration and near-matching legal names require independent identity review.",
    );
    const requestResponsePromise = requesterPage.waitForResponse(
      (response) => response.url().includes("/api/v1/holdings/simulation/customer-duplicates") && response.request().method() === "POST",
    );
    await reviewDrawer.getByRole("button", { name: "Send for independent review", exact: true }).click();
    const requestResponse = await requestResponsePromise;
    expect(requestResponse.ok(), await requestResponse.text()).toBe(true);
    const requestResult = (await requestResponse.json()).data;
    reviewId = String(requestResult.id);
    expect(requestResult).toMatchObject({ version: 1, status: "pending" });
    await expect(reviewDrawer).not.toBeVisible();
    const requestedReview = requesterPage.getByTestId("record-detail-drawer").filter({ visible: true });
    await expect(requestedReview).toHaveCount(1);
    const fullReview = requestedReview.getByRole("link", { name: "Open full page ↗", exact: true });
    await expect(fullReview).toHaveAttribute("href", `${listPath}/${reviewId}`);
    await fullReview.click();
    await expect(requesterPage).toHaveURL(new RegExp(`${listPath}/${reviewId}$`));
    await expect(requesterPage.getByRole("table", { name: "Customer identity comparison" })).toContainText("SIM-DUP-A");
    await expect(requesterPage.getByRole("table", { name: "Customer identity comparison" })).toContainText("SIM-DUP-B");
    await expect(requesterPage.getByRole("table", { name: "Customer identity comparison" })).toContainText("SIM-TAX-204812340001");
    const comparison = requesterPage.locator("#workspace-content:visible section.ui-panel:visible").filter({ has: requesterPage.getByRole("heading", { name: "Compare customer records" }) });
    await expect(comparison.getByText(/Matching simulated tax registration/)).toBeVisible();
    await expect(requesterPage.getByRole("button", { name: "Record independent decision", exact: true })).toHaveCount(0);
    await expect(requesterPage.getByRole("heading", { name: "Identity decision", exact: true })).toHaveCount(0);

    await signInRole(reviewerContext, reviewerPage, credentials, "sales_manager", `${base}/customer-duplicates/${reviewId}`);
    await expect(reviewerPage.getByRole("table", { name: "Customer identity comparison" })).toContainText("SIM-TAX-204812340001");
    await reviewerPage.locator('#workspace-content:visible').getByLabel("Decision").selectOption("duplicate");
    await reviewerPage.locator('#workspace-content:visible').getByLabel("Canonical record").selectOption(duplicateIds![0]);
    await reviewerPage.locator('#workspace-content:visible').getByLabel("Evidence and reason").fill(
      "Compared both company records and retained the first customer master as canonical.",
    );
    const decisionResponsePromise = reviewerPage.waitForResponse(
      (response) => response.url().includes("/api/v1/holdings/simulation/customer-duplicates") && response.request().method() === "POST",
    );
    await reviewerPage.getByRole("button", { name: "Record independent decision", exact: true }).click();
    const decisionResponse = await decisionResponsePromise;
    expect(decisionResponse.ok(), await decisionResponse.text()).toBe(true);
    expect((await decisionResponse.json()).data).toMatchObject({
      version: 2,
      status: "duplicate",
    });
    await expect(reviewerPage.locator("#workspace-content:visible").getByText(new RegExp(`duplicate · canonical ${duplicateIds![0]}`))).toBeVisible();
    await expect(reviewerPage.locator("#workspace-content:visible").getByText(/Version 2 · duplicate · Compared both company records/)).toBeVisible();

    reviewerPage.once("dialog", async (dialog) => {
      expect(dialog.message()).toContain("Merge safe source records");
      await dialog.accept();
    });
    const mergeForm = reviewerPage.locator("#workspace-content:visible form").filter({ has: reviewerPage.getByRole("button", { name: "Merge into canonical customer", exact: true }) });
    await mergeForm.getByLabel("Evidence and reason").fill(
      "Merged the eligible quotation into the selected canonical customer after reviewing source ownership.",
    );
    const mergeResponsePromise = reviewerPage.waitForResponse(
      (response) => response.url().includes("/api/v1/holdings/simulation/customer-duplicates") && response.request().method() === "POST",
    );
    await reviewerPage.getByRole("button", { name: "Merge into canonical customer", exact: true }).click();
    const mergeResponse = await mergeResponsePromise;
    expect(mergeResponse.ok(), await mergeResponse.text()).toBe(true);
    expect((await mergeResponse.json()).data).toMatchObject({
      version: 3,
      status: "merged",
      canonicalCustomerId: duplicateIds![0],
      mergedCustomerId: duplicateIds![1],
    });
    await expect(reviewerPage.locator("#workspace-content:visible").getByText(/merged · canonical/)).toBeVisible();
    await expect(reviewerPage.locator("#workspace-content:visible").getByText(/Version 3 · merge · Merged the eligible quotation/)).toBeVisible();
    await expect(reviewerPage.getByRole("heading", { name: "Identity decision", exact: true })).toHaveCount(0);

    const detailResponse = await reviewerContext.request.get(`${base.replace("/holdings/", "/api/v1/holdings/")}/customer-duplicates?id=${encodeURIComponent(reviewId)}`);
    expect(detailResponse.ok(), await detailResponse.text()).toBe(true);
    const detail = (await detailResponse.json()).data;
    expect(detail.row).toMatchObject({ status: "merged", canonical_customer_id: duplicateIds![0] });
    const loser = String(detail.row.left_customer_id) === duplicateIds![1] ? detail.row.left_merged_into_id : detail.row.right_merged_into_id;
    expect(loser).toBe(duplicateIds![0]);
    expect(detail.history.map((event: { action: string }) => event.action)).toEqual(["request", "duplicate", "merge"]);
    await reviewerPage.evaluate(() => window.scrollTo(0, 0));
    await reviewerPage.screenshot({ path: testInfo.outputPath("customer-duplicate-merged.png"), fullPage: true });

    await reviewerPage.goto(`${base}/customers/${duplicateIds![0]}`, { waitUntil: "commit" });
    await expect(reviewerPage.getByRole("heading", { name: "Demo Harbor Logistics", exact: true })).toBeVisible();
    const sourceActivity = reviewerPage.locator("#workspace-content:visible section").filter({ has: reviewerPage.getByRole("heading", { name: "Pending work", exact: true }) });
    await expect(sourceActivity).toHaveCount(1);
    await expect(sourceActivity.getByRole("link").filter({ hasText: /Quotation ·/ })).toHaveAttribute(
      "href",
      new RegExp(`/commerce/orders/${quotationId}\\?view=quote$`),
    );
    const customerHeader = reviewerPage.locator("#workspace-content:visible header").filter({ has: reviewerPage.getByRole("heading", { name: "Demo Harbor Logistics", exact: true }) });
    await expect(customerHeader).toHaveCount(1);
    await expect(customerHeader).toContainText("SIM-DUP-A");
    const historyHref = `${base}/customers/${duplicateIds![0]}?section=history`;
    const historyLink = reviewerPage.locator("#workspace-content:visible").getByRole("navigation", { name: "Customer profile views", exact: true }).getByRole("link", { name: "History", exact: true });
    await expect(historyLink).toHaveAttribute("href", historyHref);
    await historyLink.click();
    const historyDrawer = reviewerPage.getByTestId("record-detail-drawer").filter({ visible: true });
    await expect(historyDrawer).toHaveCount(1);
    const fullHistory = historyDrawer.getByRole("link", { name: "Open full page ↗", exact: true });
    await expect(fullHistory).toHaveAttribute("href", historyHref);
    await fullHistory.click();
    await expect(reviewerPage).toHaveURL(new URL(historyHref, reviewerPage.url()).href);
    await expect(reviewerPage.locator("#workspace-content:visible").getByRole("heading", { name: "Customer history", exact: true })).toBeVisible();
    await reviewerPage.evaluate(() => window.scrollTo(0, 0));

    await testInfo.attach("customer-duplicate-review-result", {
      contentType: "application/json",
      body: JSON.stringify({
        run,
        reviewId,
        roles: ["holding_owner", "sales_manager"],
        canonicalCustomerId: duplicateIds![0],
        mergedCustomerId: duplicateIds![1],
        comparisonFieldsVisible: ["code", "tax registration", "submitted evidence"],
        decisionHistory: detail.history.map((event: { action: string }) => event.action),
        safeQuotationRepointed: quotationId,
        losingCustomerBlockedIntoCanonical: loser,
      }, null, 2),
    });
    await reviewerPage.screenshot({ path: testInfo.outputPath("customer-canonical-timeline.png"), fullPage: true });
  } finally {
    await Promise.allSettled([requesterContext.close(), reviewerContext.close()]);
  }
});
