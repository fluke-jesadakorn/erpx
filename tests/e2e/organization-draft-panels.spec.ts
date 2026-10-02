import { expect, test, type Locator, type Page } from "@playwright/test";
import { emptyDraft } from "../../src/lib/organization-draft";
import { organizationSession, readOrganization } from "../helpers/organization-access-browser";
import { mockPeopleDraft } from "../helpers/people-draft-browser";

const searchName = "Search people, positions, and units";
const workspace = (page: Page) => page.getByTestId("organization-draft-workspace");
const detail = (page: Page) => page.getByTestId("people-detail");
const row = (page: Page, kind: "person" | "position" | "department" | "branch", id: string) => page.getByTestId(`people-${kind}-${id}`);
async function fitsViewport(page: Page) {
  expect(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth + 1)).toBe(true);
}
async function drag(page: Page, source: Locator, destination: Locator) {
  const transfer = await page.evaluateHandle(() => new DataTransfer());
  await source.dispatchEvent("dragstart", { dataTransfer: transfer });
  await destination.dispatchEvent("dragover", { dataTransfer: transfer });
  await destination.dispatchEvent("drop", { dataTransfer: transfer });
  await source.dispatchEvent("dragend", { dataTransfer: transfer });
  await transfer.dispose();
}
async function mouseDrag(page: Page, source: Locator, destination: Locator) {
  await source.scrollIntoViewIfNeeded();
  const start = (await source.boundingBox())!;
  await page.mouse.move(start.x + start.width / 2, start.y + start.height / 2);
  await page.mouse.down();
  await page.mouse.move(start.x + start.width / 2 + 10, start.y + start.height / 2 + 10, { steps: 4 });
  await page.evaluate(() => new Promise<void>(resolve => requestAnimationFrame(() => requestAnimationFrame(() => resolve()))));
  await destination.scrollIntoViewIfNeeded();
  const end = (await destination.boundingBox())!;
  await page.mouse.move(end.x + end.width / 2, end.y + end.height / 2, { steps: 8 });
  await page.mouse.move(end.x + end.width / 2 + 2, end.y + end.height / 2 + 2);
  await page.mouse.up();
}
async function chooseMove(page: Page, source: Locator, destinationId: string) {
  await source.getByRole("button").first().focus();
  await source.getByRole("button", { name: /^Move/ }).focus();
  await page.keyboard.press("Enter");
  const chooser = page.getByTestId("people-move-chooser");
  await expect(chooser).toBeVisible();
  await chooser.getByRole("combobox", { name: "Move destination", exact: true }).selectOption(`position:${destinationId}`);
  await chooser.getByRole("button", { name: "Move here", exact: true }).focus();
  await page.keyboard.press("Enter");
  await expect(chooser).toBeHidden();
}

