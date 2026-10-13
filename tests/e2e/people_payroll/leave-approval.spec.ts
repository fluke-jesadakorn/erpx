import { expect, test, type BrowserContext, type Page } from "@playwright/test";
import { prepareLegacyLeaveReviewer } from "../../support/helpers/legacy-leave-reviewer-browser";
import { selectFixtureAssignment } from "../../support/browser/select-fixture-assignment";

async function login(page: Page, email: string, password: string, assignmentId?: string) {
  await page.goto("/login");
  await page.getByLabel("Work email").fill(email);
  await page.getByLabel(/^Password(?:\s*\*)?$/).fill(password);
  await page.getByRole("button", { name: "Sign in" }).click();
  if (assignmentId) {
    await expect(page).toHaveURL(/\/holdings\/acme(?:\/branches\/[^/]+)?\/(?:home|work-role)(?:\?.*)?$/);
    await selectFixtureAssignment(page.context(), "acme-co", assignmentId);
    await page.goto("/holdings/acme/home");
  }
  await expect(page).toHaveURL(/\/holdings\/acme\/home$/);
}

type PersistedLeave = { id: string; state: string; version: number; created_by: string; approval: { currentApprover: { approver_user_id: string } | null } };
async function readLeave(context: BrowserContext, api: string): Promise<PersistedLeave> {
  const response = await context.request.get(api);
  expect(response.ok(), await response.text()).toBe(true);
  return (await response.json() as { data: PersistedLeave }).data;
}

