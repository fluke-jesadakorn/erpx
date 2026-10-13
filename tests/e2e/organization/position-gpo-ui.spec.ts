import { expect, test } from "@playwright/test";
import type { PeopleLifecycleData } from "../../../src/lib/people_payroll/people-lifecycle";
import type { GpoRulesOverview } from "../../../src/lib/organization/gpo-rules";
import { interceptOrganizationWrites, organizationSession, readDirectory, readOrganization, selectExplorerItem } from "../../support/helpers/organization-access-browser";

test("HR can choose a Position independently of its Directory readiness", async ({ context, page }) => {
  test.setTimeout(120_000);
  const base = await organizationSession(context, "hr_admin");
  await interceptOrganizationWrites(page);
  const response = await context.request.get(`/api/v1${base}/people/lifecycle`);
  expect(response.ok(), await response.text()).toBe(true);
  const lifecycle = (await response.json()).data as PeopleLifecycleData;
  await page.goto(`${base}/people?new=person`);
  const positions = page.locator("label").filter({ hasText: "Positions supply a default level" }).locator("select");
  await expect(positions).toBeVisible();
  await expect(page.getByRole("button", { name: "Save employee" })).toBeEnabled();
  const options = await positions.locator("option").evaluateAll(options => options.filter(option => (option as HTMLOptionElement).value).map(option => ({ id: (option as HTMLOptionElement).value, disabled: (option as HTMLOptionElement).disabled })));
  expect(options.length).toBeGreaterThan(0);
  expect(options.every(option => !option.disabled), "Directory state never disables an HR Position").toBe(true);
  const position = lifecycle.positions.find(position => position.active && options.some(option => option.id === position.id))!;
  expect(position).toBeTruthy();
  await positions.selectOption(position.id);
  await expect(positions.locator("xpath=..").locator("small").last()).toContainText("HR can save this placement now.");
});

test("IT opens old Directory links in the combined workspace and previews a direct GPO rule", async ({ context, page }) => {
  test.setTimeout(120_000);
  const base = await organizationSession(context, "it_admin"), writes = await interceptOrganizationWrites(page);
  const [organization, directory] = await Promise.all([readOrganization(context, base), readDirectory(context, base)]);
  const position = organization.positions.find(position => !position.is_ceo)!;
  const connection = directory.connections.find(connection => connection.inventory?.objects.some(object => object.kind === "gpo") && connection.inventory.objects.some(object => object.kind === "group") && connection.inventory.objects.some(object => object.kind === "ou"));
  expect(connection, "Seeded Directory inventory has a GPO, security group and OU").toBeTruthy();
  const gpo = connection!.inventory!.objects.find(object => object.kind === "gpo")!;
  const group = connection!.inventory!.objects.find(object => object.kind === "group")!;
  const ou = connection!.inventory!.objects.find(object => object.kind === "ou")!;
  await page.goto(`${base}/technology?view=directory&position=${encodeURIComponent(position.id)}`);
  const workspace = page.getByTestId("organization-access-workspace");
  await expect(workspace).toBeVisible();
  if (test.info().project.name !== "mobile") await expect(workspace.getByRole("tab", { name: "Policies" })).toBeVisible();
  await expect(page.locator("[data-feature-tabs]")).toHaveCount(0);
  await expect(page.getByTestId(`org-position-${position.id}`)).toHaveAttribute("data-selected", "true");
  await workspace.getByRole("searchbox", { name: "Search organization", exact: true }).fill(position.title);
  const details = await selectExplorerItem(page, "position", position.id, true);
  await expect(details.getByRole("button", { name: "Move position", exact: true })).toBeDisabled();
  await expect(details.getByText("Position GPO tasks")).toBeVisible();
  await details.getByRole("button", { name: "Assign GPO", exact: true }).click();
  const dialog = page.getByRole("dialog", { name: "Assign GPO policy", exact: true });
  await expect(dialog.getByRole("combobox", { name: "Apply to", exact: true })).toHaveValue("position");
  await expect(dialog.getByTestId("organization-target-select")).toHaveValue(position.id);
  await dialog.getByRole("combobox", { name: "Directory connection", exact: true }).selectOption(connection!.id);
  await dialog.getByRole("combobox", { name: "Group Policy Object", exact: true }).selectOption(gpo.id);
  await dialog.getByRole("combobox", { name: "User security filter group", exact: true }).selectOption(group.id);
  await dialog.getByRole("combobox", { name: "Computer target OU", exact: true }).selectOption(ou.id);
  await dialog.getByRole("textbox", { name: "Reason", exact: true }).fill("Browser verification preview only");
  const waiting = page.waitForResponse(response => response.url().endsWith("/technology/gpo-rules/preview") && response.request().method() === "POST");
  await dialog.getByRole("button", { name: "Preview change", exact: true }).click();
  const preview = await waiting;
  if (preview.ok()) {
    await expect(dialog.getByRole("heading", { name: "Review policy assignment" })).toBeVisible();
    await expect(dialog.getByText("Directory delivery starts after confirmation. Endpoint application remains unconfirmed.")).toBeVisible();
    await expect(dialog.getByRole("button", { name: "Publish GPO rule", exact: true })).toBeEnabled();
  } else {
    const failure = (await preview.json()).error as { code: string; message: string };
    expect(["DIRECTORY_CONNECTION_REQUIRED", "DIRECTORY_WRITE_NOT_READY", "DIRECTORY_TARGET_INVALID", "GPO_RULE_DUPLICATE"]).toContain(failure.code);
    await expect(dialog.getByRole("alert")).toHaveText(failure.message);
  }
  expect(writes).toHaveLength(0);
});

test("IT closes an unmapped Position task with a reason by keyboard", async ({ context, page }) => {
  const base = await organizationSession(context, "it_admin"), writes = await interceptOrganizationWrites(page);
  const response = await context.request.get(`/api/v1${base}/technology/gpo-rules`);
  expect(response.ok(), await response.text()).toBe(true);
  const overview = (await response.json()).data as GpoRulesOverview;
  const task = overview.tasks.find(task => task.mappingCount === 0 && task.status === "pending");
  expect(task).toBeTruthy();
  await page.goto(`${base}/technology?view=organization&position=${encodeURIComponent(task!.positionId)}`);
  const details = page.getByRole("region", { name: "Selected item details" }).filter({ visible: true });
  const close = details.getByRole("button", { name: "GPO not required" });
  await close.focus(); await page.keyboard.press("Enter");
  const dialog = page.getByRole("dialog", { name: "GPO not required" });
  await expect(dialog.getByRole("button", { name: "Confirm not required" })).toBeDisabled();
  await dialog.getByRole("textbox", { name: "Reason" }).fill("No Windows policy for this Position");
  await dialog.getByRole("button", { name: "Confirm not required" }).click();
  await expect(dialog).toBeHidden();
  expect(writes).toEqual([expect.objectContaining({ path: expect.stringContaining("/technology/gpo-rules/not-required"), body: expect.objectContaining({ positionId: task!.positionId, expectedVersion: task!.version, reason: "No Windows policy for this Position" }) })]);
});
