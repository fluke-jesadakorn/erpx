import { expect, test, type Page } from "@playwright/test";

async function login(page: Page, email: string, password: string) {
  await page.goto("/login");
  await page.getByLabel("Work email").fill(email);
  await page.getByLabel("Password").fill(password);
  await page.getByRole("button", { name: "Sign in" }).click();
  await expect(page).toHaveURL(/\/holdings\/acme\/home$/);
}

test("independent users see the persisted leave result", async ({
  browser,
}) => {
  const employee = await browser.newContext();
  const mali = await employee.newPage();
  await login(mali, "mali@erpx.dev", "Mali-ERPX-2026!");
  await mali.goto(
    "/holdings/acme/branches/bangkok/tasks/people_payroll/leave_001",
  );
  await expect(
    mali.getByRole("heading", { name: "LV-2026-001" }),
  ).toBeVisible();

  const submit = mali.getByRole("button", { name: "Review request" });
  if (await submit.isVisible()) {
    await submit.click();
    await mali
      .getByRole("button", { name: "Submit leave request", exact: true })
      .click();
    await expect(
      mali.getByText(
        "Request submitted. It’s ready for an independent review.",
      ),
    ).toBeVisible();
  }

  const manager = await browser.newContext();
  const nisa = await manager.newPage();
  await login(nisa, "nisa@erpx.dev", "Nisa-ERPX-2026!");
  await nisa.goto("/holdings/acme/tasks/people_payroll/leave_001");
  const approve = nisa.getByRole("button", { name: "Review approval" });
  if (await approve.isVisible()) {
    await approve.click();
    await nisa
      .getByRole("button", { name: "Approve leave", exact: true })
      .click();
    await expect(
      nisa.getByText("Approval recorded. The current stage is shown above."),
    ).toBeVisible();
  }

  await nisa.reload();
  const hrContext = await browser.newContext();
  const ava = await hrContext.newPage();
  await ava.goto("/login");
  await ava.getByLabel("Work email").fill("ava@erpx.dev");
  await ava.getByLabel("Password", { exact: true }).fill("Ava-ERPX-2026!");
  await ava.getByRole("button", { name: "Sign in", exact: true }).click();
  await expect(ava).toHaveURL(
    /\/holdings\/acme(?:\/branches\/bangkok)?\/home$/,
  );
  await ava.goto(
    "/holdings/acme/branches/bangkok/tasks/people_payroll/leave_001",
  );
  const hrApproval = ava.getByRole("button", {
    name: "Review approval",
    exact: true,
  });
  if (await hrApproval.isVisible()) {
    await hrApproval.click();
    await ava
      .getByRole("button", { name: "Approve leave", exact: true })
      .click();
    await expect(ava.getByText("This request is approved.")).toBeVisible();
  }
  await hrContext.close();
  await nisa.reload();
  await expect(nisa.getByText("This request is approved.")).toBeVisible();
  await employee.close();
  await manager.close();
});

test("a device can add accounts and each account sees its combined responsibilities", async ({
  browser,
}) => {
  test.setTimeout(90000);
  const context = await browser.newContext();
  const page = await context.newPage();
  await login(page, "mali@erpx.dev", "Mali-ERPX-2026!");
  await page.goto("/holdings/acme/branches/bangkok/home");

  await page.getByRole("button", { name: "Workspace menu" }).click();
  await expect(
    page
      .getByRole("dialog", { name: "Workspace", exact: true })
      .getByText(/holding owner/i, { exact: true }),
  ).toBeVisible();
  await expect(
    page
      .getByRole("dialog", { name: "Workspace", exact: true })
      .getByRole("link", { name: "All responsibilities", exact: true }),
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
    .getByLabel("Password")
    .fill("Nisa-ERPX-2026!");
  await page.getByRole("button", { name: "Add account", exact: true }).click();
  await expect(page.locator(".account-trigger")).toContainText("Nisa Chen");
  await expect(
    page.getByRole("heading", { name: "Hello, Nisa" }),
  ).toBeVisible();

  await page.getByRole("button", { name: "Workspace menu" }).click();
  await expect(
    page
      .getByRole("dialog", { name: "Workspace", exact: true })
      .getByText(/finance approver/i, { exact: true }),
  ).toBeVisible();
  await expect(
    page
      .getByRole("dialog", { name: "Workspace", exact: true })
      .getByText(/holding owner/i, { exact: true }),
  ).toHaveCount(0);
  await page.keyboard.press("Escape");
  await page.locator(".account-trigger").click();
  await page
    .getByRole("button", { name: /Mali Srisai mali@erpx\.dev/ })
    .click();
  await expect(page.getByRole("heading", { name: "Hello, Mali" })).toBeVisible({
    timeout: 15000,
  });
  await page.getByRole("button", { name: "Workspace menu" }).click();
  await expect(
    page
      .getByRole("dialog", { name: "Workspace", exact: true })
      .getByText(/holding owner/i, { exact: true }),
  ).toBeVisible();
  await page.keyboard.press("Escape");

  // Accounts can be authorized in another Holding. Adding one must resolve
  // to that account's first workspace instead of reloading Acme into a 404.
  await page.locator(".account-trigger").click();
  await page.getByRole("button", { name: "Add another account" }).click();
  await page
    .getByRole("dialog", { name: "Account switcher" })
    .getByLabel("Work email")
    .fill("sam@erpx.dev");
  await page
    .getByRole("dialog", { name: "Account switcher" })
    .getByLabel("Password")
    .fill("Sam-ERPX-2026!");
  await page.getByRole("button", { name: "Add account", exact: true }).click();
  await expect(page).toHaveURL(/\/holdings\/northstar\/home$/, {
    timeout: 20000,
  });
  await expect(page.getByRole("heading", { name: "Hello, Sam" })).toBeVisible();
  await context.close();
});
