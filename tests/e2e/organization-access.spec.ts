import { expect, test, type Locator, type Page } from "@playwright/test";
import type { OrganizationAccessData } from "../../src/lib/organization-access";
import type { PositionMovePreview } from "../../src/lib/position-moves";
import type { LifecyclePreview } from "../../src/lib/people-lifecycle";
import { interceptOrganizationWrites, openHrOrganization, organizationSession, readDirectory, readOrganization, selectExplorerItem } from "../helpers/organization-access-browser";

function occupiedPosition(data: OrganizationAccessData) {
  const position = data.positions.find(position => !position.is_ceo && position.level_number === 6 && data.people.some(person => person.position_id === position.id && person.status === "active") && data.people.filter(person => person.position_id === position.id).every(person => person.issues.length === 0));
  expect(position, "Seeded organization has a valid occupied Staff position").toBeTruthy();
  return position!;
}
async function previewRequest(page: Page, dialog: Locator, endpoint: string) {
  const response = page.waitForResponse(response => response.url().endsWith(endpoint) && response.request().method() === "POST");
  await dialog.getByRole("button", { name: "Preview change", exact: true }).click();
  const result = await response;
  expect(result.ok(), await result.text()).toBe(true);
  return (await result.json()).data;
}
test("one workspace preserves old deep links, searches across the tree and supports keyboard selection", async ({ page, context }) => {
  test.setTimeout(120_000);
  const base = await organizationSession(context, "hr_admin"), data = await readOrganization(context, base);
  await interceptOrganizationWrites(page);
  const position = occupiedPosition(data), person = data.people.find(person => person.position_id === position.id)!;
  const links = [
    { query: `view=positions&position=${encodeURIComponent(position.id)}`, testId: `org-position-${position.id}`, detailName: position.title },
    { query: `view=employees&person=${encodeURIComponent(person.person_id)}`, testId: `org-person-${person.engagement_id}`, detailName: person.legal_name },
    { query: `view=directory&position=${encodeURIComponent(position.id)}`, testId: `org-position-${position.id}`, detailName: position.title },
  ];
  for (const link of links) {
    await openHrOrganization(page, `${base}/technology?${link.query}`);
    await expect(page.getByTestId("organization-access-workspace")).toBeVisible();
    await expect(page.getByTestId(link.testId)).toHaveAttribute("data-selected", "true");
    await expect(page.locator('[role="tablist"][aria-label="Detail section"]')).toHaveCount(1);
    await expect(page.locator("#organization-detail-title")).toHaveText(link.detailName);
    if (test.info().project.name === "mobile") {
      await expect(page.getByRole("button", { name: "← Organization" })).toBeVisible();
    }
  }
  await openHrOrganization(page, `${base}/technology?view=organization`);
  const search = page.getByRole("searchbox", { name: "Search organization", exact: true });
  await search.fill("no-such-organization-item-827463");
  await expect(page.getByText("No matching people or organization items.")).toBeVisible();
  await search.fill(person.legal_name);
  const details = await selectExplorerItem(page, "person", person.engagement_id, true);
  await expect(page.locator("#organization-detail-title")).toHaveText(person.legal_name);
  await expect(details.getByRole("button", { name: "Transfer employee", exact: true })).toBeEnabled();
  await expect(page).toHaveURL(new RegExp(`person=${encodeURIComponent(person.engagement_id).replace(/[.*+?^${}()|[\]\\]/g, "\\$&")}`));
  if (test.info().project.name === "mobile") {
    await page.getByRole("button", { name: "← Organization" }).click();
    await expect(page.getByTestId(`org-person-${person.engagement_id}`)).toBeVisible();
  }
  expect(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth + 1)).toBe(true);
});

