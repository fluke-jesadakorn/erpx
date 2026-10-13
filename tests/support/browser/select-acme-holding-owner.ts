import { expect, type Page } from "@playwright/test";
import { openAccountMenu, matchingRoleOptions } from "../helpers/context-switching-browser";

/** Select the signed-in fixture owner's existing role and Company through the normal UI. */
export async function selectAcmeHoldingOwner(page: Page): Promise<void> {
  const base = "/holdings/acme";
  const assignmentId = "dev_usr_jesadakorn_acme-co_holding_owner";
  // This multi-Company account first chooses its existing role in a branch
  // whose lineage supplies Company, then explicitly opens the Company workspace.
  await page.goto(`${base}/branches/bangkok/home`);
  const currentRole = await page.context().request.get(`/api/v1${base}/branches/bangkok/assignments/active`);
  expect(currentRole.ok(), await currentRole.text()).toBe(true);
  const roleState = (await currentRole.json()).data as { revision: number; companyId: string; userId: string; options: { id: string }[] };
  expect(roleState).toMatchObject({ companyId: "acme-co", userId: "usr_jesadakorn" });
  expect(roleState.options.some(option => option.id === assignmentId)).toBe(true);
  await openAccountMenu(page);
  const roles = await matchingRoleOptions(page, "holding owner");
  const owner = roles.filter({ hasText: "Acme Corporation" });
  await expect(owner).toHaveCount(1);
  const roleResponse = page.waitForResponse(response => response.request().method() === "POST" && response.url().endsWith("/assignments/active"));
  await owner.click();
  const selectedRole = await roleResponse;
  expect(selectedRole.status(), "The existing Holding owner role is selected through its normal UI").toBe(200);
  expect(selectedRole.request().postDataJSON()).toMatchObject({ assignmentId, revision: roleState.revision });
  const ready = page.locator("[data-work-assignment]");
  await expect(ready).toHaveAttribute("data-work-company", "acme-co");
  await expect(ready).toHaveAttribute("data-work-assignment", assignmentId);
  await expect(page.getByRole("dialog", { name: "Account switcher", exact: true })).toBeHidden();
  await page.getByRole("button", { name: "Switch workspace", exact: true }).click();
  const companyDialog = page.getByRole("dialog", { name: "Switch workspace", exact: true });
  await expect(companyDialog).toBeVisible();
  const companyResponse = page.waitForResponse(response => response.request().method() === "POST" && response.url().endsWith("/companies/active"));
  await companyDialog.getByRole("button", { name: "Acme Corporation", exact: true }).click();
  const selectedCompany = await companyResponse;
  expect(selectedCompany.status(), "The authorized Company workspace opens through its normal UI").toBe(200);
  expect(selectedCompany.request().postDataJSON()).toEqual({ companyId: "acme-co" });
  await expect(page).toHaveURL(`${base}/home`);
  const persistedRole = await page.context().request.get(`/api/v1${base}/assignments/active`);
  expect(persistedRole.ok(), await persistedRole.text()).toBe(true);
  const persisted = (await persistedRole.json()).data as { valid: boolean; revision: number; companyId: string; userId: string; assignmentId: string };
  expect(persisted).toMatchObject({ valid: true, companyId: "acme-co", userId: "usr_jesadakorn", assignmentId });
  expect(persisted.revision).toBeGreaterThanOrEqual(roleState.revision);
  await expect(ready).toHaveAttribute("data-work-user", persisted.userId);
  await expect(ready).toHaveAttribute("data-work-revision", String(persisted.revision));
}
