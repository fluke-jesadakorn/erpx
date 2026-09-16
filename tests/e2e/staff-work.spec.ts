import { test, expect, type BrowserContext, type Page } from "@playwright/test";
import { randomUUID } from "node:crypto";
test.use({ actionTimeout: 10000 });
const base = "/holdings/acme",
  local = `${base}/branches/bangkok`;
async function login(context: BrowserContext, who: string) {
  const response = await context.request.post("/api/auth/sign-in/email", {
    data: {
      email: `${who}@erpx.dev`,
      password: `${who[0].toUpperCase() + who.slice(1)}-ERPX-2026!`,
    },
  });
  expect(response.ok()).toBeTruthy();
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
  test.setTimeout(120000);
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
    if (testInfo.project.name === "mobile")
      await page
        .getByRole("button", { name: "More navigation" })
        .filter({ visible: true })
        .click();
    await page.getByRole("link", { name: "Projects", exact: true }).click();
    await page.getByText("New project", { exact: true }).click();
    await page.getByLabel("Project title", { exact: true }).fill(name);
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
    await expect(page.getByLabel("Title", { exact: true })).toBeVisible({
      timeout: 20000,
    });
    await page
      .getByLabel("Title", { exact: true })
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
      .getByRole("combobox", { name: "Approval timing", exact: true })
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
    await page.screenshot({
      path: `.scratch/staff-work/task-form-${testInfo.project.name}.png`,
      fullPage: true,
    });
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
    await execute
      .getByLabel("Attach supporting file", { exact: true })
      .setInputFiles({
        name: "delivery-checks.csv",
        mimeType: "text/csv",
        buffer: Buffer.from("delivery,status\nD1,verified\n"),
      });
    await expect(
      execute.getByRole("link", { name: /delivery-checks.csv/ }),
    ).toBeVisible();
    const attachment = await execute
      .getByRole("link", { name: /delivery-checks.csv/ })
      .getAttribute("href");
    expect((await assignee.request.get(attachment!)).status()).toBe(200);
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
    await execute
      .getByRole("button", { name: "Submit completion", exact: true })
      .click();
    await expect(
      execute.getByText("Current approver: Mali Srisai", { exact: true }),
    ).toBeVisible();
    await page.reload();
    await expect(
      page.getByRole("link", { name: /delivery-checks.csv/ }),
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
    await page.screenshot({
      path: `.scratch/staff-work/task-complete-${testInfo.project.name}.png`,
      fullPage: true,
    });
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
    await page.getByLabel("Title", { exact: true }).fill(title);
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
