import { captureCaseScreenshot } from "../../support/helpers/case-screenshot";
import { test, expect, type BrowserContext, type Page } from "@playwright/test";
import { randomUUID } from "node:crypto";
import { selectFixtureAssignment } from "../../support/browser/select-fixture-assignment";
test.use({ actionTimeout: 10000 });
const base = "/holdings/acme",
  local = `${base}/branches/bangkok`;
const evidencePng = Buffer.from(
  "iVBORw0KGgoAAAANSUhEUgAAAAIAAAACCAYAAABytg0kAAAACXBIWXMAAAPoAAAD6AG1e1JrAAAAEUlEQVQImWOQaEj4D8IMMAYAQaQH3RNNL/AAAAAASUVORK5CYII=",
  "base64",
);
async function login(context: BrowserContext, who: string) {
  const response = await context.request.post("/api/auth/sign-in/email", {
    data: {
      email: `${who}@erpx.dev`,
      password: `${who[0].toUpperCase() + who.slice(1)}-ERPX-2026!`,
    },
  });
  expect(response.ok()).toBeTruthy();
  const assignments: Record<string, string> = { mali: "seed_usr_mali", nisa: "workflow_controller", ava: "seed_usr_ava", noah: "seed_usr_noah" };
  await selectFixtureAssignment(context, "acme-co", assignments[who], "acme", ["ava", "noah"].includes(who) ? (who === "ava" ? "/branches/bangkok" : "/branches/new-york") : "");
}
async function fits(page: Page) {
  expect(
    await page.evaluate(
      () => document.documentElement.scrollWidth <= window.innerWidth + 1,
    ),
  ).toBe(true);
}
test("staff project and task approvals work across roles, departments and viewports", async ({
  browser,
}, testInfo) => {
  test.setTimeout(240000);
  const creator = await browser.newContext(testInfo.project.use),
    assignee = await browser.newContext(testInfo.project.use),
    reviewer = await browser.newContext(testInfo.project.use),
    unrelated = await browser.newContext(testInfo.project.use);
  const name = `Staff workflow ${randomUUID().slice(0, 8)}`;
  try {
    await Promise.all([
      login(creator, "mali"),
      login(assignee, "nisa"),
      login(reviewer, "ava"),
      login(unrelated, "noah"),
    ]);
    const page = await creator.newPage();
    await page.goto(`${local}/work`);
    await page.getByRole("button", { name: "New", exact: true }).click();
    await expect(
      page
        .getByRole("dialog", { name: "New work" })
        .getByRole("link", { name: "Expense claim", exact: true }),
    ).toBeVisible();
    await expect(
      page
        .getByRole("dialog", { name: "New work" })
        .getByRole("link", { name: "Leave request", exact: true }),
    ).toBeVisible();
    await page.keyboard.press("Escape");
    const navigation = page.getByRole("navigation", { name: testInfo.project.name === "mobile" ? "Mobile navigation" : "Primary", exact: true });
    await navigation.getByRole("button", { name: "App launcher", exact: true }).click();
    await page.getByRole("dialog", { name: "App launcher", exact: true }).getByRole("link", { name: "Projects", exact: true }).click();
    await page.getByText("New project", { exact: true }).click();
    await page.getByLabel(/^Project title(?:\s*\*)?$/).fill(name);
    await page
      .getByRole("combobox", { name: "Add company member", exact: true })
      .selectOption("usr_nisa");
    await page.getByRole("button", { name: "Add member", exact: true }).click();
    await page
      .getByRole("button", { name: "Save project", exact: true })
      .click();
    await expect(
      page.getByRole("heading", { name, exact: true }),
    ).toBeVisible();
    const projectId = page.url().split("/").pop()!;
    await page
      .getByRole("link", { name: "Create a task", exact: true })
      .click();
    await expect(page.getByRole("textbox", { name: "Title", exact: true })).toBeVisible({
      timeout: 20000,
    });
    await page
      .getByRole("textbox", { name: "Title", exact: true })
      .fill(`${name} delivery checks`);
    await page
      .getByLabel("Description", { exact: true })
      .fill("Inspect the delivery logs and attach evidence.");
    await page
      .getByRole("combobox", { name: "Project", exact: true })
      .selectOption(projectId);
    await page
      .getByRole("combobox", { name: "Accountable assignee", exact: true })
      .selectOption("usr_nisa");
    await page
      .getByRole("combobox", { name: "Review", exact: true })
      .selectOption("both");
    await page
      .getByRole("group", { name: "Start approval", exact: true })
      .getByRole("combobox", { name: "Step 1 approver", exact: true })
      .selectOption("usr_ava");
    await page
      .getByRole("group", { name: "Completion approval", exact: true })
      .getByRole("combobox", { name: "Step 1 approver", exact: true })
      .selectOption("usr_mali");
    await fits(page);
    await page.evaluate(() => window.scrollTo(0, 0));
    await captureCaseScreenshot(page, `task-form-${testInfo.project.name}.png`, { fullPage: true });
    await page
      .getByRole("button", { name: "Save task draft", exact: true })
      .click();
    await expect(
      page.getByRole("heading", {
        name: `${name} delivery checks`,
        exact: true,
      }),
    ).toBeVisible();
    const taskId = page.url().split("/").pop()!;
    await page
      .getByRole("button", { name: "Submit task", exact: true })
      .click();
    await expect(
      page.getByText("Current approver: Ava Williams", { exact: true }),
    ).toBeVisible();
    const approve = await reviewer.newPage();
    await approve.goto(`${local}/staff-tasks/${taskId}`);
    await approve
      .getByRole("textbox", {
        name: "Completion or decision note",
        exact: true,
      })
      .fill("Start is approved.");
    await approve
      .getByRole("button", { name: "Approve current step", exact: true })
      .click();
    await expect(
      approve.getByRole("button", {
        name: "Approve current step",
        exact: true,
      }),
    ).toHaveCount(0);
    const execute = await assignee.newPage();
    await execute.goto(`${base}/staff-tasks/${taskId}`);
    await execute
      .getByRole("button", { name: "Start work", exact: true })
      .click();
    await expect(
      execute.getByRole("button", { name: "Submit completion", exact: true }),
    ).toBeVisible();
    const evidence = execute.getByRole("heading", { name: "Task evidence", exact: true }).locator("xpath=ancestor::section[1]");
    await evidence
      .locator('input[type="file"]:not([capture])')
      .setInputFiles({
        name: "delivery-checks.png",
        mimeType: "image/png",
        buffer: evidencePng,
      });
    await expect(evidence.getByText("Ready to attach", { exact: true })).toBeVisible({ timeout: 120000 });
    const download = evidence.getByRole("link", { name: "Download delivery-checks.png", exact: true });
    await expect(download).toBeVisible();
    const attachment = await download
      .getAttribute("href");
    const original = await assignee.request.get(attachment!);
    expect(original.status()).toBe(200);
    expect(original.headers()["cache-control"]).toBe("private, no-store");
    expect(original.headers()["content-disposition"]).toContain("attachment");
    expect([403, 404]).toContain(
      (await unrelated.request.get(attachment!)).status(),
    );
    expect([403, 404]).toContain(
      (
        await unrelated.request.get(`/api/v1${base}/staff-tasks/${taskId}`)
      ).status(),
    );

    await execute
      .getByRole("textbox", {
        name: "Completion or decision note",
        exact: true,
      })
      .fill("All delivery logs verified.");
    const executing = await assignee.request.get(`/api/v1${base}/staff-tasks/${taskId}`);
    expect(executing.ok(), await executing.text()).toBe(true);
    const executingVersion = (await executing.json()).data.task.version as number;
    const completion = execute.waitForResponse(response => new URL(response.url()).pathname === `/api/v1${base}/staff-tasks/${taskId}/transition` && response.request().method() === "POST");
    await execute
      .getByRole("button", { name: "Submit completion", exact: true })
      .click();
    const completedSubmission = await completion;
    expect(completedSubmission.ok(), await completedSubmission.text()).toBe(true);
    expect(completedSubmission.request().postDataJSON()).toMatchObject({ action: "complete", expectedVersion: executingVersion });
    expect(completedSubmission.request().postDataJSON().attachmentSetIds).toHaveLength(1);
    const pendingCompletion = await assignee.request.get(`/api/v1${base}/staff-tasks/${taskId}`);
    expect(pendingCompletion.ok(), await pendingCompletion.text()).toBe(true);
    expect((await pendingCompletion.json()).data.task).toMatchObject({ state: "completion_approval", version: executingVersion + 1 });
    await expect(
      execute.getByText("Current approver: Mali Srisai", { exact: true }),
    ).toBeVisible();
    await page.reload();
    await expect(
      page.getByRole("link", { name: "Download delivery-checks.png", exact: true }),
    ).toBeVisible();
    await page
      .getByRole("textbox", {
        name: "Completion or decision note",
        exact: true,
      })
      .fill("Completion accepted.");
    await page
      .getByRole("button", { name: "Approve current step", exact: true })
      .click();
    await expect(
      page.getByRole("button", { name: "Approve current step", exact: true }),
    ).toHaveCount(0);
    const state = await creator.request.get(
      `/api/v1${local}/staff-tasks/${taskId}`,
    );
    expect((await state.json()).data.task.state).toBe("completed");
    await expect(page.getByText("completed", { exact: true })).toBeVisible();
    await page.evaluate(() => window.scrollTo(0, 0));
    await fits(page);
    await captureCaseScreenshot(page, `task-complete-${testInfo.project.name}.png`, { fullPage: true });
    const wrongCompany = await assignee.request.get(
      `/api/v1/holdings/northstar/staff-tasks/${taskId}`,
    );
    expect([403, 404]).toContain(wrongCompany.status());
  } finally {
    await creator.close();
    await assignee.close();
    await reviewer.close();
    await unrelated.close();
  }
});
test("staff can publish routines, pause and resume schedules, and run an independent occurrence", async ({
  browser,
}, testInfo) => {
  test.setTimeout(90000);
  const context = await browser.newContext(testInfo.project.use);
  try {
    await login(context, "mali");
    const page = await context.newPage(),
      title = `Routine ${randomUUID().slice(0, 8)}`;
    await page.goto(`${local}/routine-templates`);
    await page
      .getByRole("combobox", { name: "Create as", exact: true })
      .selectOption("routine");
    await page.getByRole("textbox", { name: "Title", exact: true }).fill(title);
    await page
      .getByRole("combobox", { name: "Accountable assignee", exact: true })
      .selectOption("usr_nisa");
    await page
      .getByRole("combobox", { name: "Step 1 approver", exact: true })
      .selectOption("usr_mali");
    await page
      .getByRole("button", { name: "Save routine template", exact: true })
      .click();
    await expect(
      page.getByRole("heading", { name: title, exact: true }),
    ).toBeVisible();
    await page.getByText("Add schedule", { exact: true }).click();
    await page
      .getByRole("combobox", { name: "Frequency", exact: true })
      .selectOption("monthly");
    await page.getByLabel("Monthly day", { exact: true }).fill("31");
    await page
      .getByRole("button", { name: "Save paused schedule", exact: true })
      .click();
    await expect(
      page.getByRole("button", { name: "Resume schedule", exact: true }),
    ).toBeVisible();
    await page
      .getByRole("button", { name: "Resume schedule", exact: true })
      .click();
    await expect(
      page.getByRole("button", { name: "Pause schedule", exact: true }),
    ).toBeVisible();
    await page
      .getByRole("button", { name: "Pause schedule", exact: true })
      .click();
    await expect(
      page.getByRole("button", { name: "Resume schedule", exact: true }),
    ).toBeVisible();
    await page
      .getByRole("button", { name: "Create occurrence", exact: true })
      .click();
    await expect(page).toHaveURL(/staff-tasks\/task-/);
    await expect(
      page.getByRole("heading", { name: title, exact: true }),
    ).toBeVisible();
    await fits(page);
  } finally {
    await context.close();
  }
});