test("independent users see the persisted leave result", async ({
  browser,
}, info) => {
  test.setTimeout(120000);
  await prepareLegacyLeaveReviewer(await browser.newContext(info.project.use));
  const employee = await browser.newContext(info.project.use);
  const manager = await browser.newContext(info.project.use);
  const hrContext = await browser.newContext(info.project.use);
  try {
    const mali = await employee.newPage();
    await login(mali, "mali@erpx.dev", "Mali-ERPX-2026!", "asg_mali_owner");
    await mali.goto(
      "/holdings/acme/branches/bangkok/tasks/people_payroll/leave_001",
    );
    await expect(
      mali.getByText("Personal leave request · LV-2026-001", { exact: true }),
    ).toBeVisible();
    const branchApi = "/api/v1/holdings/acme/branches/bangkok/leave-requests/leave_001";
    const before = await readLeave(employee, branchApi);
    expect(before).toMatchObject({ id: "leave_001", state: "draft", version: 1, created_by: "usr_mali" });
    await mali.getByRole("combobox", { name: "Leave type", exact: true }).selectOption({ label: "Annual leave" });
    await mali.getByRole("button", { name: "Review request", exact: true }).click();
    await expect(mali.getByRole("heading", { name: "Check the calculation", exact: true })).toBeVisible();
    const submittedResponse = mali.waitForResponse(response => new URL(response.url()).pathname === branchApi && response.request().method() === "PATCH");
    await mali.getByRole("button", { name: "Submit leave request", exact: true }).click();
    const submitted = await submittedResponse;
    expect(submitted.ok(), await submitted.text()).toBe(true);
    expect(submitted.request().postDataJSON()).toMatchObject({ expectedVersion: before.version, saveOnly: false });
    expect((await submitted.json()).data).toMatchObject({ id: before.id, state: "pending_approval", version: before.version + 1 });
    const pending = await readLeave(employee, branchApi);
    expect(pending).toMatchObject({ state: "pending_approval", version: before.version + 1, approval: { currentApprover: { approver_user_id: "usr_nisa" } } });
    await expect(mali.getByRole("button", { name: "Approve", exact: true })).not.toBeVisible();

    const nisa = await manager.newPage();
    await login(nisa, "nisa@erpx.dev", "Nisa-ERPX-2026!", "workflow_controller");
    await nisa.goto("/holdings/acme/tasks/people_payroll/leave_001");
    const managerApi = "/api/v1/holdings/acme/leave-requests/leave_001";
    expect(await readLeave(manager, managerApi)).toMatchObject({ state: "pending_approval", version: pending.version });
    await nisa.getByRole("button", { name: "Approve", exact: true }).click();
    const managerResponse = nisa.waitForResponse(response => new URL(response.url()).pathname === managerApi + "/approve" && response.request().method() === "POST");
    await nisa.getByRole("button", { name: "Confirm Approve", exact: true }).click();
    const reviewed = await managerResponse;
    expect(reviewed.ok(), await reviewed.text()).toBe(true);
    expect(reviewed.request().postDataJSON()).toMatchObject({ expectedVersion: pending.version });
    expect((await reviewed.json()).data).toMatchObject({ state: "pending_approval", version: pending.version + 1 });
    const hrPending = await readLeave(manager, managerApi);
    expect(hrPending).toMatchObject({ state: "pending_approval", version: pending.version + 1, approval: { currentApprover: { approver_user_id: "usr_ava" } } });

    await nisa.reload();
    await nisa.getByRole("tab", { name: "History", exact: true }).click();
    await expect(nisa.getByText("Current approver: Ava Williams", { exact: true })).toBeVisible();
    const ava = await hrContext.newPage();
    await ava.goto("/login");
    await ava.getByLabel("Work email").fill("ava@erpx.dev");
    await ava.getByLabel(/^Password(?:\s*\*)?$/).fill("Ava-ERPX-2026!");
    await ava.getByRole("button", { name: "Sign in", exact: true }).click();
    await expect(ava).toHaveURL(
      /\/holdings\/acme(?:\/branches\/bangkok)?\/home$/,
    );
    await ava.goto(
      "/holdings/acme/branches/bangkok/tasks/people_payroll/leave_001",
    );
    expect(await readLeave(hrContext, branchApi)).toMatchObject({ state: "pending_approval", version: hrPending.version });
    await ava.getByRole("button", { name: "Approve", exact: true }).click();
    const hrResponse = ava.waitForResponse(response => new URL(response.url()).pathname === branchApi + "/approve" && response.request().method() === "POST");
    await ava.getByRole("button", { name: "Confirm Approve", exact: true }).click();
    const approvedResponse = await hrResponse;
    expect(approvedResponse.ok(), await approvedResponse.text()).toBe(true);
    expect(approvedResponse.request().postDataJSON()).toMatchObject({ expectedVersion: hrPending.version });
    expect((await approvedResponse.json()).data).toMatchObject({ state: "approved", version: hrPending.version + 1 });
    await expect(ava.getByText("Your leave is approved.", { exact: true })).toBeVisible();
    await nisa.reload();
    await nisa.getByRole("tab", { name: "Overview", exact: true }).click();
    await expect(nisa.getByText("Your leave is approved.", { exact: true })).toBeVisible();
    await mali.reload();
    await expect(mali.getByRole("tabpanel", { name: "Overview", exact: true }).getByText("Your approved leave remains in place. You can request a cancellation review if plans change.", { exact: true })).toBeVisible();
    for (const [context, api] of [[employee, branchApi], [manager, managerApi], [hrContext, branchApi]] as const)
      expect(await readLeave(context, api)).toMatchObject({ state: "approved", version: hrPending.version + 1, approval: { currentApprover: null } });
  } finally {
    await employee.close().catch(() => {});
    await manager.close().catch(() => {});
    await hrContext.close().catch(() => {});
  }
});

