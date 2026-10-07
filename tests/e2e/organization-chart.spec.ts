import { captureCaseScreenshot } from "../helpers/case-screenshot";
import { browserProjects } from "../../scripts/testing/browser-results";
import { expect, test, type BrowserContext, type Page } from "@playwright/test";
import type { HoldingPeopleDirectory } from "../../src/server/holding-people-directory";
import { organizationSession, readOrganization } from "../helpers/organization-access-browser";
import { mockPeopleDraft } from "../helpers/people-draft-browser";

async function readPeopleDirectory(context: BrowserContext, base: string) {
  const response = await context.request.get(`/api/v1${base}/people-public-directory`);
  expect(response.ok()).toBe(true);
  return (await response.json()).data as HoldingPeopleDirectory;
}
const positionRows = (page: Page) => page.locator('[data-testid^="people-position-"]:not([data-testid="people-position-group"])');
const occupancyFilters = (page: Page) => page.getByRole("group", { name: "Position occupancy", exact: true });
const occupancyButton = (page: Page, name: "All" | "Filled" | "Vacant") => occupancyFilters(page).getByRole("button", { name: new RegExp(`^${name}`) });
async function openPeopleWorkspace(page: Page, url: string) {
  const hydrated = page.waitForResponse(response => response.request().method() === "GET" && /\/people\/organization\/drafts(?:\?|$)/.test(response.url()));
  await page.goto(url);
  await hydrated;
}

test("occupancy filters preserve selection, search, refresh and browser Back", async ({ page, context }) => {
  test.setTimeout(120_000);
  const base = await organizationSession(context, "hr_admin"), organization = await readOrganization(context, base);
  const drafts = await mockPeopleDraft(page, organization), directory = await readPeopleDirectory(context, base);
  const companyId = organization.positions[0].company_id, positions = directory.positions.filter(position => position.companyId === companyId);
  const people = directory.people.filter(person => person.companyId === companyId);
  const filled = positions.filter(position => people.some(person => person.positionId === position.id));
  const vacant = positions.filter(position => !people.some(person => person.positionId === position.id));
  expect(filled.length).toBeGreaterThan(0); expect(vacant.length).toBeGreaterThan(0);
  await openPeopleWorkspace(page, `${base}/people?${new URLSearchParams({ unit: `company:${companyId}`, occupancy: "invalid" })}`);
  await expect(occupancyButton(page, "All")).toHaveAttribute("aria-pressed", "true");
  await expect(occupancyButton(page, "All")).toHaveText(`All${positions.length}`);
  await expect(occupancyButton(page, "Filled")).toHaveText(`Filled${filled.length}`);
  await expect(occupancyButton(page, "Vacant")).toHaveText(`Vacant${vacant.length}`);
  await expect(positionRows(page)).toHaveCount(positions.length);
  await expect(page.getByTestId("people-result-counts")).toHaveText(`${positions.length} positions · ${people.length} people`);
  await occupancyButton(page, "Filled").click();
  await expect(positionRows(page)).toHaveCount(filled.length);
  await expect(page.getByTestId("people-unassigned")).toHaveCount(0);
  await expect(page.locator('[data-occupancy="vacant"]')).toHaveCount(0);
  await expect.poll(() => new URL(page.url()).searchParams.get("unit")).toBe(`company:${companyId}`);
  await occupancyButton(page, "Vacant").click();
  await expect(positionRows(page)).toHaveCount(vacant.length);
  await expect(page.locator('[data-testid^="people-person-"]')).toHaveCount(0);
  await expect(page.getByTestId("people-result-counts")).toHaveText(`${vacant.length} positions · 0 people`);
  await page.reload();
  await expect(occupancyButton(page, "Vacant")).toHaveAttribute("aria-pressed", "true");
  await page.goBack();
  await expect(occupancyButton(page, "Filled")).toHaveAttribute("aria-pressed", "true");
  await occupancyButton(page, "All").click();
  await expect.poll(() => new URL(page.url()).searchParams.has("occupancy")).toBe(false);
  const person = people.find(person => person.positionId)!;
  await page.getByRole("searchbox", { name: "Search people, positions, and units" }).fill(person.legalName);
  await page.getByTestId(`people-person-${person.engagementId}`).getByRole("button").first().click();
  const detail = page.getByTestId("people-detail");
  await expect(detail.getByRole("heading", { name: person.legalName, exact: true })).toBeVisible({ timeout: 30_000 });
  // Keep the detail beside the list so occupancy can change without closing it.
  await page.setViewportSize({ width: 2560, height: 1200 });
  await occupancyButton(page, "Filled").click();
  await expect.poll(() => new URL(page.url()).searchParams.get("occupancy")).toBe("filled");
  await expect.poll(() => new URL(page.url()).searchParams.get("person")).toBe(person.engagementId);
  await expect.poll(() => new URL(page.url()).searchParams.get("q")).toBe(person.legalName);
  await page.reload();
  await expect(detail.getByRole("heading", { name: person.legalName, exact: true })).toBeVisible({ timeout: 30_000 });
  await expect(occupancyButton(page, "Filled")).toHaveAttribute("aria-pressed", "true");
  await detail.getByRole("button", { name: "Close details", exact: true }).click();
  await expect(page.getByRole("searchbox")).toHaveValue(person.legalName);
  expect(drafts.writes).toEqual([]);
});

