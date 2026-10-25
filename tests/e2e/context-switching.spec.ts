import { captureCaseScreenshot } from "../helpers/case-screenshot";
import { expect, test, type BrowserContext, type Page } from "@playwright/test";
import {
  activeRoles,
  addAccountFromMenu,
  chooseRole,
  expectWorkAssignment,
  expectPreservedScroll,
  observeDocument,
  observeScroll,
  openAccountMenu,
  selectRoleForSetup,
  signIn,
} from "../helpers/context-switching-browser";

const companyEndpoint = "/api/v1/holdings/acme/companies/active";
const ownerEmail = process.env.ERPX_E2E_ADMIN_EMAIL ?? "jesadakorn.kirtnu@gmail.com";
const ownerPassword = process.env.ERPX_E2E_ADMIN_PASSWORD ?? "12345678";
const peopleEmail = process.env.ERPX_E2E_PEOPLE_EMAIL ?? "mali@erpx.dev";
const peoplePassword = process.env.ERPX_E2E_PEOPLE_PASSWORD ?? "Mali-ERPX-2026!";
const staffEmail = process.env.ERPX_E2E_READER_EMAIL ?? "elena.suwan@erpx.dev";
const staffPassword = process.env.ERPX_E2E_READER_PASSWORD ?? "12345678";

test.setTimeout(180_000);
test.use({ actionTimeout: 30_000, navigationTimeout: 60_000 });

async function ownerSession(context: BrowserContext) {
  const user = await signIn(context, ownerEmail, ownerPassword);
  expect((await context.request.post(companyEndpoint, { data: { companyId: "acme-co" } })).ok()).toBe(true);
  const assignment = await selectRoleForSetup(context, "holding_owner");
  return { user, assignment };
}

async function selectAccount(page: Page, email: string) {
  const menu = await openAccountMenu(page);
  await expect(menu.locator(".account-option").filter({ hasText: email })).toHaveCount(1);
  await menu.locator(".account-option").filter({ hasText: email }).click();
}

test.beforeEach(async ({ baseURL }) => {
  const local = ["localhost", "127.0.0.1"].includes(new URL(baseURL!).hostname);
  test.skip(!local && !process.env.ERPX_E2E_ADMIN_EMAIL, "Use an explicitly authorized account on public hosts");
});

test.afterEach(async ({ context }, info) => {
  if (info.status === "skipped") return;
  // Restore only account/role preferences. These cases never save a business form.
  await ownerSession(context);
});

