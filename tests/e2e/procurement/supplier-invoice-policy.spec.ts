import { expect, test } from "@playwright/test";
import { selectAcmeHoldingOwner } from "../../support/browser/select-acme-holding-owner";

test("company Finance authority can require supplier matching and restore the policy with immutable history", async ({ context, page }) => {
  const login = await context.request.post("/api/auth/sign-in/email", {
    data: {
      email: process.env.ERPX_E2E_ADMIN_EMAIL ?? "jesadakorn.kirtnu@gmail.com",
      password: process.env.ERPX_E2E_ADMIN_PASSWORD ?? "12345678",
    },
  });
  expect(login.ok()).toBe(true);
  await selectAcmeHoldingOwner(page);

  await page.goto("/holdings/acme/supplier-bills");
  const workspace = page.locator("#workspace-content:visible");
  await expect(workspace).toHaveCount(1);
  await expect(workspace.getByRole("heading", { name: "Supplier invoice reviews" })).toBeVisible();
  const policy = workspace.locator('section[data-slot="card"]').filter({ has: page.getByRole("heading", { name: "Supplier invoice matching policy", exact: true }) });
  await expect(policy).toHaveCount(1);
  await expect(policy.getByText(/Current policy:/)).toBeVisible();

  async function changePolicy(mode: "optional" | "required", reason: string) {
    await policy.getByLabel("Required control").selectOption(mode);
    await policy.getByLabel("Policy change reason").fill(reason);
    await policy.getByRole("button", { name: "Save company policy" }).click();
    await expect(policy.getByText(new RegExp(`Current policy:\\s*${mode}`))).toBeVisible();
  }

  if (await policy.getByText(/Current policy:\s*required/).count())
    await changePolicy("optional", "Browser acceptance precondition restores direct AP before exercising the required control");

  const suffix = Date.now();
  const requiredReason = `Browser acceptance ${suffix}: require independent matching`;
  const optionalReason = `Browser acceptance ${suffix}: restore direct AP after verified control`;
  await changePolicy("required", requiredReason);
  await expect(policy).toContainText("Every purchase receipt must pass independent quantity and price matching");
  await changePolicy("optional", optionalReason);
  await policy.getByText("Policy change history").click();
  await expect(policy).toContainText(requiredReason);
  await expect(policy).toContainText(optionalReason);
});