test("Without position and Archived exclude vacancy filters and empty position rows", async ({ page, context }, info) => {
  test.setTimeout(120_000);
  const base = await organizationSession(context, "hr_admin"), organization = await readOrganization(context, base);
  const drafts = await mockPeopleDraft(page, organization), directory = await readPeopleDirectory(context, base);
  const companyId = organization.positions[0].company_id, unassigned = directory.people.filter(person => person.companyId === companyId && !person.positionId);
  expect(unassigned.length).toBeGreaterThan(0);
  await openPeopleWorkspace(page, `${base}/people?${new URLSearchParams({ unit: `unassigned:${companyId}`, occupancy: "vacant" })}`);
  await expect(page.getByRole("heading", { name: "People without a position", exact: true })).toBeVisible();
  await expect(occupancyFilters(page)).toHaveCount(0);
  await expect(positionRows(page)).toHaveCount(0);
  await expect(page.locator('[data-testid^="people-person-"]')).toHaveCount(unassigned.length);
  await expect(page.getByTestId("people-result-counts")).toHaveText(`0 positions · ${unassigned.length} people`);
  const search = page.getByRole("searchbox", { name: "Search people, positions, and units" });
  await search.fill(unassigned[0].legalName);
  await expect(page.getByTestId(`people-person-${unassigned[0].engagementId}`)).toBeVisible();
  await expect(positionRows(page)).toHaveCount(0);
  await page.reload();
  await expect(occupancyFilters(page)).toHaveCount(0);
  await expect(page.getByTestId(`people-person-${unassigned[0].engagementId}`)).toBeVisible();
  await search.fill(directory.people.find(person => person.companyId === companyId && person.positionId)!.legalName);
  await expect(positionRows(page)).toHaveCount(0);
  await expect(page.getByTestId("people-unassigned")).toHaveCount(0);
  await openPeopleWorkspace(page, `${base}/people?${new URLSearchParams({ unit: `company:${companyId}`, occupancy: "vacant", status: "archived" })}`);
  await expect(page.getByRole("heading", { name: "Archived people", exact: true })).toBeVisible();
  await expect(occupancyFilters(page)).toHaveCount(0);
  await expect(page.getByText("Vacant position", { exact: true })).toHaveCount(0);
  await expect(page.getByRole("button", { name: "+ Add person", exact: true })).toHaveCount(0);
  expect(await positionRows(page).evaluateAll(rows => rows.every(row => !!row.querySelector('[data-testid^="people-person-"]')))).toBe(true);
  if (info.project.name === "mobile") await page.getByRole("button", { name: /^Choose unit/ }).click();
  await page.getByRole("combobox", { name: "People status" }).selectOption("current");
  await expect(occupancyButton(page, "Vacant")).toHaveAttribute("aria-pressed", "true");
  expect(drafts.writes).toEqual([]);
});

