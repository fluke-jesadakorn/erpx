import {createEngineeringBrowserContext} from "../../support/browser/engineering-browser-evidence";
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

test("isolated demo delegates task work, revokes access and independently reviews completion", async ({ browser }, testInfo) => {
  const run = process.env.ERPX_DEMO_RUN;
  test.skip(!run, "Set ERPX_DEMO_RUN to an exact prepared browser-simulation run.");
  test.setTimeout(120_000);
  const credentials = JSON.parse(await readFile(resolve(`.runtime/browser/${run}/credentials.json`), "utf8")) as Credentials;
  const member = (role: string) => {
    const user = credentials.users.find(item => item.roles?.includes(role) || item.role === role);
    expect(user, `${role} account`).toBeTruthy();
    return user!;
  };
  const owner = member("holding_owner"), assignee = member("employee"), delegate = member("people_manager"), reviewer = member("finance_approver");
  expect(new Set([owner.userId, assignee.userId, delegate.userId, reviewer.userId]).size).toBe(4);
  const contexts = await Promise.all(Array.from({ length: 4 }, () => createEngineeringBrowserContext(browser,testInfo)));
  const [ownerContext, assigneeContext, delegateContext, reviewerContext] = contexts;
  contexts.forEach(context => context.setDefaultTimeout(15_000));
  const main = (page: Page) => page.locator("#workspace-content");
  async function command(page: Page, label: string, endpoint: string) {
    const pending = page.waitForResponse(response => response.request().method() === "POST" && new URL(response.url()).pathname === endpoint);
    await main(page).getByRole("button", { name: label, exact: true }).click();
    const response = await pending;
    expect(response.status(), await response.text()).toBe(200);
    return (await response.json()).data;
  }
  try {
    const ownerPage = await ownerContext.newPage();
    await signInRole(ownerContext, ownerPage, credentials, "holding_owner", "/holdings/simulation/work?new=task");
    const title = `Receiving checklist delegation ${Date.now()}`;
    await main(ownerPage).getByLabel("Title", { exact: true }).fill(title);
    await main(ownerPage).getByLabel("Description", { exact: true }).fill("Verify the received equipment while the accountable colleague is away.");
    await main(ownerPage).getByRole("combobox", { name: "Accountable assignee", exact: true }).selectOption(assignee.userId);
    await main(ownerPage).getByRole("combobox", { name: "Review", exact: true }).selectOption("completion");
    await main(ownerPage).getByRole("group", { name: "Completion approval", exact: true }).getByRole("combobox", { name: "Step 1 approver", exact: true }).selectOption(reviewer.userId);
    const created = await command(ownerPage, "Save task draft", "/api/v1/holdings/simulation/staff-tasks");
    const id = created.id as string;
    const path = `/holdings/simulation/staff-tasks/${id}`;
    const endpoint = `/api/v1/holdings/simulation/staff-tasks/${id}`;
    await expect(main(ownerPage).getByRole("heading", { name: title, exact: true })).toBeVisible();
    await command(ownerPage, "Submit task", `${endpoint}/transition`);
    await expect(main(ownerPage).getByText("ready", { exact: true }).first()).toBeVisible();
    const coverage = main(ownerPage).locator("section").filter({ has: ownerPage.getByRole("heading", { name: "Temporary delegation", exact: true }) });
    await coverage.getByRole("combobox", { name: "Delegate to", exact: true }).selectOption(delegate.userId);
    await coverage.getByLabel("Reason", { exact: true }).fill("Receiving colleague is away for one day; temporary coverage is required.");
    await command(ownerPage, "Delegate task", `${endpoint}/delegate`);
    await expect(coverage.getByText(/This task is delegated to/)).toContainText(delegate.label);
    await ownerPage.reload();
    await expect(coverage.getByText(/This task is delegated to/)).toContainText(delegate.label);

    const delegatePage = await delegateContext.newPage();
    await signInRole(delegateContext, delegatePage, credentials, "people_manager", path);
    await command(delegatePage, "Start work", `${endpoint}/transition`);
    await expect(main(delegatePage).getByRole("button", { name: "Save progress", exact: true })).toBeVisible();
    await main(delegatePage).getByLabel("Completion or decision note", { exact: true }).fill("Checked the first six laptops and their serial numbers.");
    await command(delegatePage, "Save progress", `${endpoint}/transition`);

    await ownerPage.reload();
    const detail = (await (await ownerContext.request.get(endpoint)).json()).data;
    const revokeReason = "The accountable colleague returned and will verify the remaining four laptops.";
    await coverage.getByLabel("Reason for revoking", { exact: true }).fill(revokeReason);
    await command(ownerPage, "Revoke delegation", `${endpoint}/delegate/revoke/${detail.delegation.id}`);
    await expect(coverage.getByRole("button", { name: "Delegate task", exact: true })).toBeVisible();
    await coverage.getByText("Delegation history", { exact: true }).click();
    await expect(coverage.locator("details")).toContainText(revokeReason);
    await ownerPage.reload();
    await coverage.getByText("Delegation history", { exact: true }).click();
    await expect(coverage.locator("details")).toContainText(revokeReason);
    const revokedRead = await delegateContext.request.get(endpoint);
    expect(revokedRead.status()).toBe(404);

    const assigneePage = await assigneeContext.newPage();
    await signInRole(assigneeContext, assigneePage, credentials, "employee", path);
    await expect(main(assigneePage).getByLabel("Completion or decision note", { exact: true })).toHaveValue("Checked the first six laptops and their serial numbers.");
    await main(assigneePage).getByLabel("Completion or decision note", { exact: true }).fill("Reviewed all ten laptops and accepted the temporary colleague's evidence.");
    await command(assigneePage, "Submit completion", `${endpoint}/transition`);
    const reviewerPage = await reviewerContext.newPage();
    await signInRole(reviewerContext, reviewerPage, credentials, "finance_approver", path);
    await main(reviewerPage).getByLabel("Completion or decision note", { exact: true }).fill("Independently checked the equipment evidence and completed checklist.");
    await command(reviewerPage, "Approve current step", `${endpoint}/transition`);
    await expect(main(reviewerPage).getByText("completed", { exact: true }).first()).toBeVisible();
    await assigneePage.reload();
    await expect(main(assigneePage).getByText("completed", { exact: true }).first()).toBeVisible();
    await main(assigneePage).getByText("Delegation history", { exact: true }).click();
    await expect(main(assigneePage).locator("details").filter({ has: assigneePage.getByText("Delegation history", { exact: true }) })).toContainText(revokeReason);
    await testInfo.attach("task-delegation-result", { contentType: "application/json", body: JSON.stringify({ run, id, delegatedTo: delegate.userId, accountableAssignee: assignee.userId, reviewer: reviewer.userId, revokedReadStatus: revokedRead.status(), finalStatus: "completed" }, null, 2) });
    await assigneePage.screenshot({ path: testInfo.outputPath("task-delegation.png"), fullPage: true });
  } finally {
    await Promise.allSettled(contexts.map(context => context.close()));
  }
});

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
