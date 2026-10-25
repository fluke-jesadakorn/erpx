import { readFile } from "node:fs/promises";
import { expect, test, type Route } from "@playwright/test";
import { openAccountMenu, matchingRoleOptions } from "../helpers/context-switching-browser";
const base = "/holdings/acme/iam";
test("user management previews, persistence, drag alternatives and responsive layouts", async ({
  page,
}, info) => {
  test.setTimeout(180000);
  const errors: string[] = [];
  page.on("pageerror", (e) => errors.push(e.message));
  await page.setViewportSize({ width: 1440, height: 1000 });
  await page.goto("/holdings/acme/people/access");
  await expect(page).toHaveURL("/holdings/acme/technology?view=resources");
  await expect(page.getByRole("heading", { name: "Resources", exact: true, level: 1 })).toBeVisible();
  const nav = page.getByRole("navigation", { name: "Resource sections", exact: true });
  await nav.getByRole("link", { name: "Groups", exact: true }).click();
  // Recover only this suite's empty, unattached rehearsal groups after interrupted runs.
  const leftovers = page
    .getByRole("region", { name: "Management results" })
    .getByRole("button", {
      name: /^Browser rehearsal (desktop|mobile) \d+ 0 members · Manual group$/,
    });
  while (await leftovers.count()) {
    await leftovers.first().click();
    await expect(
      page
        .getByRole("region", { name: "Management details" })
        .getByText("No applicable policy attachments."),
    ).toBeVisible();
    await page.getByRole("button", { name: "Edit group & parent" }).click();
    const cleanup = page.getByRole("dialog", { name: "Edit group" });
    await cleanup.getByLabel("Archive this group").check();
    await cleanup
      .getByLabel("Reason", { exact: true })
      .fill("Archive this suite's empty interrupted rehearsal");
    await cleanup.getByRole("button", { name: "Preview changes" }).click();
    await page
      .getByRole("dialog", { name: "Review the impact" })
      .getByRole("button", { name: "Confirm changes" })
      .click();
    await expect(cleanup).not.toBeVisible();
  }
  await page.getByRole("button", { name: "New group", exact: true }).click();
  let dialog = page.getByRole("dialog", { name: "Set up group" });
  const name = "Browser rehearsal " + info.project.name + " " + Date.now();
  await dialog.getByLabel("Group name").fill(name);
  await dialog
    .getByLabel("Reason", { exact: true })
    .fill("Verify reviewed user management");
  await dialog.getByRole("button", { name: "Preview changes" }).click();
  let preview = page.getByRole("dialog", { name: "Review the impact" });
  await expect(
    preview.getByText("Nothing is saved yet. Review the changes below."),
  ).toBeVisible();
  await preview.getByRole("button", { name: "Back to editing" }).click();
  dialog = page.getByRole("dialog", { name: "Set up group" });
  // Returning from a preview must preserve the draft.
  await expect(dialog.getByLabel("Group name")).toHaveValue(name);
  await dialog.getByRole("button", { name: "Preview changes" }).click();
  preview = page.getByRole("dialog", { name: "Review the impact" });
  await preview.getByRole("button", { name: "Confirm changes" }).click();
  const results = page.getByRole("region", { name: "Management results" });
  await expect(
    results.getByRole("button", { name: new RegExp(name) }),
  ).toBeVisible();
  await nav.getByRole("link", { name: "Accounts", exact: true }).click();
  await results
    .getByRole("textbox", { name: "Search people" })
    .fill("Uri Suwan");
  await results
    .getByRole("checkbox", { name: "Select Uri Suwan", exact: true })
    .check();
  await results
    .getByRole("button", { name: "Groups (1)", exact: true })
    .click();
  dialog = page.getByRole("dialog", { name: "Set up membership" });
  await dialog.getByLabel("Manual group").selectOption({ label: name });
  await dialog
    .getByLabel("Reason", { exact: true })
    .fill("Preview only, preserve employee access");
  await dialog.getByRole("button", { name: "Preview changes" }).click();
  preview = page.getByRole("dialog", { name: "Review the impact" });
  await expect(
    preview.getByText("Uri Suwan → " + name, { exact: true }),
  ).toBeVisible();
  await preview.getByRole("button", { name: "Close dialog" }).click();
  await page
    .getByRole("dialog", { name: "Set up membership" })
    .getByRole("button", { name: "Close dialog" })
    .click();
  await results
    .getByRole("button", { name: "Uri Suwan Linked account" })
    .click();
  await page
    .getByRole("button", { name: "Account & direct access", exact: true })
    .click();
  await expect(
    page
      .getByRole("region", { name: "Access detail" })
      .getByRole("heading", { name: "Uri Suwan", exact: true }),
  ).toBeVisible();
  await page
    .getByRole("button", { name: "Back to user management", exact: true })
    .click();
  // A drag prepares the same membership editor; it never writes access.
  const target = page
    .getByRole("region", { name: "Group membership preparation", exact: true })
    .getByRole("button", { name, exact: true });
  const membershipWrites: string[] = [];
  const observeMembershipWrite = (request: import("@playwright/test").Request) => {
    if (!["GET", "HEAD", "OPTIONS"].includes(request.method()) &&
      new URL(request.url()).pathname.startsWith("/api/v1/holdings/acme/")) membershipWrites.push(request.url());
  };
  page.on("request", observeMembershipWrite);
  await target.scrollIntoViewIfNeeded();
  await results
    .getByRole("button", { name: "Uri Suwan Linked account" })
    .scrollIntoViewIfNeeded();
  await results
    .getByRole("button", { name: "Uri Suwan Linked account" })
    .dragTo(target);
  await expect(
    page.getByRole("dialog", { name: "Set up membership" }),
  ).toBeVisible();
  await page
    .getByRole("dialog", { name: "Set up membership" })
    .getByRole("button", { name: "Close dialog" })
    .click();
  page.off("request", observeMembershipWrite);
  expect(membershipWrites, "Drag and closing the membership editor never commit access or membership").toEqual([]);
  await page.setViewportSize({ width: 390, height: 900 });
  await page.getByRole("navigation", { name: "Management navigation", exact: true })
    .getByRole("button", { name: "Back to list", exact: true }).click();
  await expect(results).toBeVisible();
  await expect(results.getByRole("checkbox", { name: "Select Uri Suwan", exact: true })).toBeChecked();
  await expect(results.getByRole("button", { name: "Groups (1)", exact: true })).toBeEnabled();
  await expect(target).toBeVisible();
  page.on("request", observeMembershipWrite);
  await target.focus();
  await expect(target).toBeFocused();
  await target.press("Enter");
  dialog = page.getByRole("dialog", { name: "Set up membership", exact: true });
  await expect(dialog).toBeVisible();
  await expect(dialog.getByText("1 selected people", { exact: true })).toBeVisible();
  const manualGroup = dialog.getByRole("combobox", { name: "Manual group", exact: true });
  const preparedGroupId = await manualGroup.getByRole("option", { name, exact: true, includeHidden: true }).getAttribute("value");
  expect(preparedGroupId, "The keyboard target is the actual rehearsal group").toBeTruthy();
  await expect(manualGroup).toHaveValue(preparedGroupId!);
  expect(await page.evaluate(() => document.documentElement.scrollWidth <= window.innerWidth + 1)).toBe(true);
  await page.screenshot({ path: info.outputPath("membership-preparation-390.png"), fullPage: true });
  await dialog.getByRole("button", { name: "Close dialog", exact: true }).click();
  await expect(dialog).not.toBeVisible();
  await expect(target).toBeFocused();
  page.off("request", observeMembershipWrite);
  expect(membershipWrites, "Drag, narrow-screen keyboard preparation and closing never commit access or membership").toEqual([]);
  await page.setViewportSize({ width: 1440, height: 1000 });
  await nav.getByRole("link", { name: "Groups", exact: true }).click();
  await expect(results.getByRole("button", { name: new RegExp(name) })).toContainText("0 members · Manual group");
  for (const width of [390, 768, 1440]) {
    await page.setViewportSize({ width, height: 900 });
    for (const theme of ["light", "dark"]) {
      await page.evaluate((theme) => {
        document.documentElement.dataset.theme = theme;
      }, theme);
      await nav.getByRole("link", { name: "Groups", exact: true }).click();
      await results.getByRole("button", { name: new RegExp(name) }).click();
      await expect(
        page
          .getByRole("region", { name: "Management details" })
          .getByRole("heading", { name, exact: true }),
      ).toBeVisible();
      expect(
        await page.evaluate(
          () => document.documentElement.scrollWidth <= window.innerWidth + 1,
        ),
      ).toBe(true);
      await page.screenshot({
        path: info.outputPath("management-" + width + "-" + theme + ".png"),
        fullPage: true,
      });
      if (width <= 760) {
        await page.getByRole("navigation", { name: "Management navigation", exact: true })
          .getByRole("button", { name: "Back to list", exact: true }).click();
        await expect(results).toBeVisible();
      }
    }
  }
  await page.getByRole("button", { name: "Edit group & parent" }).click();
  dialog = page.getByRole("dialog", { name: "Edit group" });
  await dialog.getByLabel("Archive this group").check();
  await dialog
    .getByLabel("Reason", { exact: true })
    .fill("Archive empty browser rehearsal group");
  await dialog.getByRole("button", { name: "Preview changes" }).click();
  await page
    .getByRole("dialog", { name: "Review the impact" })
    .getByRole("button", { name: "Confirm changes" })
    .click();
  await results.getByLabel("Include archived").check();
  await expect(
    results.getByRole("button", { name: new RegExp(name + " Archived") }),
  ).toBeVisible();
  await page.getByRole("navigation", { name: "Primary", exact: true }).filter({ visible: true })
    .getByRole("link", { name: "Custom attributes", exact: true })
    .click();
  await expect(page).toHaveURL("/holdings/acme/technology?view=attributes");
  await expect(page.getByRole("heading", { name: "Custom attributes", exact: true, level: 1 })).toBeVisible();
  await page.getByRole("button", { name: "New attribute" }).click();
  dialog = page.getByRole("dialog", { name: "Set up attribute" });
  await dialog.getByLabel("Label", { exact: true }).fill("Preview skill");
  await dialog.getByLabel("Field type").selectOption("single-select");
  await dialog.getByLabel("Choices, one per line").fill("Beginner\nAdvanced");
  await dialog
    .getByLabel("Reason", { exact: true })
    .fill("Verify attribute preview");
  await dialog.getByRole("button", { name: "Preview changes" }).click();
  await expect(
    page.getByRole("dialog", { name: "Review the impact" }),
  ).toBeVisible();
  await page.keyboard.press("Escape");
  await page.keyboard.press("Escape");
  expect(errors).toEqual([]);
});
test("People inline ERPX grant submits revision, scope and idempotency to reviewed IAM workflow", async ({
  page,
}) => {
  await page.setViewportSize({ width: 1440, height: 1000 });
  await page.goto("/holdings/acme/people/access");
  await page.getByRole("navigation", { name: "Resource sections", exact: true })
    .getByRole("link", { name: "Accounts", exact: true }).click();
  const directoryExplorer=page.locator("summary").filter({hasText:/^\s*Directory explorer\s*$/});
  await expect(directoryExplorer).toHaveCount(1);
  await directoryExplorer.click();
  await page
    .getByRole("button", {
      name: "Accounts & connected directories",
      exact: true,
    })
    .click();
  await page
    .getByRole("region", { name: "Directory results" })
    .getByRole("button", { name: /^Uri Suwan/ })
    .click();
  const detail = page.getByRole("region", { name: "Access detail" });
  await expect(
    detail.getByRole("heading", { name: "Uri Suwan" }),
  ).toBeVisible();
  await detail.getByText("Manage ERPX access", { exact: true }).click();
  await detail
    .getByRole("combobox", { name: "Role", exact: true })
    .selectOption("employee");
  await detail
    .getByRole("combobox", { name: "Scope", exact: true })
    .selectOption("holding");
  await detail.getByLabel("Expires", { exact: true }).fill("2026-12-31");
  await detail
    .getByLabel("Reason", { exact: true })
    .fill("Browser workflow validation");
  let payload: Record<string, unknown> | null = null;
  await page.route("**/api/v1/holdings/acme/iam/assignments", async (route) => {
    expect(route.request().method()).toBe("POST");
    expect(route.request().headers()["idempotency-key"]).toBeTruthy();
    payload = route.request().postDataJSON();
    await route.fulfill({
      json: {
        data: {
          status: "pending",
          message: "Submitted for independent review.",
        },
      },
    });
  });
  await detail.getByRole("button", { name: "Submit grant / renewal" }).click();
  await expect(
    page
      .getByRole("status")
      .filter({ hasText: "Submitted for independent review." }),
  ).toBeVisible();
  expect(payload).toMatchObject({
    roleId: "employee",
    scopeKind: "holding",
    scopeNodeId: null,
    expectedRoleRevision: expect.any(Number),
    userId: expect.any(String),
    reason: "Browser workflow validation",
  });
});
test("People explorer search, pagination, one-context comparison, responsive panes and themes", async ({
  page,
}, testInfo) => {
  test.setTimeout(180000);
  await page.setViewportSize({ width: 1440, height: 1000 });
  await page.goto("/holdings/acme/people/access");
  await page.getByRole("navigation", { name: "Resource sections", exact: true })
    .getByRole("link", { name: "Accounts", exact: true }).click();
  const directoryExplorer=page.locator("summary").filter({hasText:/^\s*Directory explorer\s*$/});
  await expect(directoryExplorer).toHaveCount(1);
  await directoryExplorer.click();
  await page
    .getByRole("button", {
      name: "Accounts & connected directories",
      exact: true,
    })
    .click();
  const tree = page.getByRole("tree", { name: "People hierarchy" }),
    results = page.getByRole("region", { name: "Directory results" }),
    detail = page.getByRole("region", { name: "Access detail" });
  await expect(tree).toBeVisible();
  await tree.getByRole("treeitem").first().focus();
  await page.keyboard.press("ArrowDown");
  await expect(tree.getByRole("treeitem").nth(1)).toBeFocused();
  const initial = await results.getByRole("button", { pressed: false }).count();
  await results.getByRole("button", { name: "Load more" }).click();
  await expect
    .poll(() => results.getByRole("button", { pressed: false }).count())
    .toBeGreaterThan(initial);
  await page.getByRole("searchbox").fill("jesadakorn");
  await expect(results.getByRole("button", { pressed: false })).toHaveCount(1);
  await results.getByRole("button", { pressed: false }).click();
  await expect(
    detail.getByRole("heading", { name: "Jesadakorn Kirtnu" }),
  ).toBeVisible();
  await expect(
    detail.getByText("Select an assignment to see access by business area."),
  ).toBeVisible();
  await detail.getByLabel("Assignment to inspect").selectOption({ index: 1 });
  await expect(
    detail.getByText(
      "Only this assignment is evaluated. Permissions from other roles are not combined.",
    ),
  ).toBeVisible();
  await detail.locator("details").first().locator("summary").first().click();
  await expect(detail.locator("details[open]")).toHaveCount(1);
  await detail.getByRole("button", { name: /Compare person/ }).click();
  await page.getByRole("searchbox").fill("");
  await expect(results.getByRole("button", { pressed: false })).not.toHaveCount(
    0,
  );
  for (let i = 1; i <= 3; i++) {
    const name = await results
      .getByRole("button")
      .nth(i)
      .locator("strong")
      .innerText();
    await results.getByRole("button").nth(i).click();
    await expect(detail.getByRole("heading").first()).toHaveText(name);
    await detail.getByLabel("Assignment to inspect").selectOption({ index: 1 });
    await detail.getByRole("button", { name: /Compare person/ }).click();
  }
  await expect(
    detail.getByRole("button", { name: /Compare person/ }),
  ).toBeDisabled();
  await expect(
    page
      .getByRole("region", { name: "People comparison", exact: true })
      .getByRole("combobox"),
  ).toHaveCount(4);
  for (const width of [390, 768, 1440]) {
    await page.setViewportSize({ width, height: 1000 });
    if (width < 850) {
      await page
        .getByRole("navigation", { name: "Explorer steps" })
        .getByRole("button", { name: "Tree", exact: true })
        .click();
      await expect(tree).toBeVisible();
      await tree.getByRole("treeitem").first().press("Enter");
      await expect(results).toBeVisible();
      await expect(results.getByRole("heading")).toBeFocused();
      await results.getByRole("button").first().click();
      await expect(detail).toBeVisible();
    }
    for (const theme of ["light", "dark"]) {
      await page.evaluate(
        (theme) => (document.documentElement.dataset.theme = theme),
        theme,
      );
      expect(
        await page.evaluate(
          () => document.documentElement.scrollWidth <= window.innerWidth,
        ),
      ).toBe(true);
      await page.screenshot({
        path: testInfo.outputPath(`people-access-${width}-${theme}.png`),
      });
    }
  }
});
test.beforeEach(async ({ page, baseURL }) => {
  test.skip(
    !["localhost", "127.0.0.1"].includes(new URL(baseURL!).hostname),
    "IAM browser acceptance uses the documented local development roster.",
  );
  await page.goto("/login");
  await page
    .getByLabel("Work email")
    .fill(process.env.ERPX_IAM_E2E_EMAIL ?? "jesadakorn.kirtnu@gmail.com");
  await page
    .getByLabel(/^Password(?:\s*\*)?$/)
    .fill(process.env.ERPX_IAM_E2E_PASSWORD ?? "12345678");
  await page.getByRole("button", { name: "Sign in", exact: true }).click();
  await expect(page).toHaveURL(/\/(work-role\?|holdings\/|platform\/)/);
  // Branch lineage supplies Company before this multi-Company account has a
  // Holding preference. Select its existing role through the recovery menu.
  await page.goto("/holdings/acme/branches/bangkok/home");
  await openAccountMenu(page);
  const roles = await matchingRoleOptions(page, "holding owner");
  const [selected] = await Promise.all([
    page.waitForResponse(
      (response) =>
        response.url().endsWith("/assignments/active") &&
        response.request().method() === "POST",
    ).then(async response => ({ ok: response.ok(), status: response.status(), body: await response.text() })),
    roles
      .filter({ hasText: "Acme Corporation" })
      .click(),
  ]);
  expect(selected.ok, selected.body).toBe(true);
  expect(selected.status, selected.body).toBe(200);
  const selection = JSON.parse(selected.body).data as { companyId: string; assignmentId: string };
  expect(selection.companyId).toBe("acme-co");
  const ready = page.locator("[data-work-assignment]");
  await expect(ready).toHaveAttribute("data-work-company", "acme-co");
  await expect(ready).toHaveAttribute("data-work-assignment", selection.assignmentId);
  await expect(page.getByRole("dialog", { name: "Account switcher", exact: true })).toBeHidden();
  await page.getByRole("button", { name: "Switch workspace", exact: true }).click();
  const workspaces = page.getByRole("dialog", { name: "Switch workspace", exact: true });
  await expect(workspaces).toBeVisible();
  const companyUrl = "**/api/v1/holdings/acme/companies/active";
  type CompanyResponse = { ok: boolean; status: number; body: string };
  let resolveCompany!: (response: CompanyResponse) => void;
  let rejectCompany!: (error: unknown) => void;
  const companyResponse = new Promise<CompanyResponse>((resolve, reject) => {
    resolveCompany = resolve;
    rejectCompany = reject;
  });
  const retainCompanyResponse = async (route: Route) => {
    if (route.request().method() !== "POST") return route.continue();
    try {
      // Send the original UI request to the real server. Retain its bytes before
      // forwarding the unchanged response that triggers the full navigation.
      const response = await route.fetch();
      const body = await response.body();
      await route.fulfill({ response, body });
      resolveCompany({ ok: response.ok(), status: response.status(), body: body.toString("utf8") });
    } catch (error) {
      rejectCompany(error);
      await route.abort().catch(() => undefined);
    }
  };
  // Playwright removes this one-use handler before invoking it, so no extra
  // interception update needs to race the resulting full navigation.
  await page.route(companyUrl, retainCompanyResponse, { times: 1 });
  const [company] = await Promise.all([
    companyResponse,
    page.waitForURL("**/holdings/acme/home",{waitUntil:"domcontentloaded"}),
    workspaces.getByRole("button", { name: "Acme Corporation", exact: true }).click(),
  ]);
  expect(company.ok, company.body).toBe(true);
  expect(company.status, company.body).toBe(200);
  expect(JSON.parse(company.body).data.companyId).toBe("acme-co");
  await expect(page).toHaveURL(/\/holdings\/acme\/home/);
  await page.goto(base);
  await expect(page).toHaveURL("/holdings/acme/people");
  await expect(
    page.getByRole("heading", { name: "People", exact: true, level: 1 }),
  ).toBeVisible();
  await expect(ready).toHaveAttribute("data-work-company", "acme-co");
  await expect(ready).toHaveAttribute("data-work-assignment", selection.assignmentId);
});
test("IAM pages, provider selection, matrix explanation and keyboard navigation", async ({
  page,
}) => {
  test.setTimeout(120000);
  const errors: string[] = [];
  page.on("pageerror", (e) => errors.push(e.message));
  await page.goto("/holdings/acme/people");
  await page.getByRole("navigation", { name: /^(Primary|Mobile navigation)$/ }).filter({ visible: true })
    .getByRole("button", { name: "App launcher", exact: true }).click();
  const launcher = page.getByRole("dialog", { name: "App launcher", exact: true });
  await launcher.getByRole("searchbox", { name: "Search apps and pages", exact: true }).fill("Resources");
  const resources = launcher.getByRole("link", { name: /^Resources(?:\s|$)/ });
  await expect(resources).toHaveAttribute("href", "/holdings/acme/technology?view=resources");
  await resources.click();
  await expect(page).toHaveURL("/holdings/acme/technology?view=resources");
  await expect(
    page.getByRole("heading", { name: "Resources", exact: true, level: 1 }),
  ).toBeVisible();
  await page.getByRole("navigation", { name: "Resource sections", exact: true })
    .getByRole("link", { name: "Accounts", exact: true }).click();
  const directoryExplorer=page.locator("summary").filter({hasText:/^\s*Directory explorer\s*$/});
  await expect(directoryExplorer).toHaveCount(1);
  await directoryExplorer.click();
  await page.getByRole("button", { name: "Accounts & connected directories", exact: true }).click();
  const advancedIam = page.getByRole("link", { name: "Advanced IAM console ↗", exact: true });
  await expect(advancedIam).toHaveAttribute("href", `${base}/matrix`);
  await advancedIam.click();
  await expect(page).toHaveURL("/holdings/acme/people/settings?section=matrix");
  for (const [section, destination, heading] of [
    ["users", "/holdings/acme/people", "People"],
    ["roles", "/holdings/acme/people/settings?section=roles", "Roles & permissions"],
    ["groups", "/holdings/acme/technology?view=organization", "Organization & Access"],
    ["connections", "/holdings/acme/people/settings?section=connections", "Directory connections"],
    ["reviews", "/holdings/acme/work?type=people_lifecycle", "My work"],
    ["my-access", "/holdings/acme/people/settings?section=my-access", "People settings"],
  ]) {
    await page.goto(`${base}/${section}`);
    await expect(page).toHaveURL(destination);
    await expect(
      page.getByRole("heading", { name: heading, exact: true, level: 1 }),
    ).toBeVisible();
    if (section === "my-access") {
      const myAccess = page.locator("#iam-my-access");
      await expect(myAccess).toHaveCount(1);
      const myAccessHeading = myAccess.getByRole("heading", { name: "My access", exact: true, level: 2 });
      await expect(myAccessHeading).toHaveCount(1);
      await expect(myAccessHeading).toBeVisible();
    }
    await expect(
      page.getByText("Something went wrong", { exact: true }),
    ).toHaveCount(0);
  }
  // Lifecycle work and IAM audit history now have separate canonical pages.
  await page.goto("/holdings/acme/people/settings?section=reviews");
  await expect(page.getByRole("heading", { name: "Reviews & audit", exact: true, level: 1 })).toBeVisible();
  await page.getByLabel("Search audit").fill("no-such-audit-browser-acceptance");
  const auditResponse = page.waitForResponse(
    (r) => r.url().includes("/iam/audit?") && r.request().method() === "GET",
  );
  await page.getByRole("button", { name: "Search history", exact: true }).click();
  expect((await auditResponse).status()).toBe(200);
  await expect(page.getByText("No matching IAM events.", { exact: true })).toBeVisible();
  await page.goto(`${base}/connections`);
  const provider = page.getByRole("combobox", {
    name: "Provider",
    exact: true,
  });
  await expect(provider).toHaveValue("active_directory");
  await provider.selectOption("entra_id");
  await expect(
    page.getByLabel("Entra tenant ID", { exact: true }),
  ).toBeVisible();
  await provider.selectOption("univention");
  await page
    .getByRole("combobox", { name: "UCS sign-in method" })
    .selectOption("oidc");
  await expect(
    page.getByLabel("Keycloak issuer", { exact: true }),
  ).toBeVisible();
  await page
    .getByRole("combobox", { name: "UCS sign-in method" })
    .selectOption("ldaps");
  await expect(page.getByLabel("LDAPS URL", { exact: true })).toBeVisible();
  // Saving or activation is deliberately unnecessary for presentation acceptance.
  await page.goto(`${base}/matrix`);
  await page.getByLabel("Search", { exact: true }).fill("finance");
  const table = page.getByRole("table");
  const first = table.getByRole("button").first();
  await first.focus();
  await first.press("ArrowDown");
  await expect(table.getByRole("button").nth(1)).toBeFocused();
  await first.click();
  const detail = page.getByRole("region", { name: "Access explanation" });
  await expect(detail).toBeFocused();
  await expect(detail).toContainText("Policy revision");
  await expect(detail).toContainText("Manual assignment");
  await page.getByRole("button", { name: "Close explanation" }).click();
  await expect(detail).toHaveCount(0);
  const downloadPromise = page.waitForEvent("download");
  await page
    .getByRole("button", { name: "Export selected access", exact: true })
    .click();
  const download = await downloadPromise;
  expect(download.suggestedFilename()).toBe("erpx-access.csv");
  expect(await readFile((await download.path())!, "utf8")).toContain(
    '"Permission","Decision"',
  );
  await page.goto("/my-access");
  await expect(page).toHaveURL("/my-access");
  await expect(
    page.getByRole("heading", { name: "My access", exact: true, level: 1 }),
  ).toBeVisible();
  expect(errors).toEqual([]);
  expect(
    await page.evaluate(
      () => document.documentElement.scrollWidth <= window.innerWidth + 1,
    ),
  ).toBe(true);
});