test("People has one structure, expandable details and no page overflow at supported widths", async ({ page, context }, info) => {
  test.setTimeout(180_000);
  test.skip(info.project.name !== "desktop");
  const errors: string[] = [];
  page.on("pageerror", error => errors.push(error.message));
  page.on("console", message => { if (message.type() === "error") errors.push(message.text()); });
  const base = await organizationSession(context, "hr_admin"), organization = await readOrganization(context, base);
  const drafts = await mockPeopleDraft(page, organization);
  const person = organization.people.find(person => person.status === "active" && person.position_id)!;
  await page.goto(`${base}/people`);
  await expect(workspace(page)).toBeVisible();
  await expect(page.getByTestId("people-organization-rail")).toBeVisible();
  await expect(page.getByRole("searchbox", { name: searchName })).toHaveCount(1);
  await expect(page.locator('[data-testid^="organization-panel-row-"]')).toHaveCount(0);
  await expect(page.getByRole("tablist")).toHaveCount(0);
  const colonDepartment = organization.nodes.find(node => node.kind === "department" && node.status === "active" && node.id.includes(":"));
  if (colonDepartment) {
    const rail = page.getByTestId("people-organization-rail"), companyName = await rail.locator('button[aria-pressed="true"]').first().innerText();
    await row(page, "department", colonDepartment.id).getByRole("button", { name: colonDepartment.display_name, exact: true }).click();
    await expect.poll(() => new URL(page.url()).searchParams.get("unit")).toBe(`department:${colonDepartment.id}`);
    await page.reload();
    await expect(row(page, "department", colonDepartment.id).getByRole("button", { name: colonDepartment.display_name, exact: true })).toHaveAttribute("aria-pressed", "true");
    await rail.getByRole("button", { name: companyName, exact: true }).click();
  }
  await page.getByRole("searchbox", { name: searchName }).fill(person.legal_name);
  await row(page, "person", person.engagement_id).getByRole("button").first().click();
  await expect(detail(page).getByRole("heading", { name: person.legal_name, exact: true })).toBeVisible();
  await detail(page).locator("summary").filter({ hasText: /^Employment/ }).click();
  await expect(detail(page).getByText("Employment type", { exact: true })).toBeVisible();
  for (const width of [320, 390, 768, 1024, 1440]) {
    await page.setViewportSize({ width, height: 1000 });
    await fitsViewport(page);
    await expect(detail(page)).toBeVisible();
    await page.screenshot({ path: `.scratch/dev/people-single-structure-${width}.png`, fullPage: true });
    if (width < 650) {
      await detail(page).getByRole("button", { name: "Close details", exact: true }).click();
      await expect(page.getByRole("button", { name: /^Choose unit/ })).toBeVisible();
      await row(page, "person", person.engagement_id).getByRole("button").first().click();
    }
  }
  await detail(page).getByRole("button", { name: "Close details", exact: true }).click();
  await page.getByRole("button", { name: /Account menu for/ }).click();
  await page.getByRole("group", { name: "Appearance", exact: true }).getByRole("button", { name: "Dark", exact: true }).click();
  await page.keyboard.press("Escape");
  await page.getByRole("searchbox", { name: searchName }).fill("");
  await page.screenshot({ path: ".scratch/dev/people-structure-dark-final.png" });
  await page.getByRole("button", { name: "+ Add", exact: true }).click();
  await page.getByRole("button", { name: "Add person", exact: true }).click();
  await expect(detail(page).getByRole("heading", { name: "Add person", exact: true })).toBeVisible();
  await expect(detail(page).getByLabel("Legal name", { exact: true })).toBeVisible();
  await page.screenshot({ path: ".scratch/dev/people-add-person-dark-final.png" });
  await detail(page).getByRole("button", { name: "Close details", exact: true }).click();
  await page.getByRole("searchbox", { name: searchName }).fill(person.legal_name);
  await row(page, "person", person.engagement_id).locator('[draggable="true"]').focus();
  await page.keyboard.press("Enter");
  const chooser = page.getByTestId("people-move-chooser"), target = organization.positions.find(position => position.active && !position.is_ceo && position.id !== person.position_id)!;
  await chooser.getByRole("combobox", { name: "Move destination", exact: true }).selectOption(`position:${target.id}`);
  await page.screenshot({ path: ".scratch/dev/people-move-dark-final.png" });
  await chooser.getByRole("button", { name: "Cancel", exact: true }).click();
  expect(drafts.writes).toEqual([]);
  expect(errors).toEqual([]);
});

