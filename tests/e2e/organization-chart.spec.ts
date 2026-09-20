import { test, expect, type BrowserContext, type Page } from "@playwright/test";
import type { OrganizationOverview } from "../../src/lib/organization";
import { buildOrganizationGraph } from "../../src/lib/organization-chart";

const base = "/holdings/acme";
let signedIn: Awaited<ReturnType<BrowserContext["storageState"]>>;

test.beforeAll(async ({ browser, baseURL }) => {
  const context = await browser.newContext({ baseURL });
  const response = await context.request.post("/api/auth/sign-in/email", { data: { email: process.env.ERPX_E2E_ADMIN_EMAIL ?? "jesadakorn.kirtnu@gmail.com", password: process.env.ERPX_E2E_ADMIN_PASSWORD ?? "12345678" } });
  expect(response.ok()).toBe(true);
  signedIn = await context.storageState();
  const organization = await context.request.get("/api/v1/holdings/acme/people/organization");
  expect(organization.ok()).toBe(true);
  const { data } = await organization.json() as { data: OrganizationOverview };
  expect(buildOrganizationGraph(data).entries.size).toBeGreaterThan(0);
  await context.close();
});
test.beforeEach(async ({ context }) => { await context.addCookies(signedIn.cookies); });

async function fits(page: Page) {
  expect(await page.evaluate(() => document.documentElement.scrollWidth <= window.innerWidth + 1)).toBe(true);
}
async function findPerson(page: Page, name: string) {
  await page.getByRole("textbox", { name: "Find staff in organization" }).fill(name);
  await page.getByRole("region", { name: "Staff search results" }).getByRole("button", { name: new RegExp(name) }).click();
}

for (const width of [360, 390, 768, 1024, 1440]) {
  for (const theme of ["light", "dark"]) {
    test(`Teams at ${width}px in ${theme}`, async ({ page }, testInfo) => {
      test.skip(testInfo.project.name !== "desktop");
      await page.setViewportSize({ width, height: 900 });
      await page.goto(base + "/people/organization");
      await page.evaluate(theme => document.documentElement.dataset.theme = theme, theme);
      const teams = page.getByRole("region", { name: "Organization teams", exact: true });
      await expect(teams.getByTestId("organization-team-view")).toBeVisible();
      await expect(teams.getByRole("region", { name: "Level 6 · Staff", exact: true })).toBeVisible();
      await fits(page);
      await findPerson(page, "Sofia Ramirez");
      const card = teams.getByRole("article", { name: "Sofia Ramirez", exact: true });
      const detailsButton = card.getByRole("button", { name: "View details for Sofia Ramirez" });
      await expect(detailsButton).toBeFocused();
      await page.keyboard.press("Enter");
      const details = page.getByRole("dialog", { name: "Sofia Ramirez", exact: true });
      await expect(details).toBeVisible();
      await expect(details.getByRole("link", { name: "Open profile to edit" })).toHaveAttribute("href", base + "/people/hr-acme-co-008");
      await page.keyboard.press("Escape");
      await expect(detailsButton).toBeFocused();
      await card.getByRole("button", { name: "View team of Sofia Ramirez" }).click();
      await expect(teams.getByRole("navigation", { name: "Reporting path" })).toContainText("Sofia Ramirez");
      await expect(teams.getByRole("article", { name: "Mali Srisai", exact: true })).toHaveCount(0);
      await teams.getByRole("button", { name: "All teams", exact: true }).click();
      const level = teams.getByRole("region", { name: "Level 6 · Staff", exact: true });
      const expand = level.getByRole("button", { name: /^Show more departments/ });
      if (await expand.count()) {
        await expand.focus(); await page.keyboard.press("Enter");
        await expect(expand).toBeFocused();
      }
      const expandable = teams.locator("section").filter({ has: page.getByRole("button", { name: /^Show more ·/ }) });
      const groupName = await expandable.last().getAttribute("aria-label");
      const group = teams.getByRole("region", { name: groupName!, exact: true });
      if (await group.count()) {
        const more = group.getByRole("button", { name: /^Show more ·/ });
        const before = await group.getByRole("article").count();
        await more.focus(); await page.keyboard.press("Enter");
        expect(await group.getByRole("article").count()).toBeGreaterThan(before);
        await expect(group.locator("button[aria-expanded=true]")).toBeFocused();
        const expandedCount = await group.getByRole("article").count();
        await page.setViewportSize({ width: 390, height: 900 });
        expect(await group.getByRole("article").count()).toBe(expandedCount);
        await group.getByRole("button", { name: "Show less", exact: true }).click();
        await expect(group.getByRole("article")).toHaveCount(2);
        await expect(group.getByRole("button", { name: /^Show more ·/ })).toBeFocused();
      }
      await page.setViewportSize({ width, height: 900 });
      const branch = teams.getByRole("combobox", { name: "Branch", exact: true });
      const option = await branch.locator("option").nth(2).getAttribute("value");
      if (option) {
        await branch.selectOption(option); await expect(branch).toHaveValue(option);
        const department = teams.getByRole("combobox", { name: "Department", exact: true });
        const departmentId = await department.locator("option").nth(2).getAttribute("value");
        await department.selectOption(departmentId!);
        await expect(department).toHaveValue(departmentId!);
      }
      await findPerson(page, "Daniel Kim");
      await expect(branch).toHaveValue("");
      await expect(teams.getByRole("article", { name: "Daniel Kim", exact: true })).toBeVisible();
      await fits(page);
      await page.screenshot({ path: `.scratch/dev/teams-${width}-${theme}.png` });
    });
  }
}