test("role refresh preserves the complete non-Home URL, document and scroll; downgrade can recover in place", async ({ page, context }, info) => {
  const { assignment: owner } = await ownerSession(context);
  const state = await activeRoles(context);
  const finance = state.options.find(option => option.role === "auditor")!;
  const employee = state.options.find(option => option.role === "employee")!;
  expect(finance).toBeTruthy(); expect(employee).toBeTruthy();
  const path = "/holdings/acme/finance?contextSwitch=role&keep=a%2Fb#flow-title";
  await page.goto(path);
  await expect(page.getByRole("heading", { name: "My finance work", exact: true })).toBeVisible();
  await expectWorkAssignment(page, owner.id);
  const url = page.url();
  await page.evaluate(() => window.scrollTo(0, Math.min(60, document.documentElement.scrollHeight - innerHeight)));
  const scroll = await page.evaluate(() => window.scrollY);
  expect(scroll, "Finance has a scrollable document on both viewports").toBeGreaterThan(0);
  const unchangedDocument = await observeDocument(page);
  await observeScroll(page);

  await openAccountMenu(page);
  await expectPreservedScroll(page, scroll, info);
  await chooseRole(page, "auditor");
  await expectWorkAssignment(page, finance.id);
  await expect(page).toHaveURL(url);
  await expect(page.locator(".active-work-role")).toContainText("auditor");
  await expect(page.getByRole("link", { name: "Add document", exact: true })).toHaveCount(0);
  const expectedScroll = await page.evaluate(previous => Math.min(previous, Math.max(0, document.documentElement.scrollHeight - innerHeight)), scroll);
  await expectPreservedScroll(page, expectedScroll, info);
  await unchangedDocument();

  const other = await context.newPage();
  await other.goto("/holdings/acme/work");
  await expect(other.getByRole("button", { name: /Account menu for/ })).toBeEnabled();
  await expect(other.locator("[data-work-stale]")).toHaveAttribute("data-work-stale", "false");
  const otherUrl = other.url(), unchangedOtherDocument = await observeDocument(other);
  await chooseRole(page, "employee");
  await expect(page.getByRole("heading", { name: "Your role cannot open this view", exact: true })).toBeVisible();
  await expect(page).toHaveURL(url);
  await expect(page.getByRole("heading", { name: "My finance work", exact: true })).toHaveCount(0);
  await expect(page.getByRole("link", { name: "Add document", exact: true })).toHaveCount(0);
  await expect(other.locator("[data-work-stale]")).toHaveAttribute("data-work-stale", "true");
  await captureCaseScreenshot(page, `context-switch-forbidden-${info.project.name}.png`);
  await expect(other.getByText("Work role changed. Your open form has not been submitted.", { exact: false })).toBeVisible();
  expect((await context.request.get("/api/v1/holdings/acme/finance/charts")).status()).toBe(403);
  await unchangedDocument();

  await chooseRole(page, "holding_owner");
  await expectWorkAssignment(page, owner.id);
  await expect(page.getByRole("heading", { name: "My finance work", exact: true })).toBeVisible();
  await expect(page.getByRole("link", { name: "Add document", exact: true })).toBeVisible();
  await expect(page.getByRole("heading", { name: "Your role cannot open this view", exact: true })).toHaveCount(0);
  await expect(page).toHaveURL(url);
  await unchangedDocument();
  await other.getByRole("button", { name: "Reload", exact: true }).click();
  await expect(other.locator("[data-work-stale]")).toHaveAttribute("data-work-stale", "false");
  await expectWorkAssignment(other, owner.id);
  await expect(other).toHaveURL(otherUrl);
  await unchangedOtherDocument();
  await other.close();
});

test("selecting the active role or account makes no switch request", async ({ page, context }) => {
  const { assignment } = await ownerSession(context);
  await page.goto("/holdings/acme/finance?contextSwitch=same#flow-title");
  await expectWorkAssignment(page, assignment.id);
  const state = await activeRoles(context);
  const unchangedDocument = await observeDocument(page);
  const switches: string[] = [];
  page.on("request", request => {
    if (request.method() === "POST" && /\/(assignments\/active|multi-session\/set-active)$/.test(new URL(request.url()).pathname)) switches.push(request.url());
  });
  await chooseRole(page, "holding_owner");
  await expect(page.getByRole("combobox", { name: "Search roles", exact: true })).not.toBeVisible();
  await selectAccount(page, ownerEmail);
  await page.keyboard.press("Escape");
  expect(switches).toEqual([]);
  expect((await activeRoles(context)).revision).toBe(state.revision);
  await expectWorkAssignment(page, assignment.id);
  await unchangedDocument();
});