test("three kinds of drag and keyboard moves share a saved private draft, Undo and review", async ({ page, context }, info) => {
  test.setTimeout(180_000);
  test.skip(info.project.name !== "desktop");
  const base = await organizationSession(context, "hr_admin"), organization = await readOrganization(context, base);
  const drafts = await mockPeopleDraft(page, organization);
  const sourcePosition = organization.positions.find(position => position.active && !position.is_ceo && position.department_id && organization.people.some(person => person.status === "active" && person.position_id === position.id) && organization.positions.some(other => other.active && other.id !== position.id && other.title === position.title))!;
  const person = organization.people.find(person => person.status === "active" && person.position_id === sourcePosition.id)!;
  const targetPosition = organization.positions.find(position => position.active && position.title === sourcePosition.title && position.id !== sourcePosition.id)!;
  const targetDepartment = organization.nodes.find(node => node.kind === "department" && node.status === "active" && node.id !== sourcePosition.department_id && organization.nodes.some(branch => branch.id === node.branch_id && branch.kind === "branch" && branch.status === "active"))!;
  const departments = organization.nodes.filter(node => node.kind === "department" && node.status === "active" && node.branch_id);
  const sourceDepartment = departments.find(department => organization.nodes.some(branch => branch.kind === "branch" && branch.status === "active" && branch.id !== department.branch_id && !departments.some(existing => existing.branch_id === branch.id && existing.slug === department.slug))) ?? departments[0];
  const branches = organization.nodes.filter(branch => branch.kind === "branch" && branch.status === "active" && branch.id !== sourceDepartment.branch_id);
  const targetBranch = branches.find(branch => !organization.nodes.some(existing => existing.kind === "department" && existing.branch_id === branch.id && existing.slug === sourceDepartment.slug)) ?? branches[0];
  const duplicateDepartment = organization.nodes.some(existing => existing.kind === "department" && existing.branch_id === targetBranch.id && existing.slug === sourceDepartment.slug);
  const arrangedCount = duplicateDepartment ? 2 : 3;
  expect(targetPosition && targetDepartment && sourceDepartment && targetBranch).toBeTruthy();
  await page.setViewportSize({ width: 1440, height: 1000 });
  await page.goto(`${base}/people`);
  await expect(workspace(page)).toBeVisible();
  await page.getByRole("searchbox", { name: searchName }).fill(sourcePosition.title);
  await page.evaluate(() => {
    const events: Record<string, unknown>[] = [];
    (window as Window & { peopleDragEvents?: Record<string, unknown>[] }).peopleDragEvents = events;
    for (const type of ["dragstart", "dragover", "drop", "dragend"]) document.addEventListener(type, event => {
      const drag = event as DragEvent, target = event.target as HTMLElement;
      const record = { type, target: target.closest("[data-testid]")?.getAttribute("data-testid"), destination: target.closest("[data-people-drop]")?.getAttribute("data-people-drop"), types: [...(drag.dataTransfer?.types ?? [])], prevented: false };
      events.push(record); queueMicrotask(() => record.prevented = event.defaultPrevented);
    }, true);
  });
  await mouseDrag(page, row(page, "person", person.engagement_id).locator('[draggable="true"]').first(), row(page, "position", targetPosition.id).locator(":scope > div").first());
  const events = await page.evaluate(() => (window as Window & { peopleDragEvents?: Record<string, unknown>[] }).peopleDragEvents);
  await test.info().attach("native-people-drag", { body: JSON.stringify(events), contentType: "application/json" });
  await expect.poll(() => drafts.draft?.definition.items, { message: JSON.stringify(events) }).toContainEqual(expect.objectContaining({ kind: "person", id: person.engagement_id, positionId: targetPosition.id, parked: false }));
  await page.getByRole("searchbox", { name: searchName }).fill("");
  await drag(page, row(page, "position", sourcePosition.id).locator('[draggable="true"]').first(), row(page, "department", targetDepartment.id));
  await expect.poll(() => drafts.draft?.definition.items).toContainEqual(expect.objectContaining({ kind: "position", id: sourcePosition.id, departmentId: targetDepartment.id }));
  await drag(page, row(page, "department", sourceDepartment.id).locator('[draggable="true"]').first(), row(page, "branch", targetBranch.id));
  if (duplicateDepartment) await expect(workspace(page).getByRole("alert")).toContainText("A department with this URL code already exists in that branch.");
  await expect.poll(() => drafts.draft?.definition.items).toHaveLength(arrangedCount);
  await page.getByRole("button", { name: "Undo", exact: true }).click();
  await expect.poll(() => drafts.draft?.definition.items).toHaveLength(arrangedCount - 1);
  await page.getByRole("button", { name: "Redo", exact: true }).click();
  await expect.poll(() => drafts.draft?.definition.items).toHaveLength(arrangedCount);
  await expect(page.getByRole("button", { name: "Save draft", exact: true })).toBeDisabled();
  await page.reload();
  await expect(page.getByTestId("people-pending-changes")).toContainText(`${arrangedCount} pending changes`);
  await page.goto(`${base}/people/organization/drafts?draft=browser-private-draft`);
  await expect(page).toHaveURL(/\/people\?draft=browser-private-draft/);
  await expect(page.getByTestId("people-pending-changes")).toContainText(`${arrangedCount} pending changes`);
  const secondPerson = organization.people.find(other => other.status === "active" && other.position_id && other.engagement_id !== person.engagement_id && other.position_id !== targetPosition.id)!;
  await page.getByRole("searchbox", { name: searchName }).fill(secondPerson.legal_name);
  await chooseMove(page, row(page, "person", secondPerson.engagement_id), targetPosition.id);
  await expect.poll(() => drafts.draft?.definition.items).toHaveLength(arrangedCount + 1);
  await page.getByRole("button", { name: `Review changes · ${arrangedCount + 1}`, exact: true }).click();
  await expect(page).toHaveURL(/panel=review/);
  const review = page.getByTestId("organization-draft-review");
  await expect(review).toBeVisible();
  await expect(review.getByRole("heading", { name: "Employee impact", exact: true })).toBeVisible();
  await review.getByLabel("Reason", { exact: true }).fill("Review browser arrangement without changing live staff");
  await review.getByRole("button", { name: "Preview all changes", exact: true }).click();
  await expect(review.getByRole("button", { name: "Confirm and submit all", exact: true })).toBeVisible();
  await expect(review).toContainText("Existing leave remains.");
  await page.goBack();
  await expect(workspace(page)).toBeVisible();
  expect(drafts.writes.some(write => write.action === "submit")).toBe(false);
});

