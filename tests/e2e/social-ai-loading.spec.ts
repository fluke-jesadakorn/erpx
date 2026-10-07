import { expect, test, type BrowserContext } from "@playwright/test";
import { selectFixtureAssignment } from "./helpers/select-fixture-assignment";

const base = "/holdings/acme/branches/bangkok";

async function login(context: BrowserContext) {
  const response = await context.request.post("/api/auth/sign-in/email", {
    data: {
      email: "mali@erpx.dev",
      password: "Mali-ERPX-2026!",
    },
  });
  expect(response.status(), await response.text()).toBe(200);
  await selectFixtureAssignment(context, "acme-co", "seed_usr_mali");
}

test("shows a distinct loading state while AI prepares a task", async ({
  browser,
}, testInfo) => {
  const context = await browser.newContext(testInfo.project.use);
  try {
    await login(context);
    const page = await context.newPage();
    await page.goto(`${base}/home`);

    await page.getByRole("article").first().getByRole("button", {
      name: "Create task",
      exact: true,
    }).click();

    const dialog = page.getByRole("dialog");
    await expect(dialog).toBeVisible();

    let finishAiRequest!: () => void;
    const finishAi = new Promise<void>((resolve) => {
      finishAiRequest = resolve;
    });
    await page.route("**/posts/*/ai-drafts", async (route) => {
      await finishAi;
      await route.fulfill({
        status: 503,
        contentType: "application/json",
        body: JSON.stringify({
          error: { message: "AI temporarily unavailable. Continue manually." },
        }),
      });
    });

    await dialog.getByRole("button", {
      name: "Let AI prepare this task",
      exact: true,
    }).click();

    const preparingButton = dialog.getByRole("button", {
      name: "Preparing task…",
      exact: true,
    });
    await expect(preparingButton).toBeDisabled();
    await expect(preparingButton).toHaveAttribute("aria-busy", "true");

    finishAiRequest();
    await expect(dialog.getByRole("status")).toContainText(
      "AI temporarily unavailable",
    );
    await expect(dialog.getByRole("button", {
      name: "Let AI prepare this task",
      exact: true,
    })).toBeEnabled();
  } finally {
    await context.close();
  }
});
