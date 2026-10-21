import { expect, test } from "@playwright/test";

test("scrubs the invitation fragment and presents email-pinned enrollment and acceptance", async ({ page, baseURL }) => {
  if (!baseURL) throw new Error("BROWSER_BASE_URL_REQUIRED");
  const invitationToken = "browser-only-tenant-invitation-token";
  const enrollmentPath = "/customer-portal/enroll";
  const enrollmentRequests: string[] = [];
  const mutationRequests: string[] = [];
  const applicationOrigin = new URL(baseURL).origin;

  page.on("request", request => {
    const url = new URL(request.url());
    const method = request.method().toUpperCase();
    if (url.pathname === "/api/customer-portal/enroll") enrollmentRequests.push(method);
    if (url.origin === applicationOrigin && url.pathname.startsWith("/api/") && !["GET", "HEAD", "OPTIONS"].includes(method)) {
      mutationRequests.push(`${method} ${url.pathname}`);
    }
  });

  await page.goto(`${enrollmentPath}#token=${encodeURIComponent(invitationToken)}`, { waitUntil: "domcontentloaded" });
  await expect(page.getByRole("heading", { name: "Accept your Customer Portal invitation", exact: true })).toBeVisible();
  await expect(page).toHaveURL(new URL(enrollmentPath, baseURL).href);

  const renderedText = await page.locator("body").innerText();
  expect(renderedText).not.toContain(invitationToken);
  await expect(page.getByRole("heading", { name: "Create the invited account", exact: true })).toBeVisible();
  await expect(page.getByText(
    "The account email is fixed to the address that received this invitation. After email verification, reopen the invitation and sign in before accepting.",
    { exact: true },
  )).toBeVisible();
  await expect(page.getByRole("heading", { name: "Already have an account?", exact: true })).toBeVisible();
  await expect(page.getByText(
    "Sign in with the invited email address, then reopen the complete link in your invitation email.",
    { exact: true },
  )).toBeVisible();
  await expect(page.getByRole("link", { name: "Sign in to ERPX Account", exact: true })).toHaveAttribute("href", "/login");
  await expect(page.getByRole("button", { name: "Accept while signed in", exact: true })).toBeVisible();

  expect(enrollmentRequests).toEqual([]);
  expect(mutationRequests).toEqual([]);
});

test("offers verification resend when the invited account was created but email delivery failed", async ({ page, baseURL }) => {
  if (!baseURL) throw new Error("BROWSER_BASE_URL_REQUIRED");
  const invitationToken = "browser-only-tenant-invitation-token";
  let signupBody: Record<string, unknown> | undefined;
  await page.route("**/api/customer-portal/enroll", async route => {
    signupBody = route.request().postDataJSON() as Record<string, unknown>;
    await route.fulfill({
      status: 503,
      contentType: "application/json",
      body: JSON.stringify({ error: {
        code: "VERIFICATION_EMAIL_DELIVERY_FAILED",
        message: "Your account was created, but its verification email could not be sent. Use Resend verification email for the invited address.",
      } }),
    });
  });

  await page.goto(`/customer-portal/enroll#token=${encodeURIComponent(invitationToken)}`, { waitUntil: "domcontentloaded" });
  await page.getByRole("textbox", { name: "Your name" }).fill("Invited customer");
  await page.locator('input[name="password"]').fill("Browser-only-Portal-2026!");
  await page.getByRole("button", { name: "Create invited account", exact: true }).click();

  await expect(page.getByRole("status")).toContainText("Your account was created, but its verification email could not be sent.");
  await expect(page.getByRole("link", { name: "Resend verification email", exact: true })).toHaveAttribute("href", "/verify-email");
  expect(signupBody).toMatchObject({ action: "signup", token: invitationToken, name: "Invited customer" });
  expect(signupBody).not.toHaveProperty("email");
  await expect(page).toHaveURL(new URL("/customer-portal/enroll", baseURL).href);
  expect(await page.locator("body").innerText()).not.toContain(invitationToken);
});