test("compact groups retain multiple holders, duplicate titles and keyboard action menus", async ({ page, context }, info) => {
  test.setTimeout(120_000);
  const base = await organizationSession(context, "hr_admin"), organization = await readOrganization(context, base);
  const drafts = await mockPeopleDraft(page, organization), directory = await readPeopleDirectory(context, base);
  const companyId = organization.positions[0].company_id, positions = directory.positions.filter(position => position.companyId === companyId);
  const multiple = positions.find(position => directory.people.filter(person => person.positionId === position.id).length > 1)!;
  expect(multiple).toBeTruthy();
  const holders = directory.people.filter(person => person.positionId === multiple.id);
  const duplicates = positions.filter(position => position.title === multiple.title);
  expect(duplicates.length).toBeGreaterThan(1);
  await openPeopleWorkspace(page, `${base}/people?${new URLSearchParams({ unit: `company:${companyId}` })}`);
  const row = page.getByTestId(`people-position-${multiple.id}`);
  for (const position of duplicates) await expect(page.getByTestId(`people-position-${position.id}`)).toBeVisible();
  for (const holder of holders) {
    const person = row.getByTestId(`people-person-${holder.engagementId}`);
    await expect(person).toBeVisible();
    if (holder.jobTitle === multiple.title) await expect(person.getByRole("button").first()).toHaveText(holder.legalName);
  }
  const group = page.getByTestId("people-position-group").filter({ has: row });
  const groupPositions = positions.filter(position => position.departmentId === multiple.departmentId && position.branchId === multiple.branchId);
  const groupPeople = directory.people.filter(person => groupPositions.some(position => position.id === person.positionId));
  await expect(group.getByTestId("people-group-counts")).toHaveText(`${groupPositions.length} positions · ${groupPeople.length} people`);
  const groupOrder = await page.getByTestId("people-position-group").evaluateAll(groups => groups.map(group => group.querySelector("small")!.textContent));
  expect(groupOrder).toEqual([...groupOrder].sort((a, b) => a!.localeCompare(b!)));
  await expect(row.getByRole("button", { name: "+ Add person", exact: true })).toHaveCount(0);
  const actions = row.getByRole("button", { name: `Actions for ${multiple.title}`, exact: true });
  await actions.focus(); await page.keyboard.press("Enter");
  await expect(row.getByRole("menuitem", { name: "Move…", exact: true })).toBeFocused();
  await page.keyboard.press("ArrowDown");
  await expect(row.getByRole("menuitem", { name: "Add person", exact: true })).toBeFocused();
  await page.keyboard.press("Escape");
  await expect(actions).toBeFocused(); await expect(row.getByRole("menu")).toHaveCount(0);
  await actions.click(); await page.getByRole("searchbox").click();
  await expect(row.getByRole("menu")).toHaveCount(0);
  await actions.click(); await row.getByRole("menuitem", { name: "Add person", exact: true }).click();
  const detail = page.getByTestId("people-detail");
  await expect(detail.getByRole("heading", { name: "Add person", exact: true })).toBeVisible();
  await expect.poll(() => new URL(page.url()).searchParams.get("unit")).toBe(`company:${companyId}`);
  await detail.getByRole("button", { name: "Close details", exact: true }).click();
  if (info.project.name === "desktop") {
    await page.setViewportSize({ width: 2560, height: 1200 });
    const ordinary = positions.find(position => directory.people.filter(person => person.positionId === position.id).length === 1 && position.title.length < 35)!;
    const ordinaryRow = page.getByTestId(`people-position-${ordinary.id}`);
    expect((await ordinaryRow.boundingBox())!.height).toBeLessThanOrEqual(84);
    await ordinaryRow.getByRole("button").first().focus();
    await expect(ordinaryRow.locator(':scope > div:last-child [draggable="true"]')).toHaveCSS("opacity", "1");
    await page.getByRole("searchbox").focus(); await page.mouse.move(0, 0);
    await expect(ordinaryRow.locator(':scope > div:last-child [draggable="true"]')).toHaveCSS("opacity", "0");
    const rail = page.getByTestId("people-organization-rail");
    expect((await rail.boundingBox())!.width).toBe(300);
    await positionRows(page).last().scrollIntoViewIfNeeded();
    expect((await rail.boundingBox())!.y).toBeGreaterThanOrEqual(70);
    expect((await rail.boundingBox())!.y).toBeLessThan(100);
    expect(await rail.evaluate(element => element.scrollHeight > element.clientHeight)).toBe(true);
  } else await expect(row.locator('[draggable="true"]').first()).toHaveCSS("opacity", "1");
  expect(drafts.writes).toEqual([]);
});