test("saved items with no destination survive refresh and save failures keep the local change", async ({ page, context }, info) => {
  test.setTimeout(120_000);
  test.skip(info.project.name !== "desktop");
  const base = await organizationSession(context, "hr_admin"), organization = await readOrganization(context, base);
  const person = organization.people.find(person => person.status === "active" && person.position_id)!;
  const target = organization.positions.find(position => position.active && !position.is_ceo && position.id !== person.position_id)!;
  const definition = { ...emptyDraft(), items: [{ kind: "person" as const, id: person.engagement_id, parked: true }] };
  const drafts = await mockPeopleDraft(page, organization, definition);
  await page.goto(`${base}/people?draft=browser-private-draft`);
  await expect(page.getByTestId("people-pending-changes")).toContainText(person.legal_name);
  for (const width of [1440, 390, 320]) {
    await page.setViewportSize({ width, height: 1000 });
    await fitsViewport(page);
    const review = page.getByRole("button", { name: /Review changes · 1/ }), bounds = (await review.boundingBox())!;
    expect(await page.evaluate(() => window.scrollY)).toBe(0);
    expect(bounds.y).toBeGreaterThanOrEqual(0);
    expect(bounds.y + bounds.height).toBeLessThanOrEqual(1000);
    for (const name of [/Review changes · 1/, /Discard/]) {
      expect(await page.getByRole("button", { name }).evaluate(button => {
        const box = button.getBoundingClientRect();
        return button.contains(document.elementFromPoint(box.x + box.width / 2, box.y + box.height / 2));
      })).toBe(true);
    }
    await page.screenshot({ path: `.scratch/dev/people-pending-bar-${width}-final.png` });
  }
  await page.setViewportSize({ width: 1440, height: 1000 });
  await expect(page.getByRole("button", { name: /Review changes · 1/ })).toBeDisabled();
  await page.reload();
  await expect(page.getByTestId("people-pending-changes")).toContainText(person.legal_name);
  drafts.failNextSave("The draft service is temporarily unavailable.");
  await page.getByTestId("people-pending-changes").locator("summary").click();
  await chooseMove(page, page.getByTestId("people-pending-changes"), target.id);
  await expect(workspace(page).getByRole("alert")).toContainText("The draft service is temporarily unavailable.");
  await expect(page.getByRole("button", { name: "Save draft", exact: true })).toBeEnabled();
  expect(drafts.draft?.definition.items[0].parked).toBe(true);
  await page.getByRole("button", { name: "Save draft", exact: true }).click();
  await expect.poll(() => drafts.draft?.definition.items[0]).toMatchObject({ positionId: target.id, parked: false });
  await page.reload();
  await expect(page.getByRole("button", { name: /Review changes · 1/ })).toBeEnabled();
  drafts.failNextSave("Reload the latest draft.", "DRAFT_VERSION_CHANGED", 409);
  if (await detail(page).isVisible()) await detail(page).getByRole("button", { name: "Close details", exact: true }).click();
  await page.getByRole("searchbox", { name: searchName }).fill(person.legal_name);
  await chooseMove(page, row(page, "person", person.engagement_id), organization.positions.find(position => position.active && !position.is_ceo && position.id !== person.position_id && position.id !== target.id)!.id);
  await expect(workspace(page).getByRole("alert")).toContainText("Reload the latest draft.");
  await expect(page.getByRole("button", { name: "Reload saved draft", exact: true })).toBeVisible();
  expect(drafts.writes.some(write => write.action === "submit")).toBe(false);
});

