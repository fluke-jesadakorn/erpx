import { expect, test } from "@playwright/test";

test("purchase request and RFQ screens expose units and partial sourcing guidance", async ({ context, page }) => {
  const login = await context.request.post("/api/auth/sign-in/email", {
    data: {
      email: process.env.ERPX_E2E_ADMIN_EMAIL ?? "jesadakorn.kirtnu@gmail.com",
      password: process.env.ERPX_E2E_ADMIN_PASSWORD ?? "12345678",
    },
  });
  expect(login.ok()).toBe(true);

  await page.goto("/holdings/acme/work?new=purchase_request");
  await expect(page.getByRole("heading", { name: "New work" })).toBeVisible();
  await expect(page.getByLabel("Unit")).toHaveValue("each");

  await page.goto("/holdings/acme/procurement-rfqs");
  await expect(page.getByRole("heading", { name: "Supplier quotation requests" })).toBeVisible();
  await expect(page.getByText(/all or part of the remaining approved demand/i)).toBeVisible();
});