test("a device can add accounts and each account sees its combined responsibilities", async ({
  browser,
}, info) => {
  test.setTimeout(90000);
  const context = await browser.newContext(info.project.use);
  const page = await context.newPage();
  await login(page, "mali@erpx.dev", "Mali-ERPX-2026!", "asg_mali_owner");
  await page.goto("/holdings/acme/branches/bangkok/home");

  await page.getByRole("button", { name: "Switch workspace" }).click();
  await expect(
    page
      .getByRole("dialog", { name: "Switch workspace", exact: true })
      .getByRole("heading", { name: "Acme Group", exact: true }),
  ).toBeVisible();
  await expect(
    page
      .getByRole("dialog", { name: "Switch workspace", exact: true })
      .getByRole("button", { name: /Acme Corporation/ }),
  ).toBeVisible();
  await page.keyboard.press("Escape");

  await page.locator(".account-trigger").click();
  await page.getByRole("button", { name: "Add another account" }).click();
  await page
    .getByRole("dialog", { name: "Account switcher" })
    .getByLabel("Work email")
    .fill("nisa@erpx.dev");
  await page
    .getByRole("dialog", { name: "Account switcher" })
    .getByLabel(/^Password(?:\s*\*)?$/)
    .fill("Nisa-ERPX-2026!");
  await page.getByRole("button", { name: "Add account", exact: true }).click();
  await expect(page.locator(".account-trigger")).toContainText("Nisa Chen");
  // Choose this newly signed-in account's existing work role after the real
  // account addition; do not prepare another account's session in advance.
  await selectFixtureAssignment(context, "acme-co", "workflow_controller");
  await page.goto("/holdings/acme/branches/bangkok/home");
  await expect(page.getByRole("heading", { name: "Today", exact: true })).toBeVisible();
  await expect(page.getByRole("button", { name: "Account menu for Nisa Chen, nisa@erpx.dev", exact: true })).toBeVisible();

  await page.getByRole("button", { name: "Switch workspace" }).click();
  await expect(
    page
      .getByRole("dialog", { name: "Switch workspace", exact: true })
      .getByRole("button", { name: /Acme Corporation/ }),
  ).toBeVisible();
  await expect(
    page
      .getByRole("dialog", { name: "Switch workspace", exact: true })
      .getByText("Northstar Supply", { exact: true }),
  ).toHaveCount(0);
  await page.keyboard.press("Escape");
  await page.locator(".account-trigger").click();
  await page
    .getByRole("button", { name: /Mali Srisai mali@erpx\.dev/ })
    .click();
  await expect(page.getByRole("heading", { name: "Today", exact: true })).toBeVisible({ timeout: 15000 });
  await expect(page.getByRole("button", { name: "Account menu for Mali Srisai, mali@erpx.dev", exact: true })).toBeVisible();
  await page.getByRole("button", { name: "Switch workspace" }).click();
  await expect(
    page
      .getByRole("dialog", { name: "Switch workspace", exact: true })
      .getByRole("button", { name: /Acme Corporation/ }),
  ).toBeVisible();
  await page.keyboard.press("Escape");

  // A new account from another Holding chooses its own authorized workspace;
  // its Company and responsibilities must not inherit the previous account's.
  await page.locator(".account-trigger").click();
  await page.getByRole("button", { name: "Add another account" }).click();
  await page
    .getByRole("dialog", { name: "Account switcher" })
    .getByLabel("Work email")
    .fill("sam@erpx.dev");
  await page
    .getByRole("dialog", { name: "Account switcher" })
    .getByLabel(/^Password(?:\s*\*)?$/)
    .fill("Sam-ERPX-2026!");
  await page.getByRole("button", { name: "Add account", exact: true }).click();
  const otherHolding = page.getByRole("dialog", { name: "Switch workspace", exact: true });
  await expect(otherHolding).toBeVisible();
  await expect(otherHolding).not.toContainText("Acme Corporation");
  await otherHolding.getByRole("button", { name: /Northstar Supply/ }).click();
  await expect(page).toHaveURL(/\/holdings\/northstar\/home$/, {
    timeout: 20000,
  });
  await expect(page.getByRole("heading", { name: "Today", exact: true })).toBeVisible();
  await expect(page.getByRole("button", { name: "Account menu for Sam Rivera, sam@erpx.dev", exact: true })).toBeVisible();
  const current = await context.request.get("/api/v1/holdings/northstar/assignments/active");
  expect(current.ok(), await current.text()).toBe(true);
  expect((await current.json()).data).toMatchObject({ valid: true, companyId: "northstar-co", userId: "usr_sam", assignmentId: "seed_usr_sam" });
  await context.close();
});