test("an old move link keeps its destination chooser open when the first private draft autosaves", async ({ page, context }, info) => {
  test.setTimeout(120_000); test.skip(info.project.name !== "desktop");
  const base = await organizationSession(context, "hr_admin"), organization = await readOrganization(context, base);
  const person = organization.people.find(person => person.status === "active" && person.position_id)!;
  const target = organization.positions.find(position => position.active && !position.is_ceo && position.id !== person.position_id)!;
  const drafts = await mockPeopleDraft(page, organization);
  await page.goto(`${base}/people/move?kind=person&id=${encodeURIComponent(person.engagement_id)}`);
  const chooser = page.getByTestId("people-move-chooser");
  await expect(chooser).toBeVisible();
  await expect.poll(() => drafts.draft?.definition.items).toContainEqual(expect.objectContaining({ kind: "person", id: person.engagement_id, parked: true }));
  await expect.poll(() => new URL(page.url()).searchParams.get("draft")).toBe("browser-private-draft");
  expect(new URL(page.url()).searchParams.get("person")).toBe(person.engagement_id);
  await expect(chooser).toBeVisible();
  await chooser.getByRole("combobox", { name: "Move destination", exact: true }).selectOption(`position:${target.id}`);
  await chooser.getByRole("button", { name: "Move here", exact: true }).click();
  await expect(chooser).toBeHidden();
  await expect.poll(() => drafts.draft?.definition.items).toContainEqual(expect.objectContaining({ kind: "person", id: person.engagement_id, positionId: target.id, parked: false }));
  expect(drafts.writes.some(write => write.action === "submit")).toBe(false);
});

