import { expect, test } from "@playwright/test";
import type { HoldingPeopleDirectory } from "../../src/server/holding-people-directory";
import { organizationSession, readOrganization } from "../helpers/organization-access-browser";
import { mockPeopleDraft } from "../helpers/people-draft-browser";

// Drag, undo, recovery, responsive details and review are covered in
// organization-draft-panels. Retain the legacy route and department authority
// checks here without editing a user's private draft.
test("legacy organization selection survives refresh and browser Back in the single structure", async ({ page, context }, info) => {
  test.setTimeout(120_000); test.skip(info.project.name !== "desktop");
  const base = await organizationSession(context, "hr_admin"), organization = await readOrganization(context, base);
  const drafts = await mockPeopleDraft(page, organization);
  const person = organization.people.find(person => person.status === "active" && person.position_id)!;
  const position = organization.positions.find(position => position.id === person.position_id)!;
  await page.setViewportSize({ width: 1440, height: 1000 });
  await page.goto(`${base}/people/organization?view=positions&position=${encodeURIComponent(position.id)}`);
  await expect.poll(() => new URL(page.url()).pathname).toBe(`${base}/people`);
  await expect.poll(() => new URL(page.url()).searchParams.get("position")).toBe(position.id);
  const detail = page.getByTestId("people-detail");
  await expect(detail.getByRole("heading", { name: position.title, exact: true })).toBeVisible();
  await page.getByRole("searchbox", { name: "Search people, positions, and units" }).fill(person.legal_name);
  await page.getByTestId(`people-person-${person.engagement_id}`).getByRole("button").first().click();
  await expect(detail.getByRole("heading", { name: person.legal_name, exact: true })).toBeVisible();
  await page.goBack();
  await expect(detail.getByRole("heading", { name: position.title, exact: true })).toBeVisible();
  await page.reload();
  await expect(detail.getByRole("heading", { name: position.title, exact: true })).toBeVisible();
  await expect(page.getByRole("tablist")).toHaveCount(0);
  await detail.getByRole("button", { name: "Close details", exact: true }).click();
  await page.getByRole("searchbox", { name: "Search people, positions, and units" }).fill("");
  const bottomPosition = page.locator('[data-testid^="people-position-"]').last();
  const bottomId = (await bottomPosition.getAttribute("data-testid"))!.slice("people-position-".length);
  const bottomTitle = organization.positions.find(position => position.id === bottomId)!.title;
  await bottomPosition.getByRole("button").first().click();
  expect(await page.evaluate(() => window.scrollY)).toBeGreaterThan(500);
  const heading = detail.getByRole("heading", { name: bottomTitle, exact: true });
  await expect(heading).toBeVisible();
  const bounds = (await heading.boundingBox())!;
  expect(bounds.y).toBeGreaterThanOrEqual(0);
  expect(bounds.y + bounds.height).toBeLessThanOrEqual(1000);
  await page.screenshot({ path: ".scratch/dev/people-deep-position-detail-final.png" });
  expect(drafts.writes).toEqual([]);
});

test("department approvals remain available and the structure uses a large desktop's width", async ({ page, context }, info) => {
  test.setTimeout(120_000); test.skip(info.project.name !== "desktop");
  const base = await organizationSession(context, "hr_admin"), organization = await readOrganization(context, base);
  const drafts = await mockPeopleDraft(page, organization);
  const department = organization.nodes.find(node => node.kind === "department" && node.status === "active")!;
  await page.setViewportSize({ width: 2560, height: 1200 });
  await page.goto(`${base}/people?department=${encodeURIComponent(department.id)}`);
  const detail = page.getByTestId("people-detail");
  await expect(detail.getByRole("heading", { name: department.display_name, exact: true })).toBeVisible();
  await detail.locator("summary").filter({ hasText: /^Approvals/ }).click();
  await expect(detail.getByRole("region", { name: "Appointments and department approval flows", exact: true })).toBeVisible({ timeout: 30_000 });
  await expect(detail.getByRole("heading", { name: "Department approval flows", exact: true })).toBeVisible();
  const main = (await page.locator("#workspace-content").boundingBox())!, structure = (await page.getByTestId("people-structure-workspace").boundingBox())!;
  expect(structure.width / main.width).toBeGreaterThan(0.85);
  expect(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth + 1)).toBe(true);
  expect(drafts.writes).toEqual([]);
});

test("the keyboard destination chooser excludes positions in other companies", async ({ page, context }, info) => {
  test.setTimeout(120_000); test.skip(info.project.name !== "desktop");
  const base = await organizationSession(context, "hr_admin"), organization = await readOrganization(context, base);
  const drafts = await mockPeopleDraft(page, organization);
  const response = await context.request.get(`/api/v1${base}/people-public-directory`);
  expect(response.ok()).toBe(true);
  const directory = (await response.json()).data as HoldingPeopleDirectory;
  const person = organization.people.find(person => person.status === "active" && person.position_id)!;
  const target = organization.positions.find(position => position.active && !position.is_ceo && position.id !== person.position_id)!;
  const foreign = directory.positions.filter(position => position.companyId !== target.company_id);
  test.skip(!foreign.length, "The current seeded directory must expose another company's positions for this check.");
  await page.goto(`${base}/people?person=${encodeURIComponent(person.engagement_id)}`);
  await page.getByTestId("people-detail").getByRole("button", { name: "Move…", exact: true }).focus();
  await page.keyboard.press("Enter");
  const chooser = page.getByTestId("people-move-chooser"), options = await chooser.getByRole("combobox", { name: "Move destination", exact: true }).locator("option").evaluateAll(options => options.map(option => (option as HTMLOptionElement).value));
  expect(options).toContain(`position:${target.id}`);
  for (const position of foreign) expect(options).not.toContain(`position:${position.id}`);
  await chooser.getByRole("button", { name: "Cancel", exact: true }).click();
  expect(drafts.writes).toEqual([]);
  await page.goto(`${base}/people/move/review?${new URLSearchParams({ kind: "person", id: person.engagement_id, targetKind: "position", targetId: foreign[0].id })}`);
  const review = page.getByTestId("organization-draft-review");
  await expect(review).toContainText("Choose an active position in this Company.");
  await expect(review.getByRole("button", { name: "Confirm and submit all", exact: true })).toBeDisabled();
  expect(drafts.writes.some(write => write.action === "submit")).toBe(false);
});