test("a pending or failed switch hides old data, blocks interaction and preserves the selected context", async ({ page, context }) => {
  const { assignment } = await ownerSession(context);
  await page.goto("/holdings/acme/finance?contextSwitch=failure#flow-title");
  await expectWorkAssignment(page, assignment.id);
  const url = page.url(), initial = await activeRoles(context);
  const unchangedDocument = await observeDocument(page);
  const roleEndpoint = "**/api/v1/holdings/acme/assignments/active";
  await page.route(roleEndpoint, route => route.request().method() === "GET"
    ? route.fulfill({ status: 500, contentType: "application/json", body: JSON.stringify({ error: { message: "Context inspection is temporarily unavailable." } }) })
    : route.continue());
  const failedInspection = page.waitForResponse(response => response.request().method() === "GET" && new URL(response.url()).pathname === "/api/v1/holdings/acme/assignments/active" && response.status() === 500);
  await page.evaluate(() => window.dispatchEvent(new Event("focus")));
  await failedInspection;
  await expect(page.locator("[data-work-stale]")).toHaveAttribute("data-work-stale", "true");
  await expect(page.getByRole("heading", { name: "My finance work", exact: true })).not.toBeVisible();
  await page.unroute(roleEndpoint);
  await page.getByRole("button", { name: "Reload", exact: true }).click();
  await expect(page.locator("[data-work-stale]")).toHaveAttribute("data-work-stale", "false");
  await expectWorkAssignment(page, assignment.id);
  await expect(page.locator("[data-work-revision]")).toHaveAttribute("data-work-revision", String(initial.revision));
  await expect(page.getByRole("heading", { name: "My finance work", exact: true })).toBeVisible();
  await expect(page).toHaveURL(url);
  await unchangedDocument();
  let release!: () => void;
  const gate = new Promise<void>(resolve => { release = resolve; });
  await page.route("**/assignments/active", async route => {
    if (route.request().method() !== "POST") return route.continue();
    await gate;
    await route.fulfill({ status: 503, contentType: "application/json", body: JSON.stringify({ error: { message: "Role switch is temporarily unavailable." } }) });
  });
  try {
    await chooseRole(page, "finance_approver");
    await expect(page.locator("html")).toHaveAttribute("data-work-switching", "true");
    await expect(page.getByRole("status").filter({ hasText: "Updating account and permissions" })).toBeVisible();
    await expect(page.getByRole("heading", { name: "My finance work", exact: true })).not.toBeVisible();
    release();
    await expect(page.getByRole("dialog", { name: "Account switcher", exact: true }).getByRole("alert")).toBeVisible();
    await expect(page.locator("html")).not.toHaveAttribute("data-work-switching", "true");
    await expect(page.getByRole("heading", { name: "My finance work", exact: true })).toBeVisible();
    await expect(page.locator("[data-work-assignment]")).toHaveAttribute("data-work-assignment", assignment.id);
    expect((await activeRoles(context)).revision).toBe(initial.revision);
    await expect(page).toHaveURL(url);
    await unchangedDocument();
  } finally { release(); await page.unroute("**/assignments/active"); }
});

test("a failed account switch keeps the current account and page usable", async ({ page, context }) => {
  // The owner is the account under test here; the second persona is the one the
  // switch attempts to activate, which must fail without replacing anything.
  const { user, assignment } = await ownerSession(context);
  await page.goto("/holdings/acme/finance?contextSwitch=account-failure#flow-title");
  await expectWorkAssignment(page, assignment.id);
  await addAccountFromMenu(page, peopleEmail, peoplePassword);
  // Registering the second persona activates it, so return to the owner
  // through the real switcher before injecting the failure. The attempt below
  // must then fail without replacing the owner's account or page.
  await selectAccount(page, ownerEmail);
  await expect(page.locator("[data-work-user]")).toHaveAttribute("data-work-user", user.id);
  const url = page.url(), unchangedDocument = await observeDocument(page);
  await page.route("**/multi-session/set-active", route => route.fulfill({
    status: 503, contentType: "application/json", body: JSON.stringify({ code: "SESSION_SWITCH_UNAVAILABLE", message: "Account could not be selected." }),
  }));
  await selectAccount(page, peopleEmail);
  await expect(page.getByRole("dialog", { name: "Account switcher", exact: true }).getByRole("alert")).toContainText("Account could not be selected");
  await expect(page.locator("[data-work-user]")).toHaveAttribute("data-work-user", user.id);
  await expect(page.locator("[data-work-assignment]")).toHaveAttribute("data-work-assignment", assignment.id);
  await expect(page.locator("html")).not.toHaveAttribute("data-work-switching", "true");
  await expect(page.getByRole("heading", { name: "My finance work", exact: true })).toBeVisible();
  await expect(page).toHaveURL(url); await unchangedDocument();
});