test("review waits for schedule and policy impact, and a failed read must succeed before confirmation", async ({ page, context }, info) => {
  test.setTimeout(120_000);
  test.skip(info.project.name !== "desktop");
  const base = await organizationSession(context, "hr_admin"), organization = await readOrganization(context, base);
  const person = organization.people.find(person => person.status === "active" && person.position_id)!;
  const position = organization.positions.find(position => position.active && !position.is_ceo && position.id !== person.position_id)!;
  const drafts = await mockPeopleDraft(page, organization, { ...emptyDraft(), reason: "Review all effects before a browser arrangement", items: [{ kind: "person", id: person.engagement_id, positionId: position.id, parked: false }] });
  const impact = { people: [{ engagementId: person.engagement_id, name: person.legal_name, upcomingShifts: 0, upcomingLeave: 0, policy: "Browser work policy", calendar: null, payGroup: null }] };
  let requests = 0, release!: () => void, started!: () => void, releaseAgain!: () => void, startedAgain!: () => void;
  const gate = new Promise<void>(resolve => release = resolve), pending = new Promise<void>(resolve => started = resolve);
  const gateAgain = new Promise<void>(resolve => releaseAgain = resolve), pendingAgain = new Promise<void>(resolve => startedAgain = resolve);
  await page.route("**/people/hr-settings?impact=**", async route => {
    requests++;
    if (requests === 2) {
      started(); await gate;
      return route.fulfill({ status: 503, json: { error: { message: "The schedule impact service is unavailable." } } });
    }
    if (requests === 4) { startedAgain(); await gateAgain; }
    await route.fulfill({ status: 200, json: { data: impact } });
  });
  await page.goto(`${base}/people?draft=browser-private-draft&panel=review`);
  const review = page.getByTestId("organization-draft-review"), confirm = review.getByRole("button", { name: "Confirm and submit all", exact: true });
  await expect(review.getByText(/Work policy: Browser work policy/)).toBeVisible();
  await expect(confirm).toBeDisabled();
  try {
    await review.getByRole("button", { name: "Preview all changes", exact: true }).click();
    await pending;
    await expect(review.getByText("Checking affected staff…", { exact: true })).toBeVisible();
    await expect(review.getByText("All changes are ready to submit.", { exact: true })).toBeVisible();
    await expect(confirm).toBeDisabled();
    release();
    await expect(review.getByRole("alert")).toContainText("The schedule impact service is unavailable.");
    await expect(confirm).toBeDisabled();
    await review.getByRole("button", { name: "Retry impact check", exact: true }).click();
    await expect(review.getByText(/Work policy: Browser work policy/)).toBeVisible();
    await expect(confirm).toBeEnabled();
    expect(requests).toBe(3);
    await review.getByRole("button", { name: "Preview all changes", exact: true }).click();
    await pendingAgain;
    await expect(confirm).toBeDisabled();
    releaseAgain();
    await expect(confirm).toBeEnabled();
    expect(requests).toBe(4);
    expect(drafts.writes.some(write => write.action === "submit")).toBe(false);
  } finally { release(); releaseAgain(); }
});

test("a touch handle moves a position to a department whose id contains colons", async ({ page, context }, info) => {
  test.setTimeout(120_000);
  test.skip(info.project.name !== "desktop");
  const base = await organizationSession(context, "hr_admin"), organization = await readOrganization(context, base);
  const candidates = organization.positions.filter(position => position.active && !position.is_ceo && position.department_id);
  const position = candidates.find(position => candidates.some(other => other.title === position.title && other.department_id !== position.department_id && organization.nodes.some(node => node.id === other.department_id && node.status === "active" && node.id.includes(":"))))!;
  const target = candidates.find(other => other.title === position.title && other.department_id !== position.department_id && organization.nodes.some(node => node.id === other.department_id && node.status === "active" && node.id.includes(":")))!;
  const drafts = await mockPeopleDraft(page, organization);
  await page.setViewportSize({ width: 1440, height: 1000 });
  await page.goto(`${base}/people`);
  await page.getByRole("searchbox", { name: searchName }).fill(position.title);
  const source = row(page, "position", position.id).locator('[draggable="true"]').first(), destination = row(page, "department", target.department_id!);
  await source.scrollIntoViewIfNeeded();
  const start = (await source.boundingBox())!, end = (await destination.boundingBox())!;
  expect(start.y + start.height).toBeLessThan(1000); expect(end.y + end.height).toBeLessThan(1000);
  const session = await context.newCDPSession(page), point = (x: number, y: number) => ({ x, y, id: 1, radiusX: 4, radiusY: 4, force: 1 });
  await session.send("Input.dispatchTouchEvent", { type: "touchStart", touchPoints: [point(start.x + start.width / 2, start.y + start.height / 2)] });
  for (let step = 1; step <= 10; step++) await session.send("Input.dispatchTouchEvent", { type: "touchMove", touchPoints: [point(start.x + start.width / 2 + (end.x + end.width / 2 - start.x - start.width / 2) * step / 10, start.y + start.height / 2 + (end.y + end.height / 2 - start.y - start.height / 2) * step / 10)] });
  await session.send("Input.dispatchTouchEvent", { type: "touchEnd", touchPoints: [] });
  await session.detach();
  await expect.poll(() => drafts.draft?.definition.items).toContainEqual(expect.objectContaining({ kind: "position", id: position.id, departmentId: target.department_id, parked: false }));
  expect(drafts.writes.some(write => write.action === "submit")).toBe(false);
});