test("mocked draft moves update occupancy, group counts and Undo immediately", async ({ page, context }) => {
  test.setTimeout(120_000);
  const base = await organizationSession(context, "hr_admin"), organization = await readOrganization(context, base);
  const drafts = await mockPeopleDraft(page, organization), directory = await readPeopleDirectory(context, base);
  const companyId = organization.positions[0].company_id, positions = directory.positions.filter(position => position.companyId === companyId);
  const source = positions.find(position => !position.isCeo && directory.people.filter(person => person.positionId === position.id).length === 1)!;
  const target = positions.find(position => !position.isCeo && position.id !== source.id && directory.people.some(person => person.positionId === position.id))!;
  const person = directory.people.find(person => person.positionId === source.id)!;
  const filledCount = positions.filter(position => directory.people.some(person => person.positionId === position.id)).length;
  expect(source && target && person).toBeTruthy();
  await openPeopleWorkspace(page, `${base}/people?${new URLSearchParams({ unit: `company:${companyId}`, occupancy: "filled" })}`);
  const personRow = page.getByTestId(`people-person-${person.engagementId}`);
  await personRow.getByRole("button", { name: `Actions for ${person.legalName}`, exact: true }).click();
  await personRow.getByRole("menuitem", { name: "Move…", exact: true }).click();
  const chooser = page.getByTestId("people-move-chooser");
  await chooser.getByRole("combobox", { name: "Move destination", exact: true }).selectOption(`position:${target.id}`);
  await chooser.getByRole("button", { name: "Move here", exact: true }).click();
  await expect(chooser).toBeHidden();
  // Selection can open a detail dialog on mobile; close it before filtering again.
  if (await page.getByTestId("people-detail").isVisible()) await page.getByRole("button", { name: "Close details", exact: true }).click();
  await expect(page.getByTestId(`people-position-${source.id}`)).toHaveCount(0);
  await expect(page.getByTestId(`people-position-${target.id}`).getByTestId(`people-person-${person.engagementId}`)).toBeVisible();
  await expect(occupancyButton(page, "Filled")).toHaveText(`Filled${filledCount - 1}`);
  await expect(occupancyButton(page, "Vacant")).toHaveText(`Vacant${positions.length - filledCount + 1}`);
  const targetGroup = page.getByTestId("people-position-group").filter({ has: page.getByTestId(`people-position-${target.id}`) });
  const remainingGroupPositions = positions.filter(position => position.departmentId === target.departmentId && position.branchId === target.branchId && position.id !== source.id && directory.people.some(person => person.positionId === position.id));
  const groupPeopleCount = directory.people.filter(holder => holder.engagementId !== person.engagementId && remainingGroupPositions.some(position => position.id === holder.positionId)).length + 1;
  await expect(targetGroup.getByTestId("people-group-counts")).toHaveText(`${remainingGroupPositions.length} positions · ${groupPeopleCount} people`);
  await expect.poll(() => drafts.draft?.definition.items).toContainEqual(expect.objectContaining({ kind: "person", id: person.engagementId, positionId: target.id }));
  await occupancyButton(page, "Vacant").click();
  await expect(page.getByTestId(`people-position-${source.id}`)).toBeVisible();
  await expect(page.getByTestId(`people-position-${target.id}`)).toHaveCount(0);
  await page.getByRole("button", { name: "Undo", exact: true }).click();
  await expect(page.getByTestId(`people-position-${source.id}`)).toHaveCount(0);
  await expect(positionRows(page)).toHaveCount(positions.length - filledCount);
  await occupancyButton(page, "Filled").click();
  await expect(page.getByTestId(`people-position-${source.id}`).getByTestId(`people-person-${person.engagementId}`)).toBeVisible();
  await expect(occupancyButton(page, "Filled")).toHaveText(`Filled${filledCount}`);
  expect(drafts.writes.some(write => write.action === "submit")).toBe(false);
});

test("compact rows and details fit light and dark layouts from 320px to a large desktop", browserProjects(["desktop"], "This case explicitly sweeps eight widths from 320px to 2560px in both themes once; mobile occupancy and detail cases remain required."), async ({ page, context }) => {
  test.setTimeout(180_000);
  const base = await organizationSession(context, "hr_admin"), organization = await readOrganization(context, base);
  const drafts = await mockPeopleDraft(page, organization);
  const position = organization.positions.find(position => position.active && organization.people.filter(person => person.status === "active" && person.position_id === position.id).length > 1)!;
  const person = organization.people.find(person => person.status === "active" && person.position_id === position.id)!;
  await openPeopleWorkspace(page, `${base}/people`);
  for (const theme of ["Light", "Dark"]) {
    await page.getByRole("button", { name: /Account menu for/ }).click();
    await page.getByRole("group", { name: "Appearance", exact: true }).getByRole("button", { name: theme, exact: true }).click();
    await page.keyboard.press("Escape");
    await page.getByRole("searchbox").fill(position.title);
    for (const width of [320, 390, 650, 768, 1024, 1440, 1920, 2560]) {
      await page.setViewportSize({ width, height: 1000 });
      await expect(page.getByRole("button", { name: /Account menu for/ })).toBeEnabled();
      expect(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth + 1)).toBe(true);
      await expect(page.getByTestId(`people-person-${person.engagement_id}`)).toBeVisible();
      await captureCaseScreenshot(page, `people-compact-${theme.toLowerCase()}-${width}.png`, { animations: "disabled" });
      await page.getByTestId(`people-person-${person.engagement_id}`).getByRole("button").first().click();
      const detail = page.getByTestId("people-detail");
      await expect(detail.getByRole("heading", { name: person.legal_name, exact: true })).toBeVisible();
      expect(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth + 1)).toBe(true);
      if (width === 320) {
        expect((await detail.boundingBox())!.width).toBe(320);
        await captureCaseScreenshot(page, `people-compact-detail-${theme.toLowerCase()}-320.png`, { animations: "disabled" });
      }
      await page.keyboard.press("Escape");
      await expect(detail).toHaveCount(0);
    }
  }
  expect(drafts.writes).toEqual([]);
});

