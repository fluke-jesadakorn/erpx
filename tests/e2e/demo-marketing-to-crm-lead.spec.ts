import {createEngineeringBrowserContext} from './helpers/engineering-browser-evidence';
import { expect, test, type BrowserContext, type Page } from "@playwright/test";
import { readFile } from "node:fs/promises";
import { resolve } from "node:path";

type User = { email: string; role: string; roles?: string[]; userId: string };
type Credentials = { password: string; users: User[] };
const origin = (process.env.ERPX_E2E_BASE_URL ?? "http://127.0.0.1:3102");

async function signIn(context: BrowserContext, page: Page, credentials: Credentials, role: string, path: string) {
  const user = credentials.users.find(item => item.role === role || item.roles?.includes(role));
  expect(user, `${role} Demo account`).toBeTruthy();
  const login = await context.request.post("/api/auth/sign-in/email", { headers: { Origin: origin }, data: { email: user!.email, password: credentials.password } });
  expect(login.ok(), await login.text()).toBe(true);
  const endpoint = "/api/v1/holdings/simulation/assignments/active";
  const active = (await (await context.request.get(endpoint)).json()).data;
  const assignment = active.options.find((item: { role: string }) => item.role === role);
  expect(assignment, `${role} assignment`).toBeTruthy();
  const selected = await context.request.post(endpoint, { headers: { Origin: origin }, data: { assignmentId: assignment.id, revision: active.revision } });
  expect(selected.ok(), await selected.text()).toBe(true);
  await page.goto(path);
  await expect(page.locator("[data-work-stale]")).toHaveAttribute("data-work-stale", "false");
  return user!;
}