test("HR reviews all position holders and GPO impact before a Department move", async ({ page, context }) => {
  test.setTimeout(120_000);
  const base = await organizationSession(context, "hr_admin"), data = await readOrganization(context, base), writes = await interceptOrganizationWrites(page);
  const position = occupiedPosition(data), target = data.nodes.find(node => node.kind === "department" && node.status === "active" && node.id !== position.department_id)!;
  expect(target).toBeTruthy();
  await openHrOrganization(page, `${base}/technology?view=organization`);
  await page.getByRole("searchbox", { name: "Search organization", exact: true }).fill(position.title);
  const details = await selectExplorerItem(page, "position", position.id, true);
  await expect(details.getByRole("button", { name: "Assign GPO", exact: true })).toBeDisabled();
  const move = details.getByRole("button", { name: "Move position", exact: true });
  await move.focus(); await page.keyboard.press("Enter");
  const dialog = page.getByRole("dialog", { name: "Move position & employees", exact: true });
  await expect(dialog).toBeVisible();
  await dialog.getByRole("combobox", { name: "Target department", exact: true }).selectOption(target.id);
  const holders = data.people.filter(person => person.position_id === position.id);
  for (const holder of holders) await expect(dialog.getByRole("combobox", { name: holder.legal_name, exact: true })).toHaveValue(holder.manager_person_id ?? "");
  await dialog.getByRole("textbox", { name: "Reason", exact: true }).fill("Browser preview of whole-team Department move");
  const preview = await previewRequest(page, dialog, "/people/organization/position-moves/preview") as PositionMovePreview;
  expect(preview.affectedCount).toBe(holders.length);
  expect(preview.people.every(person => person.before.levelId === person.after.levelId)).toBe(true);
  await expect(dialog.getByRole("heading", { name: "Review scheduled change" })).toBeVisible();
  await expect(dialog.getByText("Current GPO impact", { exact: true })).toBeVisible();
  await expect(dialog.getByText(/Salary stays unchanged/)).toBeVisible();
  expect(writes).toHaveLength(0);
  const confirm = dialog.getByRole("button", { name: "Confirm scheduled change", exact: true });
  if (preview.ready) {
    await expect(confirm).toBeEnabled();
    await confirm.click();
    if (test.info().project.name === "mobile") await page.keyboard.press("Escape");
    await expect(page.getByRole("status").filter({ hasText: "Position change scheduled" })).toBeVisible();
    expect(writes).toHaveLength(1);
    expect(writes[0].body).toMatchObject({ positionId: position.id, targetKind: "department", targetId: target.id, previewToken: preview.previewToken });
  } else {
    await expect(confirm).toBeDisabled();
    for (const blocker of preview.blockers) await expect(dialog.getByRole("alert").filter({ hasText: blocker })).toBeVisible();
  }
});

test("HR Level changes preserve draft manager choices when returning from preview", async ({ page, context }) => {
  test.setTimeout(120_000);
  const base = await organizationSession(context, "hr_admin"), data = await readOrganization(context, base), writes = await interceptOrganizationWrites(page);
  const position = occupiedPosition(data), target = data.levels.find(level => level.level_number === 5)!;
  await openHrOrganization(page, `${base}/technology?view=organization`);
  await page.getByRole("searchbox", { name: "Search organization", exact: true }).fill(position.title);
  const details = await selectExplorerItem(page, "position", position.id);
  await details.getByRole("button", { name: "Change level", exact: true }).click();
  const dialog = page.getByRole("dialog", { name: "Change position level", exact: true });
  await dialog.getByRole("combobox", { name: "Target level", exact: true }).selectOption(target.id);
  const reason = "Browser review of actual Level and leave consequences";
  await dialog.getByRole("textbox", { name: "Reason", exact: true }).fill(reason);
  const preview = await previewRequest(page, dialog, "/people/organization/position-moves/preview") as PositionMovePreview;
  expect(preview.people.length).toBeGreaterThan(0);
  expect(preview.people.every(person => person.after.levelId === target.id)).toBe(true);
  expect(preview.people.every(person => person.before.departmentId === person.after.departmentId)).toBe(true);
  for (const person of preview.people) await expect(dialog.getByText(person.leaveNote, { exact: false }).first()).toBeVisible();
  await dialog.getByRole("button", { name: "Back to edit", exact: true }).click();
  await expect(dialog.getByRole("textbox", { name: "Reason", exact: true })).toHaveValue(reason);
  await expect(dialog.getByRole("combobox", { name: "Target level", exact: true })).toHaveValue(target.id);
  for (const holder of data.people.filter(person => person.position_id === position.id)) await expect(dialog.getByRole("combobox", { name: holder.legal_name, exact: true })).toHaveValue(holder.manager_person_id ?? "");
  expect(writes).toHaveLength(0);
});

test("HR can review an employee transfer from the shared IT details without changing salary", async ({ page, context }) => {
  test.setTimeout(120_000);
  const base = await organizationSession(context, "hr_admin"), data = await readOrganization(context, base), writes = await interceptOrganizationWrites(page);
  const position = occupiedPosition(data), person = data.people.find(person => person.position_id === position.id)!;
  await openHrOrganization(page, `${base}/technology?view=organization&employee=${encodeURIComponent(person.engagement_id)}`);
  await expect(page.getByTestId(`org-person-${person.engagement_id}`)).toHaveAttribute("data-selected", "true");
  const details = page.getByRole("region", { name: "Selected item details" }).filter({ visible: true });
  await expect(page.locator("#organization-detail-title")).toHaveText(person.legal_name);
  await expect(details.getByRole("link", { name: "Employee profile", exact: true })).toHaveAttribute("href", `${base}/people/${person.person_id}`);
  const transfer = details.getByRole("button", { name: "Transfer employee", exact: true });
  await transfer.focus(); await page.keyboard.press("Enter");
  const dialog = page.getByRole("dialog", { name: "Transfer employee", exact: true });
  await expect(dialog.getByRole("combobox", { name: "Target position", exact: true })).toHaveValue(position.id);
  await expect(dialog.getByRole("combobox", { name: "Actual reporting manager", exact: true })).toHaveValue(person.manager_person_id ?? "");
  await dialog.getByRole("textbox", { name: "Reason", exact: true }).fill("Browser preview of employee transfer");
  const preview = await previewRequest(page, dialog, "/people/lifecycle/placements/preview") as LifecyclePreview;
  await expect(dialog.getByRole("heading", { name: "Review scheduled change" })).toBeVisible();
  await expect(dialog.getByText("Salary stays unchanged. HR proceeds on the effective date; Directory delivery is tracked separately.")).toBeVisible();
  if (!preview.ready || preview.legacyCaseToSupersede) await expect(dialog.getByRole("button", { name: "Confirm scheduled change", exact: true })).toBeDisabled();
  expect(writes).toHaveLength(0);
});

