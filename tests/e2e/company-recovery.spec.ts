import { test, expect } from "@playwright/test";
import { selectFixtureAssignment } from "./helpers/select-fixture-assignment";

test("stale company preference recovers holding access without allowing unauthorized company selection", async ({ page, context, baseURL }) => {
  await page.goto("/login");
  await page.getByLabel("Work email").fill("mali@erpx.dev");
  await page.getByLabel(/^Password(?:\s*\*)?$/).fill("Mali-ERPX-2026!");
  await page.getByRole("button", { name: "Sign in", exact: true }).click();
  await expect(page).toHaveURL(/\/holdings\/acme(?:\/branches\/bangkok)?\/home$/);
  await selectFixtureAssignment(context, "acme-co", "asg_mali_owner");
  await page.goto("/holdings/acme/home");
  await expect(page.getByRole("heading", { name: "Today", exact: true })).toBeVisible();

  // Beacon is a real active Company outside Mali's assignments. The normal
  // recovery shell must offer only the account's authorized workspaces.
  await context.addCookies([{ name: "erpx_company_h001", value: "beacon-co", url: baseURL! }]);
  await page.goto("/holdings/acme/home");
  await expect(page.getByRole("heading", { name: "Your role cannot open this view", exact: true })).toBeVisible();
  await page.getByRole("button", { name: "Switch workspace", exact: true }).click();
  const recovery = page.getByRole("dialog", { name: "Switch workspace", exact: true });
  await expect(recovery).not.toContainText("Beacon Labs");
  await recovery.getByRole("button", { name: /Acme Corporation/ }).click();
  await expect(page).toHaveURL(/\/holdings\/acme\/home$/);
  await expect(page.getByRole("heading", { name: "Today", exact: true })).toBeVisible({ timeout: 30000 });
  await expect(page.getByRole("button", { name: "Account menu for Mali Srisai, mali@erpx.dev", exact: true })).toBeVisible();
  const active = await context.request.get("/api/v1/holdings/acme/assignments/active");
  expect(active.ok(), await active.text()).toBe(true);
  const restored = (await active.json()).data;
  expect(restored).toMatchObject({ valid: true, companyId: "acme-co", userId: "usr_mali", assignmentId: "asg_mali_owner" });

  const denied = await context.request.post("/api/v1/holdings/acme/companies/active", {
    headers: { Origin: new URL(baseURL!).origin },
    data: { companyId: "beacon-co" },
  });
  expect(denied.status()).toBe(403);
  expect(await denied.json()).toMatchObject({ error: { code: "COMPANY_UNAVAILABLE" } });
  const unchanged = await context.request.get("/api/v1/holdings/acme/assignments/active");
  expect(unchanged.ok(), await unchanged.text()).toBe(true);
  expect((await unchanged.json()).data).toMatchObject({ valid: true, companyId: "acme-co", userId: "usr_mali", assignmentId: "asg_mali_owner", revision: restored.revision });
  await page.getByRole("button", { name: "Switch workspace", exact: true }).click();
  const choices = page.getByRole("dialog", { name: "Switch workspace", exact: true });
  await expect(choices).not.toContainText("Beacon Labs");
  await expect(choices).not.toContainText("Expense test");
});