test("consented Marketing candidate hands its brief and attribution to an eligible CRM owner", async ({ browser }, testInfo) => {
  const run = process.env.ERPX_DEMO_RUN;
  test.skip(!run, "Set ERPX_DEMO_RUN to an exact prepared browser-simulation run.");
  test.setTimeout(120_000);
  const credentials = JSON.parse(await readFile(resolve(`.runtime/browser/${run}/credentials.json`), "utf8")) as Credentials;
  const marketingContext = await createEngineeringBrowserContext(browser,testInfo);
  const reviewerContext = await createEngineeringBrowserContext(browser,testInfo);
  const salesContext = await createEngineeringBrowserContext(browser,testInfo);
  try {
    const marketingPage = await marketingContext.newPage(), reviewerPage = await reviewerContext.newPage(), salesPage = await salesContext.newPage();
    const marketer = await signIn(marketingContext, marketingPage, credentials, "sales_manager", "/holdings/simulation/marketing");
    const reviewer = await signIn(reviewerContext, reviewerPage, credentials, "holding_owner", "/holdings/simulation/marketing");
    const submitter = await signIn(salesContext, salesPage, credentials, "holding_owner", "/holdings/simulation/marketing");
    expect(marketer.userId).not.toBe(reviewer.userId);
    expect(marketer.userId).not.toBe(submitter.userId);

    await marketingPage.locator("#workspace-content:visible summary").filter({ hasText: /^Draft marketing asset$/ }).click();
    const assetDrawer = marketingPage.getByRole("dialog", { name: "Draft marketing asset", exact: true });
    const assetForm = assetDrawer.locator("form");
    await expect(assetDrawer).toBeVisible();
    await expect(assetForm).toHaveCount(1);
    await assetForm.getByLabel("Kind").selectOption("form");
    await assetForm.getByLabel("Name").fill("Demo furniture inquiry");
    await assetForm.getByLabel("Public headline").fill("Tell us what you need");
    await assetForm.getByLabel("Supporting text").fill("Share a few details and our Sales team will follow up.");
    await assetForm.getByLabel("Require visitor consent before submitting").check();
    const draftResponse = marketingPage.waitForResponse(response => response.request().method() === "POST" && new URL(response.url()).pathname.endsWith("/marketing"));
    await assetForm.getByRole("button", { name: "Draft asset" }).click();
    const draft = await (await draftResponse).json();
    const formId = draft.data.id as string;
    await assetDrawer.getByRole("button", { name: "Close dialog", exact: true }).click();
    await expect(assetDrawer).not.toBeVisible();
    await expect(marketingPage.getByRole("heading", { name: "Independent review · Demo furniture inquiry" })).toBeVisible();

    await reviewerPage.reload();
    const reviewForm = reviewerPage.locator("#workspace-content:visible").locator("form").filter({ has: reviewerPage.getByRole("heading", { name: "Independent review · Demo furniture inquiry" }) });
    await reviewForm.getByLabel("Reason").fill("Consent fields and inquiry preview reviewed.");
    let responsePromise = reviewerPage.waitForResponse(response => response.request().method() === "POST" && new URL(response.url()).pathname.endsWith("/marketing"));
    await reviewForm.getByRole("button", { name: "Activate" }).click();
    expect((await responsePromise).status()).toBe(200);

    await reviewerPage.reload();
    await reviewerPage.locator("#workspace-content:visible summary").filter({ hasText: /^Capture consented lead$/ }).click();
    const captureDrawer = reviewerPage.getByRole("dialog", { name: "Capture consented lead", exact: true });
    const capture = captureDrawer.locator("form");
    await expect(captureDrawer).toBeVisible();
    await expect(capture).toHaveCount(1);
    await expect(capture.getByLabel("Active form")).toHaveValue(formId);
    await capture.getByLabel("Contact name").fill("Demo prospect - Bangkok");
    await capture.getByLabel("Contact detail").fill("demo-prospect@example.test");
    await capture.getByLabel("Consent evidence").fill("Explicit consent on approved Demo inquiry form.");
    await capture.getByLabel("Campaign", { exact: true }).fill("Bangkok furniture launch");
    await capture.getByLabel("Source").fill("Demo website inquiry");
    await capture.getByLabel("Brief").fill("Needs 20 furniture sets within seven days; budget THB 100,000.");
    responsePromise = reviewerPage.waitForResponse(response => response.request().method() === "POST" && new URL(response.url()).pathname.endsWith("/marketing"));
    await capture.getByRole("button", { name: "Create lead candidate" }).click();
    const created = await (await responsePromise).json();
    const candidateId = created.data.id as string;
    await expect(reviewerPage).toHaveURL(new RegExp(`/marketing/${candidateId}$`));
    await expect(reviewerPage.getByRole("heading", { name: "Demo prospect - Bangkok" })).toBeVisible();

    await signIn(salesContext, salesPage, credentials, "sales_manager", `/holdings/simulation/marketing/${candidateId}`);
    const engagementForm = salesPage.locator("#workspace-content:visible").locator("form").filter({ has: salesPage.getByRole("heading", { name: "Record engagement" }) });
    await engagementForm.getByLabel("Event").fill("Needs reviewed by Sales");
    await engagementForm.getByLabel("Channel").fill("email");
    await engagementForm.getByLabel("Evidence").fill("Sales manager confirmed scope for 20 sets and seven-day delivery.");
    responsePromise = salesPage.waitForResponse(response => response.request().method() === "POST" && new URL(response.url()).pathname.endsWith("/marketing"));
    await engagementForm.getByRole("button", { name: "Record" }).click();
    const engagementResponse = await responsePromise;
    expect(engagementResponse.status(), await engagementResponse.text()).toBe(200);
    await expect(salesPage.locator("#workspace-content:visible").getByText("Needs reviewed by Sales")).toBeVisible();

    const handoffForm = salesPage.locator("#workspace-content:visible").locator("form").filter({ has: salesPage.getByRole("heading", { name: "Independent CRM handoff" }) });
    await handoffForm.getByLabel("Due date").fill("2026-10-01");
    await handoffForm.getByLabel("Review evidence").fill("Consent, attribution and delivery brief reviewed for Sales follow-up.");
    responsePromise = salesPage.waitForResponse(response => response.request().method() === "POST" && new URL(response.url()).pathname.endsWith("/marketing"));
    await handoffForm.getByRole("button", { name: "Create CRM lead" }).click();
    const handoffResponse = await responsePromise;
    expect(handoffResponse.status(), await handoffResponse.text()).toBe(200);
    const handoff = (await handoffResponse.json()).data as { crmLeadId: string };
    const leadId = handoff.crmLeadId;
    const detailResponse = await salesContext.request.get(
      `/api/v1/holdings/simulation/crm?id=${encodeURIComponent(leadId)}`,
    );
    expect(detailResponse.ok(), await detailResponse.text()).toBe(true);
    expect((await detailResponse.json()).data.row).toMatchObject({
      contact_name: "Demo prospect - Bangkok",
      owner_id: marketer.userId,
      status: "open",
    });
    const activitiesResponse = await salesContext.request.get(
      `/api/v1/holdings/simulation/crm/activities?leadId=${encodeURIComponent(leadId)}`,
    );
    expect(activitiesResponse.ok(), await activitiesResponse.text()).toBe(true);
    await expect(salesPage.getByRole("link", { name: "Open CRM lead" })).toBeVisible();
    await salesPage.getByRole("link", { name: "Open CRM lead" }).click();
    const leadDrawer = salesPage.getByTestId("record-detail-drawer");
    await expect(leadDrawer).toHaveCount(1);
    await expect(leadDrawer.locator("[data-record-drawer-content]").getByRole("heading", { name: "Marketing lead · Demo prospect - Bangkok", exact: true })).toBeVisible();
    const leadFullPage = leadDrawer.getByRole("link", { name: "Open full page ↗", exact: true });
    await expect(leadFullPage).toHaveAttribute("href", `/holdings/simulation/crm/${leadId}`);
    await leadFullPage.click();
    await expect(salesPage).toHaveURL(new RegExp(`/crm/${leadId}$`));
    await expect(salesPage.getByRole("heading", { name: "Marketing lead · Demo prospect - Bangkok" })).toBeVisible();
    await expect(salesPage.locator("#workspace-content:visible").getByText(/Source: Marketing form .*attribution/)).toBeVisible();
    await expect(salesPage.locator("#workspace-content:visible").getByText("Needs 20 furniture sets within seven days; budget THB 100,000.", { exact: true })).toBeVisible();
    expect(new URL(salesPage.url()).pathname.split("/").at(-1)).toBe(leadId);
    await marketingPage.reload();
    await expect(marketingPage.getByRole("heading", { name: "Lead intake by campaign" })).toBeVisible();
    const campaignRow = marketingPage.locator("#workspace-content:visible")
      .locator("table.business-table tbody tr")
      .filter({ hasText: "Bangkok furniture launch" });
    await expect(campaignRow).toHaveCount(1);
    await expect(campaignRow.locator('xpath=ancestor::table[1]').getByRole('columnheader')).toHaveText(['Campaign','Source','Captured','Pending','CRM handoffs','Rejected','Details']);
    await expect(campaignRow.locator("td")).toHaveText([
      "Bangkok furniture launch",
      "Demo website inquiry",
      "1",
      "0",
      "1",
      "0",
      "View details",
    ]);
    const campaignLink=campaignRow.getByRole('cell').nth(0).getByRole('link',{name:'Bangkok furniture launch',exact:true});
    await expect(campaignLink).toHaveCount(1);
    const campaignHref=new URL((await campaignLink.getAttribute('href'))!,marketingPage.url());
    expect(campaignHref.pathname).toBe('/holdings/simulation/record-details/marketing_campaign_sources/Bangkok%20furniture%20launch');
    expect(campaignHref.searchParams.get('source')).toBe('Demo website inquiry');
    const detailsLink=campaignRow.getByRole('cell').nth(6).getByRole('link',{name:'View details',exact:true});
    await expect(detailsLink).toHaveCount(1);
    await expect(detailsLink).toHaveAttribute('href',(await campaignLink.getAttribute('href'))!);
  } finally {
    await marketingContext.close(); await reviewerContext.close(); await salesContext.close();
  }
});