test("IT assigns a library GPO by keyboard from the responsive drawer and confirms the preview", async ({ page, context }) => {
  test.setTimeout(120_000);
  const base = await organizationSession(context, "it_admin"), writes = await interceptOrganizationWrites(page);
  const [organization, directory] = await Promise.all([readOrganization(context, base), readDirectory(context, base)]);
  const connection = directory.connections.find(connection => connection.inventory?.objects.some(object => object.kind === "gpo") && connection.inventory.objects.some(object => object.kind === "group") && connection.inventory.objects.some(object => object.kind === "ou"));
  expect(connection, "Directory fixture must include policy, group and OU inventory").toBeTruthy();
  const gpo = connection!.inventory!.objects.find(object => object.kind === "gpo")!, group = connection!.inventory!.objects.find(object => object.kind === "group")!, ou = connection!.inventory!.objects.find(object => object.kind === "ou")!;
  const level = organization.levels.find(level => level.level_number === 6)!;
  // Provider readiness is covered by the live preview test; this response isolates the review UI from external Directory state.
  let captured: Record<string, unknown> | undefined;
  await page.route("**/technology/gpo-rules/preview", async route => {
    captured = route.request().postDataJSON() as Record<string, unknown>;
    await route.fulfill({ status: 200, contentType: "application/json", body: JSON.stringify({ data: {
      token: "browser-preview-token", expiresAt: Date.now() + 60_000, target: { kind: "level", id: level.id }, targetName: `Level 6 · ${level.name}`,
      positions: organization.positions.filter(position => position.level_id === level.id).map(position => ({ id: position.id, title: position.title })), holders: [], changes: { add: ["preview-membership"], remove: [], retained: ["other-source"] }, inheritedByFuturePositions: true, endpointStatus: "unconfirmed",
    } }) });
  });
  if (test.info().project.name === "desktop") await page.setViewportSize({ width: 1600, height: 1000 });
  await page.goto(`${base}/technology?view=organization&level=${encodeURIComponent(level.id)}`);
  await expect(page.getByTestId(`org-level-${level.id}`)).toHaveAttribute("data-selected", "true");
  await page.getByRole("button", { name: "Open GPO library", exact: true }).click();
  const drawer = page.getByRole("dialog", { name: "GPO library", exact: true });
  const library = drawer.getByRole("complementary", { name: "GPO library", exact: true });
  await library.getByRole("combobox", { name: "Directory connection", exact: true }).selectOption(connection!.id);
  await library.getByRole("searchbox", { name: "Search GPO library", exact: true }).fill(gpo.name);
  const card = library.getByTestId(`gpo-library-${gpo.id}`);
  await expect(card).toBeVisible();
  await card.getByRole("button", { name: /Assign policy/ }).focus(); await page.keyboard.press("Enter");
  const dialog = page.getByRole("dialog", { name: "Assign GPO policy", exact: true });
  await expect(dialog.getByRole("combobox", { name: "Apply to", exact: true })).toHaveValue("level");
  await expect(dialog.getByRole("combobox", { name: "Target level", exact: true })).toHaveValue(level.id);
  await expect(dialog.getByRole("combobox", { name: "Group Policy Object", exact: true })).toHaveValue(gpo.id);
  await dialog.getByRole("combobox", { name: "User security filter group", exact: true }).selectOption(group.id);
  await dialog.getByRole("combobox", { name: "Computer target OU", exact: true }).selectOption(ou.id);
  await dialog.getByRole("textbox", { name: "Reason", exact: true }).fill("Browser simulated policy confirmation");
  await previewRequest(page, dialog, "/technology/gpo-rules/preview");
  expect(captured).toMatchObject({ target: { kind: "level", id: level.id }, gpoId: gpo.id, connectionId: connection!.id });
  await expect(dialog.getByText("New positions matching this level will inherit the rule.")).toBeVisible();
  expect(writes).toHaveLength(0);
  await dialog.getByRole("button", { name: "Publish GPO rule", exact: true }).click();
  expect(writes).toHaveLength(1);
  expect(writes[0].body.previewToken).toBe("browser-preview-token");
  await drawer.getByRole("button", { name: "Close dialog", exact: true }).click();
  await expect(page.getByRole("status").filter({ hasText: "GPO rule published" })).toBeVisible();
  expect(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth + 1)).toBe(true);
});
