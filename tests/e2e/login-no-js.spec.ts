import { expect, test } from "@playwright/test";

test("sign-in cannot put credentials in a URL before hydration", async ({ browser, baseURL }) => {
  const context = await browser.newContext({ baseURL, javaScriptEnabled: false });
  try {
    const page = await context.newPage();
    await page.goto("/login");
    await page.getByLabel("Work email").fill("nojs@example.invalid");
    await page.getByLabel(/^Password(?:\s*\*)?$/).fill("Never-logged-in-123");
    await expect(page.getByRole("button", { name: "Sign in", exact: true })).toBeDisabled();
    await page.getByLabel(/^Password(?:\s*\*)?$/).press("Enter");
    await expect(page).not.toHaveURL(/email=|password=/i);
  } finally {
    await context.close();
  }
});
