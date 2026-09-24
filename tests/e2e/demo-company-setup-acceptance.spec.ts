import { expect, test, type BrowserContext, type Page } from "@playwright/test";
import { readFile } from "node:fs/promises";
import { resolve } from "node:path";

type User = { email: string; role: string; roles?: string[]; userId: string };
type Credentials = { password: string; users: User[] };
const origin = "http://127.0.0.1:3102";
const path = "/holdings/simulation/settings/business-readiness";

async function signIn(context: BrowserContext, page: Page, credentials: Credentials, role: string) {
  const user = credentials.users.find(item => item.role === role || item.roles?.includes(role));
  expect(user, `${role} Demo account`).toBeTruthy();
  const login = await context.request.post("/api/auth/sign-in/email", {
    headers: { Origin: origin }, data: { email: user!.email, password: credentials.password },
  });
  expect(login.ok(), await login.text()).toBe(true);
  const endpoint = "/api/v1/holdings/simulation/assignments/active";
  const active = (await (await context.request.get(endpoint)).json()).data;
  const assignment = active.options.find((item: { role: string }) => item.role === role);
  expect(assignment, `${role} assignment`).toBeTruthy();
  const selected = await context.request.post(endpoint, {
    headers: { Origin: origin }, data: { assignmentId: assignment.id, revision: active.revision },
  });
  expect(selected.ok(), await selected.text()).toBe(true);
  await page.goto(path);
  await expect(page.locator("[data-work-stale]")).toHaveAttribute("data-work-stale", "false");
  return user!;
}

test("Company setup is submitted, independently returned and accepted, then revoked through the real workspace", async ({ browser }, testInfo) => {
  const run = process.env.ERPX_DEMO_RUN;
  test.skip(!run, "Set ERPX_DEMO_RUN to an exact prepared browser-simulation run.");
  test.setTimeout(120_000);
  const credentials = JSON.parse(await readFile(resolve(`.runtime/browser/${run}/credentials.json`), "utf8")) as Credentials;
  const ownerContext = await browser.newContext(testInfo.project.use);
  const reviewerContext = await browser.newContext(testInfo.project.use);
  try {
    const ownerPage = await ownerContext.newPage(), reviewerPage = await reviewerContext.newPage();
    const owner = await signIn(ownerContext, ownerPage, credentials, "holding_owner");
    const reviewer = await signIn(reviewerContext, reviewerPage, credentials, "hr_admin");
    expect(reviewer.userId).not.toBe(owner.userId);
    const acceptance = (page: Page) => page.getByRole("region", { name: "Company setup acceptance" });
    const status = (page: Page) => acceptance(page).locator("header .badge");
    const reason = (page: Page) => acceptance(page).getByRole("textbox", { name: /reason/i });
    async function act(page: Page, button: string, suffix: string, expectedStatus: string, version: number) {
      const responsePromise = page.waitForResponse(response => response.request().method() === "POST" && new URL(response.url()).pathname.endsWith(`/people/organization/acceptance/${suffix}`));
      await acceptance(page).getByRole("button", { name: button, exact: true }).click();
      const response = await responsePromise;
      expect(response.status(), await response.text()).toBe(200);
      expect((await response.json()).data).toMatchObject({ status: expectedStatus, version });
      await expect(status(page)).toContainText(expectedStatus === "submitted" ? "Awaiting independent review" : expectedStatus === "rejected" ? "Returned for correction" : expectedStatus === "accepted" ? "Accepted" : "Revoked");
    }

    await expect(status(ownerPage)).toContainText("Not submitted");
    await expect(acceptance(ownerPage).getByText("All active staff have a level and position.")).toBeVisible();
    await reason(ownerPage).fill("Demo Company structure and active staff placement reviewed.");
    await act(ownerPage, "Submit for independent review", "submit", "submitted", 1);
    await expect(acceptance(ownerPage).getByRole("button", { name: "Accept Company setup" })).toHaveCount(0);

    await reviewerPage.goto("/holdings/simulation/work?type=company_business_acceptance&view=actions");
    await reviewerPage.getByRole("link", { name: /Company setup acceptance/ }).first().click();
    await expect(reviewerPage).toHaveURL(/business-readiness(?:\?[^#]*)?#company-setup-acceptance/);
    await reason(reviewerPage).fill("Attach final staff placement review evidence.");
    await act(reviewerPage, "Return for correction", "review", "rejected", 2);

    await ownerPage.goto("/holdings/simulation/inbox?tab=updates");
    await ownerPage.getByRole("link", { name: /Company setup acceptance was returned for correction/ }).first().click();
    await expect(ownerPage).toHaveURL(/business-readiness(?:\?[^#]*)?#company-setup-acceptance/);
    await reason(ownerPage).fill("Final active staff placement evidence checked after correction.");
    await act(ownerPage, "Submit for independent review", "submit", "submitted", 3);

    await reviewerPage.reload();
    await reason(reviewerPage).fill("Company identity, branch, levels, CEO and staff placement independently verified.");
    await act(reviewerPage, "Accept Company setup", "review", "accepted", 4);
    await ownerPage.reload();
    await expect(status(ownerPage)).toContainText("Accepted");
    await reason(ownerPage).fill("Reopen review after this isolated Company setup rehearsal.");
    await act(ownerPage, "Revoke acceptance", "revoke", "revoked", 5);
    await ownerPage.reload();
    await acceptance(ownerPage).getByText("Immutable acceptance history").click();
    await expect(acceptance(ownerPage).locator("details ol li")).toHaveCount(5);
    await expect(acceptance(ownerPage).locator("details ol")).toContainText("v5 · revoked");
  } finally {
    await ownerContext.close();
    await reviewerContext.close();
  }
});