test("People ignores superseded detail reads and an employee only sees public HR information", async ({ page, context }) => {
  test.setTimeout(120_000);
  const base = await organizationSession(context, "hr_admin"), organization = await readOrganization(context, base);
  await mockPeopleDraft(page, organization);
  const [first, second] = organization.people.filter(person => person.status === "active" && person.position_id).slice(0, 2);
  let release!: () => void, started!: () => void, finished!: () => void;
  const gate = new Promise<void>(resolve => release = resolve), pending = new Promise<void>(resolve => started = resolve), complete = new Promise<void>(resolve => finished = resolve);
  await page.route("**/people/explorer-detail?**", async route => {
    if (new URL(route.request().url()).searchParams.get("id") !== first.engagement_id) return route.continue();
    const response = await route.fetch(); started(); await gate;
    try { await route.fulfill({ response }); } catch { /* Superseded detail reads may be aborted. */ } finally { finished(); }
  });
  await page.setViewportSize({ width: 1440, height: 1000 });
  await page.goto(`${base}/people`);
  try {
    await page.getByRole("searchbox", { name: searchName }).fill(first.legal_name);
    await row(page, "person", first.engagement_id).getByRole("button").first().click(); await pending;
    await page.getByRole("searchbox", { name: searchName }).fill(second.legal_name);
    await row(page, "person", second.engagement_id).getByRole("button").first().click();
    await expect(detail(page).getByRole("heading", { name: second.legal_name, exact: true })).toBeVisible();
    release(); await complete;
    await expect(detail(page).getByRole("heading", { name: second.legal_name, exact: true })).toBeVisible();
  } finally { release(); }
  await page.unroute("**/people/explorer-detail?**");
  const login = await context.request.post("/api/auth/sign-in/email", { headers: { Origin: process.env.ERPX_E2E_BASE_URL ?? "http://127.0.0.1:3100" }, data: { email: process.env.ERPX_E2E_READER_EMAIL ?? "elena.suwan@erpx.dev", password: process.env.ERPX_E2E_READER_PASSWORD ?? "12345678" } });
  expect(login.ok(), await login.text()).toBe(true);
  const endpoint = "/api/v1/holdings/acme/assignments/active", state = (await (await context.request.get(endpoint)).json()).data;
  const assignment = state.options.find((option: { role: string }) => option.role === "employee"); expect(assignment).toBeTruthy();
  expect((await context.request.post(endpoint, { data: { assignmentId: assignment.id, revision: state.revision } })).ok()).toBe(true);
  await page.goto(`${assignment.href.replace(/\/home$/, "")}/people`);
  await expect(page.getByTestId("people-organization-rail")).toBeAttached();
  await page.getByRole("searchbox", { name: searchName }).fill(second.legal_name);
  await row(page, "person", second.engagement_id).getByRole("button").first().click();
  await expect(detail(page).getByText("Work email", { exact: true })).toBeVisible();
  for (const section of ["Employment", "Working time", "Compensation", "History"]) await expect(detail(page).locator("summary").filter({ hasText: new RegExp(`^${section}`) })).toHaveCount(0);
  await expect(detail(page).getByText("Private email (HR)", { exact: true })).toHaveCount(0);
  await expect(page.getByTestId("people-pending-changes")).toHaveCount(0);
  await expect(page.getByRole("button", { name: /^Move/ })).toHaveCount(0);
  await fitsViewport(page);
});

