import { expect, test } from "@playwright/test";
import { selectAcmeHoldingOwner } from "./helpers/select-acme-holding-owner";

test("purchase request and RFQ screens expose units and partial sourcing guidance", async ({ context, page }) => {
  const login = await context.request.post("/api/auth/sign-in/email", {
    data: {
      email: process.env.ERPX_E2E_ADMIN_EMAIL ?? "jesadakorn.kirtnu@gmail.com",
      password: process.env.ERPX_E2E_ADMIN_PASSWORD ?? "12345678",
    },
  });
  expect(login.ok()).toBe(true);
  await selectAcmeHoldingOwner(page);

  await page.goto("/holdings/acme/work?new=purchase_request");
  await expect(page).toHaveURL("/holdings/acme/requests/new?kind=purchase");
  const workspace = page.locator("#workspace-content:visible");
  await expect(workspace).toHaveCount(1);
  await expect(workspace.getByRole("heading", { name: "Purchase request", exact: true, level: 1 })).toBeVisible();
  await expect(workspace.getByLabel("Unit")).toHaveValue("each");

  await page.goto("/holdings/acme/procurement-rfqs");
  await expect(workspace.getByRole("heading", { name: "Supplier quotation requests" })).toBeVisible();
  await expect(workspace.getByText(/all or part of the remaining approved demand/i)).toBeVisible();
});