// Drag, undo, recovery, responsive details and review are covered in
// organization-draft-panels. Retain the legacy route and department authority
// checks here without editing a user's private draft.
test("legacy organization selection survives refresh and browser Back in the single structure", async ({ page, context }, info) => {
  test.setTimeout(120_000);
  const base = await organizationSession(context, "hr_admin"), organization = await readOrganization(context, base);
  const drafts = await mockPeopleDraft(page, organization);
  const person = organization.people.find(person => person.status === "active" && person.position_id)!;
  const position = organization.positions.find(position => position.id === person.position_id)!;
  await page.setViewportSize({ width: info.project.name === "desktop" ? 1440 : 390, height: 1000 });
  await page.goto(`${base}/people/organization?view=positions&position=${encodeURIComponent(position.id)}`);
  await expect.poll(() => new URL(page.url()).pathname).toBe(`${base}/people`);
  await expect.poll(() => new URL(page.url()).searchParams.get("position")).toBe(position.id);
  const detail = page.getByTestId("people-detail");
  await expect(detail.getByRole("heading", { name: position.title, exact: true })).toBeVisible();
  if (info.project.name === "mobile") {
    await detail.getByRole("button", { name: "Close details", exact: true }).click();
    await expect(detail).toHaveCount(0);
    await expect.poll(() => new URL(page.url()).searchParams.get("position")).toBeNull();
  }
  await page.getByRole("searchbox", { name: "Search people, positions, and units" }).fill(person.legal_name);
  await page.getByTestId(`people-person-${person.engagement_id}`).getByRole("button").first().click();
  await expect(detail.getByRole("heading", { name: person.legal_name, exact: true })).toBeVisible();
  await page.goBack();
  if (info.project.name === "mobile") {
    await expect(detail).toHaveCount(0);
    await expect.poll(() => new URL(page.url()).searchParams.get("person")).toBeNull();
    await expect(page.getByRole("searchbox", { name: "Search people, positions, and units" })).toHaveValue(person.legal_name);
    await page.goBack();
  }
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
  await captureCaseScreenshot(page, "people-deep-position-detail-final.png");
  expect(drafts.writes).toEqual([]);
});

test("department approvals remain available and the structure uses a large desktop's width", browserProjects(["desktop"], "The case asserts a 2560px desktop structure width; department approval behavior is covered in both projects by organization-access."), async ({ page, context }) => {
  test.setTimeout(120_000);
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

test("the keyboard destination chooser excludes positions in other companies", async ({ page, context }) => {
  test.setTimeout(120_000);
  const base = await organizationSession(context, "hr_admin"), organization = await readOrganization(context, base);
  const drafts = await mockPeopleDraft(page, organization);
  const response = await context.request.get(`/api/v1${base}/people-public-directory`);
  expect(response.ok()).toBe(true);
  const directory = (await response.json()).data as HoldingPeopleDirectory;
  const person = organization.people.find(person => person.status === "active" && person.position_id)!;
  const target = organization.positions.find(position => position.active && !position.is_ceo && position.id !== person.position_id)!;
  const foreign = directory.positions.filter(position => position.companyId !== target.company_id);
  expect(foreign.length, "The isolated directory must include another Company's positions for this boundary check.").toBeGreaterThan(0);
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
  await expect(review).toContainText("Choose an active position in this Company.", { timeout: 30_000 });
  await expect(review.getByRole("button", { name: "Confirm and submit all", exact: true })).toBeDisabled();
  expect(drafts.writes.some(write => write.action === "submit")).toBe(false);
});