test("working time, roster requirements and effective compensation use their existing workflows", async ({ page, context }, info) => {
  test.setTimeout(150_000);
  test.skip(info.project.name !== "desktop");
  const base = await organizationSession(context, "hr_admin"), organization = await readOrganization(context, base);
  await mockPeopleDraft(page, organization);
  await page.route("**/people/hr-settings?engagement=**", async route => {
    const response = await route.fetch(), payload = await response.json();
    payload.data.calendars.push({ id: "browser-holiday-calendar", name: "Browser holiday calendar", timezone: "Asia/Bangkok" });
    await route.fulfill({ response, json: payload });
  });
  const position = organization.positions.find(position => position.active && !position.is_ceo && position.department_id && organization.people.some(person => person.status === "active" && person.position_id === position.id))!;
  const person = organization.people.find(person => person.status === "active" && person.position_id === position.id)!;
  const posted: { path: string; body: Record<string, unknown> }[] = [];
  await page.route(/\/api\/v1\/.*\/(?:leave\/admin|people\/(?:compensation|engagements\/[^/]+\/pay-group|organization\/positions\/[^/]+))$/, async route => {
    if (route.request().method() !== "POST") return route.continue();
    posted.push({ path: new URL(route.request().url()).pathname, body: route.request().postDataJSON() });
    await route.fulfill({ status: 200, contentType: "application/json", body: JSON.stringify({ data: { id: "browser-hr-command", version: 1 } }) });
  });
  await page.setViewportSize({ width: 1440, height: 1000 });
  await page.goto(`${base}/people`);
  await page.getByRole("searchbox", { name: searchName }).fill(person.legal_name);
  await row(page, "person", person.engagement_id).getByRole("button").first().click();
  const panel = detail(page);
  await panel.locator("summary").filter({ hasText: /^Working time/ }).click();
  await panel.locator("summary").filter({ hasText: /^Assign work policy & holiday calendar/ }).click();
  await panel.getByRole("combobox", { name: "Work policy", exact: true }).selectOption({ index: 1 });
  await panel.getByRole("combobox", { name: "Holiday calendar", exact: true }).selectOption("browser-holiday-calendar");
  await panel.getByLabel("Effective from", { exact: true }).fill("2027-01-01");
  await panel.getByRole("button", { name: "Assign working time", exact: true }).click();
  await expect.poll(() => posted.length).toBe(1);
  expect(posted[0]).toMatchObject({ path: `/api/v1${base}/leave/admin`, body: { resource: "assignment", engagementId: person.engagement_id, effectiveFrom: "2027-01-01", effectiveTo: null } });
  expect(posted[0].body.policyVersionId).toBeTruthy(); expect(posted[0].body.calendarId).toBeTruthy();
  await panel.locator("summary").filter({ hasText: /^Working time/ }).click();
  await panel.locator("summary").filter({ hasText: /^Compensation/ }).click();
  await panel.getByRole("button", { name: "New compensation revision", exact: true }).click();
  await panel.getByLabel("Effective from", { exact: true }).fill("2027-01-01");
  await panel.getByLabel(/^Base pay/).fill("35000");
  await panel.getByLabel("Expected hours", { exact: true }).fill("160");
  await panel.getByRole("button", { name: "Save compensation draft", exact: true }).click();
  await expect.poll(() => posted.length).toBe(2);
  expect(posted[1]).toMatchObject({ path: `/api/v1${base}/people/compensation`, body: { engagementId: person.engagement_id, effectiveFrom: "2027-01-01", baseAmount: "35000", expectedHours: 160 } });
  await panel.getByRole("button", { name: "Close details", exact: true }).click();
  await page.getByRole("searchbox", { name: searchName }).fill(position.title);
  await row(page, "position", position.id).getByRole("button").first().click();
  await panel.locator("summary").filter({ hasText: /^Working time/ }).click();
  await panel.getByLabel("Staff in this position require a weekly roster", { exact: true }).check();
  await panel.getByLabel("Reason", { exact: true }).fill("Review weekly roster requirement from position details");
  await panel.getByRole("button", { name: "Save roster requirement", exact: true }).click();
  await expect.poll(() => posted.length).toBe(3);
  expect(posted[2]).toMatchObject({ path: `/api/v1${base}/people/organization/positions/${position.id}`, body: { rosterRequired: true, expectedVersion: position.version } });
  await fitsViewport(page);
});