test("unsaved input asks before changing role and account, and cancel preserves the form", async ({ page, context }) => {
  await signIn(context, peopleEmail, peoplePassword);
  const { assignment } = await ownerSession(context);
  const path = "/holdings/acme/finance/invoices?new=1&contextSwitch=dirty#workspace-content";
  await page.goto(path); await expectWorkAssignment(page, assignment.id);
  const number = page.getByLabel("Invoice number", { exact: true });
  await number.fill("UNSAVED-CONTEXT-SWITCH");
  const initial = await activeRoles(context), url = page.url();
  const unchangedDocument = await observeDocument(page);
  const confirmations: string[] = [], switches: string[] = [], businessWrites: string[] = [];
  let discard = false;
  page.on("dialog", async dialog => { confirmations.push(dialog.message()); if (discard) await dialog.accept(); else await dialog.dismiss(); });
  page.on("request", request => {
    if (!["GET", "HEAD"].includes(request.method())) {
      if (/\/(assignments\/active|multi-session\/set-active)$/.test(new URL(request.url()).pathname)) switches.push(request.url());
      if (/\/finance\//.test(new URL(request.url()).pathname)) businessWrites.push(request.url());
    }
  });
  await chooseRole(page, "finance_approver");
  await expect.poll(() => confirmations.length).toBe(1);
  await expect(number).toHaveValue("UNSAVED-CONTEXT-SWITCH");
  await selectAccount(page, peopleEmail);
  await expect.poll(() => confirmations.length).toBe(2);
  expect(confirmations.every(message => /unsaved/i.test(message))).toBe(true);
  expect(switches).toEqual([]); expect(businessWrites).toEqual([]);
  expect((await activeRoles(context)).revision).toBe(initial.revision);
  await expect(page).toHaveURL(url);
  await expect(number).toHaveValue("UNSAVED-CONTEXT-SWITCH");
  await unchangedDocument();
  discard = true;
  await chooseRole(page, "employee");
  await expect.poll(() => confirmations.length).toBe(3);
  await expect(page.getByRole("heading", { name: "Your role cannot open this view", exact: true })).toBeVisible();
  await expect(number).toHaveCount(0);
  await chooseRole(page, "holding_owner");
  await expectWorkAssignment(page, assignment.id);
  await expect(number).toHaveValue("");
  expect(businessWrites, "Discarding unsaved fields does not alter saved invoice drafts").toEqual([]);
  await expect(page).toHaveURL(url); await unchangedDocument();
});

test("an account that keeps the workspace refreshes People in place and invalidates another tab", async ({ page, context }, info) => {
  const people = await signIn(context, peopleEmail, peoplePassword);
  await selectRoleForSetup(context, "people_manager");
  const { user: owner, assignment } = await ownerSession(context);
  await page.goto("/holdings/acme/people?contextSwitch=account&keep=a%2Fb#workspace-content");
  await expectWorkAssignment(page, assignment.id);
  await expect(page.getByTestId("organization-draft-workspace")).toBeVisible();
  await expect(page.locator("[data-work-user]")).toHaveAttribute("data-work-user", owner.id);
  await page.evaluate(() => window.scrollTo(0, Math.min(120, document.documentElement.scrollHeight - innerHeight)));
  const scroll = await page.evaluate(() => window.scrollY);
  expect(scroll, "People has a scrollable document on both viewports").toBeGreaterThan(0);
  const url = page.url(), unchangedDocument = await observeDocument(page);
  await observeScroll(page);
  const other = await context.newPage();
  await other.goto("/holdings/acme/work");
  await expect(other.getByRole("button", { name: /Account menu for/ })).toBeEnabled();
  await openAccountMenu(page);
  await expectPreservedScroll(page, scroll, info);
  const contextCheck = page.waitForResponse(response => new URL(response.url()).pathname === "/api/v1/account/work-context", { timeout: 30_000 });
  await selectAccount(page, peopleEmail);
  const checked = await contextCheck;
  expect(checked.status(), "The new account's current page context is inspected successfully").toBe(200);
  const checkedData = (await checked.json()).data;
  const previousPage = new URL(url);
  expect(new URL(checked.url()).searchParams.get("from")).toBe(previousPage.pathname + previousPage.search + previousPage.hash);
  expect(new URL(checked.url()).searchParams.get("companyId")).toBe("acme-co");
  expect(checkedData.context.userId).toBe(people.id);
  expect(checkedData.sameWorkspace).toBe(true);
  expect(checkedData.requiresWorkspaceSelection).toBe(false);
  await expect(page.locator("[data-work-user]")).toHaveAttribute("data-work-user", people.id);
  await expect(page.getByRole("button", { name: new RegExp(`Account menu for .*${peopleEmail.replaceAll(".", "\\.")}`) })).toBeVisible();
  await expect(page.locator(".active-work-role")).toContainText("people manager");
  await expect(page.getByRole("heading", { name: "Positions and people", exact: true })).toBeVisible();
  await expect(page.getByRole("dialog", { name: "Account switcher", exact: true })).not.toBeVisible();
  await expect(page.getByRole("dialog", { name: "Switch workspace", exact: true })).not.toBeVisible();
  await expect(page).toHaveURL(url);
  await expectPreservedScroll(page, scroll, info);
  await expect(other.locator("[data-work-stale]")).toHaveAttribute("data-work-stale", "true");
  await unchangedDocument();
  await other.close();
});

test("a scoped account chooses its workspace explicitly; dismissing the chooser keeps the denied URL", async ({ page, context, baseURL }) => {
  test.skip(!["localhost", "127.0.0.1"].includes(new URL(baseURL!).hostname), "Local authorized scoped seed account");
  const staff = await signIn(context, staffEmail, staffPassword);
  const { assignment } = await ownerSession(context);
  await page.goto("/holdings/acme/people?contextSwitch=scope#workspace-content");
  await expectWorkAssignment(page, assignment.id);
  const url = page.url(), unchangedDocument = await observeDocument(page);
  const companySelections: string[] = [];
  page.on("request", request => { if (request.method() === "POST" && request.url().endsWith("/companies/active")) companySelections.push(request.url()); });
  const contextCheck = page.waitForResponse(response => new URL(response.url()).pathname === "/api/v1/account/work-context", { timeout: 30_000 });
  await selectAccount(page, staffEmail);
  const checked = await contextCheck;
  expect(checked.status(), "The scoped account's current page context is inspected successfully").toBe(200);
  expect(new URL(checked.url()).searchParams.get("companyId")).toBe("acme-co");
  const checkedData = (await checked.json()).data;
  expect(checkedData.user.id).toBe(staff.id);
  expect(checkedData.sameWorkspace).toBe(false);
  expect(checkedData.requiresWorkspaceSelection).toBe(true);
  const chooser = page.getByRole("dialog", { name: "Switch workspace", exact: true });
  await expect(chooser).toBeVisible();
  await expect(page).toHaveURL(url);
  expect(companySelections, "Account selection does not choose a company").toEqual([]);
  await page.keyboard.press("Escape");
  await expect(chooser).not.toBeVisible();
  await expect(page.getByRole("heading", { name: "Your role cannot open this view", exact: true })).toBeVisible();
  await expect(page.locator("[data-work-user]")).toHaveAttribute("data-work-user", staff.id);
  await expect(page.getByTestId("organization-draft-workspace")).toHaveCount(0);
  await expect(page).toHaveURL(url); await unchangedDocument();
  await page.getByRole("button", { name: "Switch workspace", exact: true }).click();
  const destination = chooser.locator('a[href="/holdings/acme/branches/bangkok/departments/operations/home"]');
  await expect(destination).toContainText("Operations");
  await destination.click();
  await expect(page).toHaveURL(/\/holdings\/acme\/branches\/bangkok\/departments\/operations\/home$/);
  await expect(page.locator("[data-work-stale]")).toHaveAttribute("data-work-stale", "false");
});

test("an applicable selected role returns from the picker route to the complete requested page", async ({ page, context }) => {
  const { assignment } = await ownerSession(context);
  const destination = "/holdings/acme/finance?contextSwitch=picker&keep=a%2Fb#flow-title";
  await page.goto(`/work-role?from=${encodeURIComponent(destination)}`);
  await expect(page).toHaveURL(new URL(destination, page.url()).toString());
  await expectWorkAssignment(page, assignment.id);
  await expect(page.getByRole("heading", { name: "My finance work", exact: true })).toBeVisible();
});

test("Platform role selection keeps the Manage ERPX page and checks any available alternate authority", async ({ page, context }, info) => {
  await ownerSession(context);
  const endpoint = "/api/v1/holdings/erpx/assignments/active";
  const response = await context.request.get(endpoint);
  test.skip(!response.ok(), "The authorized test account needs an ERPX Holding assignment and Platform access");
  const state = (await response.json()).data;
  const holding = state.options.find((option: { role: string }) => option.role === "holding_owner");
  test.skip(!holding, "An ERPX Holding owner role is required for Manage ERPX");
  if (state.assignmentId !== holding.id) {
    expect((await context.request.post(endpoint, { headers: { "x-erpx-context-company": state.companyId }, data: { assignmentId: holding.id, revision: state.revision } })).ok()).toBe(true);
  }
  // Compile context inspections before opening the page: a cold Next dev
  // route can otherwise broadcast an unrelated RSC refresh to open tabs.
  expect((await context.request.get("/api/v1/platform/assignments/active")).status()).toBe(200);
  const warmContext = new URLSearchParams({ from: "/holdings/erpx/manage-erpx", companyId: state.companyId });
  expect((await context.request.get(`/api/v1/account/work-context?${warmContext}`)).status()).toBe(200);
  await page.goto("/holdings/erpx/manage-erpx?contextSwitch=platform#platform-operations-health");
  const role = page.getByRole("combobox", { name: "Platform role", exact: true });
  await expect(role).toBeVisible();
  const url = page.url(), initial = await role.inputValue(), unchangedDocument = await observeDocument(page);
  const switches: string[] = [];
  page.on("request", request => { if (request.method() === "POST" && new URL(request.url()).pathname === "/api/v1/platform/assignments/active") switches.push(request.url()); });
  await role.selectOption(initial);
  expect(switches).toEqual([]);
  const alternate = await role.getByRole("option", { includeHidden: true }).evaluateAll((options, selected) => {
    const option = (options.find(item => /auditor/i.test(item.textContent ?? "") && (item as HTMLOptionElement).value !== selected)
      ?? options.find(item => (item as HTMLOptionElement).value !== selected)) as HTMLOptionElement | undefined;
    return option ? { id: option.value, label: option.textContent ?? "" } : null;
  }, initial);
  if (!alternate) {
    info.annotations.push({ type: "fixture", description: "Only one authorized Platform role is available; alternate Platform permission changes require a separate fixture." });
    await expect(page).toHaveURL(url); await unchangedDocument();
    return;
  }
  const other = await context.newPage(); await other.goto(url);
  await expect(other.getByRole("button", { name: /Account menu for/ })).toBeEnabled();
  await expect(other.locator("[data-platform-assignment]")).toHaveAttribute("data-platform-assignment", initial);
  try {
    await role.selectOption(alternate.id);
    await expect(page.locator("[data-platform-assignment]")).toHaveAttribute("data-platform-assignment", alternate.id);
    await expect(page.locator("html")).not.toHaveAttribute("data-work-switching", "true");
    await expect(page.getByText(`platform role ${alternate.label.replaceAll(" ", "_")}`, { exact: false })).toBeVisible();
    if (/auditor/i.test(alternate.label)) await expect(page.getByText("Only a Platform administrator can provision a Holding.", { exact: true })).toBeVisible();
    await expect(other.locator("[data-platform-stale]")).toHaveAttribute("data-platform-stale", "true");
    await expect(page).toHaveURL(url); await unchangedDocument();
  } finally {
    await role.selectOption(initial);
    await expect(page.locator("[data-platform-assignment]")).toHaveAttribute("data-platform-assignment", initial);
    await other.close();
  }
});

test("a signed-in account without roles keeps the denied page and account recovery available", async ({ page, context }) => {
  const email = process.env.ERPX_E2E_NO_ROLE_EMAIL, password = process.env.ERPX_E2E_NO_ROLE_PASSWORD;
  test.skip(!email || !password, "An explicitly authorized account without work assignments is required");
  const user = await signIn(context, email!, password!);
  const { assignment } = await ownerSession(context);
  await page.goto("/holdings/acme/finance?contextSwitch=no-role#flow-title");
  await expectWorkAssignment(page, assignment.id);
  const url = page.url(), unchangedDocument = await observeDocument(page);
  await selectAccount(page, email!);
  const chooser = page.getByRole("dialog", { name: "Switch workspace", exact: true });
  if (await chooser.isVisible()) await page.keyboard.press("Escape");
  await expect(page.getByRole("heading", { name: "Your role cannot open this view", exact: true })).toBeVisible();
  await expect(page.locator("[data-work-user]")).toHaveAttribute("data-work-user", user.id);
  await expect(page).toHaveURL(url);
  await expect(page.getByRole("button", { name: /Account menu for/ })).toBeEnabled();
  await expect(page.getByRole("heading", { name: "My finance work", exact: true })).toHaveCount(0);
  await unchangedDocument();
  await selectAccount(page, ownerEmail);
  await expectWorkAssignment(page, assignment.id);
  await expect(page.getByRole("heading", { name: "My finance work", exact: true })).toBeVisible();
});
