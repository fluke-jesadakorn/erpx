import { captureCaseScreenshot } from "../helpers/case-screenshot";
import { browserProjects } from "../../scripts/testing/browser-results";
import { test, expect, type Locator, type Page } from "@playwright/test";
import { frontOrigin } from "../helpers/front-simulation-session";
import { openAccountMenu, searchRoleOptions } from "../helpers/context-switching-browser";
import { selectAcmeHoldingOwner } from "./helpers/select-acme-holding-owner";
test.use({ actionTimeout: 15000 });
function workspace(page: Page) {
  return page.locator("#workspace-content:visible");
}
async function login(page: Page) {
  await page.goto("/login");
  await page.getByLabel("Work email").fill(
    process.env.ERPX_E2E_ADMIN_EMAIL ?? "jesadakorn.kirtnu@gmail.com",
  );
  await page.getByLabel(/^Password(?:\s*\*)?$/).fill(
    process.env.ERPX_E2E_ADMIN_PASSWORD ?? "12345678",
  );
  const signIn = page.waitForResponse((response) =>
    response.url().endsWith("/api/auth/sign-in/email"),
  );
  await page.getByRole("button", { name: "Sign in", exact: true }).click();
  expect((await signIn).ok()).toBe(true);
  await selectAcmeHoldingOwner(page);
  await page.goto("/holdings/acme/home");
  await expect(page).toHaveURL(/\/holdings\/acme\/home$/);
  await expect(workspace(page)).toHaveCount(1);
  await expect(page.getByRole("button", { name: "Search workspace", exact: true })).toBeEnabled();
}
test("canonical workspace routes and legacy aliases retain their scope", async ({
  page,
}) => {

  await login(page);
  const acmeRoleResponse = await page.request.get("/api/v1/holdings/acme/assignments/active");
  expect(acmeRoleResponse.ok()).toBe(true);
  const acmeRoleState = (await acmeRoleResponse.json()).data as { valid: boolean; companyId: string; userId: string; assignmentId: string; revision: number };
  expect(acmeRoleState).toMatchObject({ valid: true, companyId: "acme-co", userId: "usr_jesadakorn" });
  const managePath = "/holdings/erpx/manage-erpx";
  await page.goto(`/work-role?from=${encodeURIComponent(managePath)}`);
  const erpxRoleResponse = await page.request.get("/api/v1/holdings/erpx/assignments/active");
  expect(erpxRoleResponse.ok()).toBe(true);
  const erpxRoleState = (await erpxRoleResponse.json()).data as { revision: number; options: { id: string; role: string }[] };
  const erpxOwners = erpxRoleState.options.filter(role => role.role === "holding_owner");
  expect(erpxOwners).toHaveLength(1);
  const erpxOptions = await searchRoleOptions(page, "holding owner");
  const erpxOwner = erpxOptions.getByRole("option").filter({ hasText: "ERPX browser realm" });
  await expect(erpxOwner).toHaveCount(1);
  const erpxSelected = page.waitForResponse(response => response.request().method() === "POST" && response.url().endsWith("/holdings/erpx/assignments/active"));
  await erpxOwner.click();
  const erpxSelection = await erpxSelected;
  expect(erpxSelection.status()).toBe(200);
  expect(erpxSelection.request().postDataJSON()).toEqual({ assignmentId: erpxOwners[0].id, revision: erpxRoleState.revision });
  await expect(page).toHaveURL(managePath);
  expect((await page.request.get("/platform/home")).status(), "/platform/home").toBe(200);
  await page.goto("/holdings/acme/home");
  const returnedAcmeResponse = await page.request.get("/api/v1/holdings/acme/assignments/active");
  expect(returnedAcmeResponse.ok()).toBe(true);
  const returnedAcme = (await returnedAcmeResponse.json()).data as typeof acmeRoleState;
  expect(returnedAcme).toMatchObject({ valid: true, companyId: acmeRoleState.companyId, userId: acmeRoleState.userId, assignmentId: acmeRoleState.assignmentId });
  expect(returnedAcme.revision).toBeGreaterThanOrEqual(acmeRoleState.revision);
  const returnedWorkspace = page.locator("[data-work-assignment]");
  await expect(returnedWorkspace).toHaveAttribute("data-work-company", returnedAcme.companyId);
  await expect(returnedWorkspace).toHaveAttribute("data-work-assignment", returnedAcme.assignmentId);
  await expect(returnedWorkspace).toHaveAttribute("data-work-user", returnedAcme.userId);
  await expect(returnedWorkspace).toHaveAttribute("data-work-revision", String(returnedAcme.revision));
  await expect(page.getByRole("button", { name: "Search workspace", exact: true })).toBeEnabled();
  for (const path of [
    "/holdings/acme/home",
    "/holdings/acme/branches/bangkok/home",
    // The current local roster places Finance inside Bangkok Operations.
    "/holdings/acme/branches/bangkok/departments/finance/home",
    "/holdings/acme/branches/bangkok/departments/people/home",
  ])
    expect((await page.request.get(path)).status(), path).toBe(200);
  await page.goto("/holdings/acme/branches/bangkok/feed");
  await expect(page).toHaveURL(/\/branches\/bangkok\/home$/);
  await page.goto("/holdings/acme/branches/bangkok/staff-tasks?q=handover");
  await expect(page).toHaveURL(
    /\/branches\/bangkok\/work\?queue=all&type=staff_task&q=handover$/,
  );
  await page.goto("/holdings/acme/staff-settings");
  await expect(page).toHaveURL("/holdings/acme/people?company=acme-co&tab=settings");
  await expect(workspace(page).getByRole("heading", { name: "People", exact: true, level: 1 })).toBeVisible();
  expect((await page.request.get("/h/h001/home")).status()).toBe(404);
});
test("development module and generic record UI routes are unavailable without redirects", async ({
  page,
}) => {

  await login(page);
  for (const path of [
    "/holdings/acme/modules",
    "/holdings/acme/modules/finance/ledger",
    "/platform/modules",
    "/platform/modules/finance/ledger",
    "/holdings/acme/preview/records/finance/sample-record",
    "/holdings/acme/records/finance/new",
    "/holdings/acme/records/finance/record-1",
  ]) {
    const response = await page.request.get(path, { maxRedirects: 0 });
    expect(response.status(), path).toBe(404);
    expect(response.headers().location, path).toBeUndefined();
  }
});
test("desktop and mobile navigation place live business destinations correctly", async ({
  page,
}, info) => {
  test.setTimeout(180_000);
  const browserErrors: string[] = [];
  page.on("pageerror", error => browserErrors.push(error.message));
  const recordConsoleError = (message: import("@playwright/test").ConsoleMessage) => {
    if (message.type() === "error") browserErrors.push(message.text());
  };
  if (info.project.name === "desktop")
    await page.setViewportSize({ width: 1280, height: 900 });
  await login(page);
  page.on("console", recordConsoleError);
  const nav = page
    .getByRole("navigation", { name: /^(Primary|Mobile navigation)$/ })
    .filter({ visible: true });
  for (const destination of info.project.name === "desktop"
    ? ["Today", "My work", "Calendar", "Messages"]
    : ["Today", "My work", "Messages"])
    await expect(
      nav.getByRole("link", { name: new RegExp(`^${destination}(?:\\s|$)`) }),
    ).toBeVisible();
  const moreButton = nav.getByRole("button", { name: "App launcher" });
  await expect(nav.getByRole("button", { name: info.project.name === "desktop" ? "Start" : "Start work", exact: true })).toBeVisible();
  if (info.project.name === "desktop") {
    await page.goto("/holdings/acme/people");
    await expect(page.getByRole("button", { name: "Search workspace", exact: true })).toBeEnabled();
    await expect(nav.getByRole("button", { name: "Collapse People & organization" })).toHaveAttribute("aria-expanded", "true");
    await expect(nav.getByRole("link", { name: "People", exact: true })).toBeVisible();
    await nav.getByRole("button", { name: "Expand Projects & routines" }).click();
    await expect(nav.getByRole("button", { name: "Collapse Projects & routines" })).toHaveAttribute("aria-expanded", "true");
    await expect(nav.getByRole("button", { name: "Expand People & organization" })).toHaveAttribute("aria-expanded", "false");
    await expect(nav.getByRole("link", { name: "All projects", exact: true })).toBeVisible();
    await nav.getByRole("button", { name: "Expand Purchasing" }).click();
    await expect(nav.getByRole("link", { name: "Suppliers", exact: true })).toBeVisible();
    await page.reload();
    await expect(page.getByRole("button", { name: "Search workspace", exact: true })).toBeEnabled();
    await expect(nav.getByRole("button", { name: "Collapse People & organization" })).toHaveAttribute("aria-expanded", "true");
    await nav.getByRole("button", { name: "Collapse sidebar" }).click();
    await expect(nav).toHaveAttribute("data-rail", "true");
    await captureCaseScreenshot(page, "people-navigation-rail-900-final.png");
    await page.setViewportSize({ width: 1280, height: 600 });
    await expect(nav.getByRole("button", { name: "App launcher", exact: true })).toBeInViewport();
    await expect(nav.getByRole("button", { name: "Expand sidebar", exact: true })).toBeInViewport();
    await nav.getByRole("button", { name: "Expand Administration", exact: true }).click();
    const systemFlyout = nav.getByRole("group", { name: "Administration", exact: true });
    await expect(systemFlyout).toBeVisible();
    const flyoutBounds = (await systemFlyout.boundingBox())!;
    expect(flyoutBounds.y).toBeGreaterThanOrEqual(0);
    expect(flyoutBounds.y + flyoutBounds.height).toBeLessThanOrEqual(600);
    await captureCaseScreenshot(page, "people-navigation-rail-600-final.png");
    await page.keyboard.press("Escape");
    await page.reload();
    await expect(page.getByRole("button", { name: "Search workspace", exact: true })).toBeEnabled();
    await expect(nav).toHaveAttribute("data-rail", "true");
    const peopleRailTrigger = nav.getByRole("button", { name: "Expand People & organization" });
    await peopleRailTrigger.click();
    await expect(nav.getByRole("group", { name: "People & organization" })).toBeVisible();
    await page.keyboard.press("Escape");
    await expect(peopleRailTrigger).toBeFocused();
    await peopleRailTrigger.click();
    await nav.getByRole("button", { name: "Expand sidebar" }).click();
    await page.setViewportSize({ width: 1280, height: 900 });
  }
  await moreButton.click();
  const more = page.getByRole("dialog", { name: "App launcher", exact: true });
  await expect(more).toBeVisible();
  await expect(more.getByRole("link", { name: /^Calendar(?:\s|$)/ }))
    .toHaveAttribute("href", "/holdings/acme/calendar");
  for (const heading of ["People & organization", "Customers & sales", "Purchasing", "Inventory & quality", "Projects & routines", "Finance", "Administration", "Knowledge"])
    await expect(more.getByRole("heading", { name: heading, exact: true })).toBeVisible();
  await expect(more.getByRole("link", { name: "People", exact: true })).toBeVisible();
  await expect(more.getByRole("link", { name: "People directory", exact: true })).toHaveCount(0);
  await expect(more.getByRole("button", { name: /^(?:Pin|Unpin)/ })).toHaveCount(0);
  const launcherSearch = more.getByRole("searchbox", { name: "Search apps and pages" });
  for (const [query, name, href] of [
    ["roster", /People.*People/, /\/people$/],
    ["suppliers", /Suppliers.*Purchasing/, /\/commerce\?view=parties&kind=supplier$/],
    ["bank reconciliation", /Cash & banking.*Finance/, /\/finance\/bank$/],
    ["contracts", /Legal.*Services/, /\/legal$/],
    ["gpo", /Organization & Access.*Administration/, /\/technology\?view=organization$/],
  ] as const) {
    await launcherSearch.fill(query);
    await expect(more.getByRole("link", { name })).toHaveAttribute("href", href);
  }
  await expect(more.getByRole("heading", { name: "People & organization", exact: true })).toHaveCount(0);
  await launcherSearch.fill("");
  await page.keyboard.press("Escape");
  await expect(moreButton).toBeFocused();
  for (const viewport of info.project.name === "desktop"
    ? [{ width: 768, height: 900 }, { width: 1440, height: 900 }]
    : [{ width: 390, height: 844 }]) {
    await page.setViewportSize(viewport);
    expect(
      await page.evaluate(
        () => document.documentElement.scrollWidth <= window.innerWidth,
      ),
    ).toBe(true);
    await expect(
      page
        .getByRole("navigation", {
          name:
            viewport.width <= 650 ? "Mobile navigation" : "Primary",
        })
        .filter({ visible: true }),
    ).toBeVisible();
    if (viewport.width <= 650) {
      const mobile = page.getByRole("navigation", { name: "Mobile navigation" }).filter({ visible: true });
      await expect(mobile.getByRole("link")).toHaveCount(3);
      await expect(mobile.getByRole("button", { name: "App launcher" })).toBeVisible();
      await expect(mobile.getByRole("button", { name: "Start work" })).toBeVisible();
      for (const destination of ["Today", "My work", "Messages"])
        await expect(mobile.getByRole("link", { name: new RegExp(`^${destination}(?:\\s|$)`) })).toBeVisible();
    }
  }
  await page.getByRole("button", { name: "Search workspace" }).click();
  const dialog = page.getByRole("dialog", { name: "Go to…" });
  await dialog.getByRole("textbox").fill("Finance");
  await dialog.getByRole("link").first().click();
  await expect(page).toHaveURL(/\/finance$/);
  await page.keyboard.press("ControlOrMeta+k");
  await expect(dialog).toBeVisible();
  await page.keyboard.press("Escape");
  await page.locator(".account-trigger").click();
  await page.getByRole("button", { name: "Dark", exact: true }).click();
  await page.keyboard.press("Escape");
  await page.reload();
  await expect(page.locator("html")).toHaveAttribute("data-theme", "dark");
  for (const viewport of info.project.name === "desktop"
    ? [{ width: 768, height: 900 }, { width: 1440, height: 900 }]
    : [{ width: 390, height: 844 }]) {
    await page.setViewportSize(viewport);
    expect(await page.evaluate(() => document.documentElement.scrollWidth <= window.innerWidth)).toBe(true);
  }
  // Inspect authenticated navigation and structure, before opening the Inbox's
  // quarantined attachment fixtures (whose content requests deliberately return 423).
  expect(browserErrors).toEqual([]);
  page.off("console", recordConsoleError);
  await page
    .getByRole("navigation", { name: /^(Primary|Mobile navigation)$/ })
    .filter({ visible: true })
    .getByRole("link", { name: "Messages", exact: true })
    .click();
  await expect(workspace(page).getByRole("heading", { name: "Your inbox" })).toBeVisible();
  expect(browserErrors).toEqual([]);
});
test("POS opens directly from the sidebar, rail, and app launcher", browserProjects(["desktop"], "The case verifies the desktop sidebar and collapsed rail; mobile app launcher access remains required in the navigation case."), async ({ page }) => {

  await page.setViewportSize({ width: 1440, height: 900 });
  await login(page);
  const base = "/holdings/acme";
  const frontHref = "/front/acme/acme-co/pos/holding";
  const frontUrl = new URL("/acme/acme-co/holding", frontOrigin("pos")).toString();
  const ownerAssignment = "dev_usr_jesadakorn_acme-co_holding_owner";
  await page.goto(`${base}/home`);
  const nav = page.getByRole("navigation", { name: "Primary" }).filter({ visible: true });
  await expect(nav.getByRole("button", { name: "Expand POS" })).toBeVisible();
  await nav.getByRole("button", { name: "Expand POS" }).click();
  await expect(nav.getByRole("button", { name: "Collapse POS" })).toHaveAttribute("aria-expanded", "true");
  for (const [label, href] of [
    ["Open POS Front, opens in a new tab", frontHref],
    ["POS approvals", `${base}/pos?tab=history&queue=approval`],
    ["POS reports", `${base}/pos?tab=reports`],
    ["POS settings", `${base}/pos?tab=settings`],
  ]) await expect(nav.getByRole("link", { name: label, exact: true })).toHaveAttribute("href", href);

  async function openFront(link: Locator, firstSignIn: boolean) {
    await expect(link).toHaveAttribute("href", frontHref);
    await expect(link).toHaveAttribute("target", "_blank");
    const [front] = await Promise.all([page.waitForEvent("popup"), link.click()]);
    if (firstSignIn) {
      // Front owns its session cookie. Use the same real account on its sign-in page.
      await expect(front).toHaveURL(new URL("/login?next=%2Facme%2Facme-co%2Fholding", frontOrigin("pos")).toString());
      await expect(front.getByRole("heading", { name: "Staff sign in", exact: true })).toBeVisible();
      await front.getByLabel("Work email").fill(process.env.ERPX_E2E_ADMIN_EMAIL ?? "jesadakorn.kirtnu@gmail.com");
      await front.getByLabel(/^Password(?:\s*\*)?$/).fill(process.env.ERPX_E2E_ADMIN_PASSWORD ?? "12345678");
      const signIn = front.waitForResponse(response => response.url().endsWith("/api/auth/sign-in/email"));
      await front.getByRole("button", { name: "Sign in", exact: true }).click();
      expect((await signIn).status()).toBe(200);
      await expect(front).toHaveURL(frontUrl);
      await expect(front.locator("html")).toHaveAttribute("lang", "th");
      await front.getByRole("button", { name: "EN", exact: true }).click();
      await expect(front.locator("html")).toHaveAttribute("lang", "en");
      await front.locator("summary").filter({ hasText: /^More/ }).click();
      const role = front.getByRole("combobox", { name: "Work role", exact: true });
      await expect(role).toBeEnabled();
      await expect(role.getByRole("option", { includeHidden: true }).filter({ hasText: "holding owner" })).toHaveAttribute("value", ownerAssignment);
      const currentAssignment = await role.inputValue();
      // Selecting the already-active role is a no-op in the normal Front picker.
      if (currentAssignment !== ownerAssignment) {
        const revision = Number(await front.locator("[data-work-revision]:visible").getAttribute("data-work-revision"));
        const selected = front.waitForResponse(response => response.request().method() === "POST" && response.url().endsWith("/pos/roles"));
        await role.selectOption(ownerAssignment);
        const response = await selected;
        expect(response.status()).toBe(200);
        expect(response.request().postDataJSON()).toEqual({ location: "holding", assignmentId: ownerAssignment, revision });
      }
      await expect(front.locator("[data-work-assignment]:visible")).toHaveAttribute("data-work-assignment", ownerAssignment);
      await expect(front.locator("html")).not.toHaveAttribute("data-work-switching", "true");
      if (currentAssignment !== ownerAssignment) {
        // The context commit remounts the ordinary More menu in its closed state.
        const more = front.locator("details").filter({ has: front.locator("summary").filter({ hasText: /^More/ }) });
        await expect(more).toHaveJSProperty("open", false);
        await more.locator("summary").click();
      }
      await expect(role).toHaveValue(ownerAssignment);
      await front.locator("summary").filter({ hasText: /^More/ }).click();
    }
    await expect(front).toHaveURL(frontUrl);
    await expect(front.locator("html")).toHaveAttribute("lang", "en");
    await expect(front.locator("[data-work-assignment]:visible")).toHaveAttribute("data-work-company", "acme-co");
    await expect(front.locator("[data-work-assignment]:visible")).toHaveAttribute("data-work-assignment", ownerAssignment);
    await expect(front.locator("[data-work-assignment]:visible")).toHaveAttribute("data-work-stale", "false");
    await expect(front.getByRole("heading", { name: "Point of sale", exact: true, level: 1 })).toBeVisible();
    return front;
  }

  const sidebarFront = await openFront(nav.getByRole("link", { name: "Open POS Front, opens in a new tab", exact: true }), true);
  try {
    const dailyNavigation = sidebarFront.getByRole("navigation", { name: "Point of sale", exact: true });
    for (const [label, tab] of [["Cash shifts", "shifts"], ["Kitchen", "kitchen"], ["History", "history"]]) {
      const destination = dailyNavigation.getByRole("button", { name: label, exact: true });
      await destination.click();
      await expect(sidebarFront).toHaveURL(`${frontUrl}?tab=${tab}`);
      await expect(destination).toHaveAttribute("aria-current", "page");
      await expect(sidebarFront.getByRole("heading", { name: "Point of sale", exact: true, level: 1 })).toBeVisible();
    }
  } finally {
    await sidebarFront.close();
  }
  await nav.getByRole("link", { name: "POS approvals", exact: true }).click();
  await expect(page).toHaveURL(`${base}/pos?tab=history&queue=approval`);
  await expect(nav.getByRole("link", { name: "POS approvals", exact: true })).toHaveAttribute("aria-current", "page");
  await expect(workspace(page).getByRole("heading", { name: "Point of sale", exact: true, level: 1 })).toBeVisible();
  await page.setViewportSize({ width: 1280, height: 900 });
  await nav.getByRole("button", { name: "Collapse sidebar" }).click();
  await expect(nav).toHaveAttribute("data-rail", "true");
  await expect(nav.getByRole("button", { name: "Expand POS" })).toHaveAttribute("aria-expanded", "false");
  await nav.getByRole("button", { name: "Expand POS" }).click();
  const flyout = nav.getByRole("group", { name: "POS", exact: true });
  await expect(flyout.getByRole("link", { name: "POS approvals", exact: true })).toBeVisible();
  const railFront = await openFront(flyout.getByRole("link", { name: "Open POS Front, opens in a new tab", exact: true }), false);
  await railFront.close();
  await expect(flyout).toBeHidden();
  await nav.getByRole("button", { name: "Expand POS" }).click();
  await nav.getByRole("group", { name: "POS", exact: true }).getByRole("link", { name: "POS settings", exact: true }).click();
  await expect(page).toHaveURL(`${base}/pos?tab=settings`);
  await expect(workspace(page).getByRole("heading", { name: "Point of sale", exact: true, level: 1 })).toBeVisible();
  await nav.getByRole("button", { name: "App launcher" }).click();
  const launcher = page.getByRole("dialog", { name: "App launcher", exact: true });
  await expect(launcher.getByRole("heading", { name: "POS", exact: true })).toBeVisible();
  await expect(launcher.getByRole("link", { name: "POS reports", exact: true })).toHaveAttribute("href", `${base}/pos?tab=reports`);
  const launcherFront = await openFront(launcher.getByRole("link", { name: "Open POS Front, opens in a new tab", exact: true }), false);
  await launcherFront.close();
  await expect(launcher).toBeHidden();
  await nav.getByRole("button", { name: "App launcher" }).click();
  await launcher.getByRole("link", { name: "POS reports", exact: true }).click();
  await expect(page).toHaveURL(`${base}/pos?tab=reports`);
  await expect(workspace(page).getByRole("heading", { name: "Point of sale", exact: true, level: 1 })).toBeVisible();
});
test("breadcrumb returns to live business parents", browserProjects(["desktop"], "The full breadcrumb exists in the desktop shell; mobile uses the compact workspace menu covered by responsive navigation."), async ({ page }) => {
  test.setTimeout(180_000);

  await login(page);
  await page.goto("/holdings/acme/people/organization");
  await expect(page).toHaveURL(/\/people(?:\?|$)/);
  const crumb = page.getByRole("navigation", { name: "Breadcrumb" });
  await expect(crumb).toContainText("People");
  await expect(crumb).not.toContainText("Organization");
});
test("a person row opens profile and employment in one detail panel", async ({
  page,
}, info) => {
  test.setTimeout(180_000);
  await page.setViewportSize({ width: info.project.name === "desktop" ? 1440 : 390, height: 900 });
  await login(page);
  await page.goto("/holdings/acme/people");
  await expect(workspace(page).getByRole("tablist")).toHaveCount(0);
  expect(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth + 1)).toBe(true);
  await workspace(page).getByRole("searchbox", { name: "Search people, positions, and units" }).fill("Ada Bennett");
  await workspace(page).locator('[data-testid^="people-person-"]').filter({ hasText: "Ada Bennett" }).getByRole("button").first().click();
  const drawer = workspace(page).getByTestId("people-detail");
  await expect(drawer).toBeVisible({ timeout: 30000 });
  await expect(drawer.getByRole("heading", { name: "Ada Bennett", exact: true })).toBeVisible();
  await expect(drawer.getByText("Work email", { exact: true })).toBeVisible({ timeout: 30_000 });
  await drawer.locator("summary").filter({ hasText: /^Employment/ }).click();
  await expect(drawer.getByText("Employment type", { exact: true })).toBeVisible({ timeout: 30_000 });
  await drawer.getByRole("button", { name: "Close details", exact: true }).click();
  await expect(drawer).toBeHidden();
  await expect(page).not.toHaveURL(/[?&]person=/);
});
test("live workspace pages fit three widths and both themes", browserProjects(["desktop"], "This case explicitly sweeps mobile, tablet and desktop widths in both themes once; device-specific navigation cases run in both projects."), async ({
  page,
}, info) => {

  test.setTimeout(300000);
  await login(page);
  const routes = [
    "home",
    "work",
    "inbox",
    "people",
    "commerce",
    "finance",
    "projects",
    "settings?section=organization",
    "settings?section=approvals",
  ].map((route) => [route, `/holdings/acme/${route}`]);
  for (const theme of ["light", "dark"])
    for (const width of [390, 768, 1440]) {
      await page.context().addCookies([
        {
          name: "erpx_appearance",
          value: theme,
          url: new URL(page.url()).origin,
        },
      ]);
      await page.setViewportSize({ width, height: 900 });
      for (const [name, path] of routes) {
        await page.goto(path);
        if (name === "commerce") await expect(page).toHaveURL(new URL("/holdings/acme/sales?view=documents&kind=customer&type=sales", page.url()).toString());
        await expect(workspace(page).getByRole("heading", { level: 1 }).first()).toBeVisible();
        await expect(page.locator("html")).toHaveAttribute("data-theme", theme);
        expect(
          await page.evaluate(
            () => document.documentElement.scrollWidth <= innerWidth,
          ),
          `${name}/${theme}/${width}`,
        ).toBeTruthy();
        await page.screenshot({
          path: info.outputPath(
            `${name.replace(/[^a-z-]/g, "-")}-${theme}-${width}.png`,
          ),
        });
      }
    }
});
test("learn retains captioned lessons", async ({ page }) => {
  await login(page);
  await page.goto("/holdings/acme/learn");
  const video = workspace(page).locator("video").first();
  await expect(video).toBeVisible();
  await expect(video.locator("track")).toHaveAttribute("kind", "captions");
  const source = await video.locator("source").getAttribute("src");
  expect((await page.request.get(source!)).ok()).toBeTruthy();
});
test("restricted staff sees only authorized live destinations", async ({
  page,
}, info) => {
  await login(page);
  const endpoint = "/api/v1/holdings/acme/assignments/active";
  const previousResponse = await page.request.get(endpoint);
  expect(previousResponse.status()).toBe(200);
  const previous = (await previousResponse.json()).data as { revision: number; companyId: string; userId: string };
  expect(previous).toMatchObject({ companyId: "acme-co", userId: "usr_jesadakorn" });
  await openAccountMenu(page);
  const roles = await searchRoleOptions(page, "operations manager");
  const operations = roles.getByRole("option").filter({ hasText: "Acme Corporation" }).filter({ hasText: /operations manager/i });
  await expect(operations).toHaveCount(1);
  const selectedRole = page.waitForResponse(response => response.request().method() === "POST" && response.url().endsWith("/assignments/active"));
  await operations.click();
  const selected = await selectedRole;
  expect(selected.status()).toBe(200);
  expect(selected.request().postDataJSON()).toEqual({ assignmentId: "dev_usr_jesadakorn_acme-co_operations_manager", revision: previous.revision });
  const ready = page.locator("[data-work-assignment]:visible");
  await expect(ready).toHaveAttribute("data-work-assignment", "dev_usr_jesadakorn_acme-co_operations_manager");
  await expect(page.locator("html")).not.toHaveAttribute("data-work-switching", "true");
  await expect(page.getByRole("dialog", { name: "Account switcher", exact: true })).toBeHidden();
  const persistedResponse = await page.request.get(endpoint);
  expect(persistedResponse.status()).toBe(200);
  const persisted = (await persistedResponse.json()).data as { valid: boolean; revision: number; companyId: string; userId: string; assignmentId: string };
  expect(persisted).toMatchObject({ valid: true, companyId: "acme-co", userId: "usr_jesadakorn", assignmentId: "dev_usr_jesadakorn_acme-co_operations_manager" });
  expect(persisted.revision).toBeGreaterThan(previous.revision);
  await expect(ready).toHaveAttribute("data-work-company", persisted.companyId);
  await expect(ready).toHaveAttribute("data-work-user", persisted.userId);
  await expect(ready).toHaveAttribute("data-work-revision", String(persisted.revision));
  const nav = page
    .getByRole("navigation", { name: /^(Primary|Mobile navigation)$/ })
    .filter({ visible: true });
  if (info.project.name === "desktop") {
    await nav.getByRole("button", { name: "Expand People & organization", exact: true }).click();
    await expect(nav.getByRole("link", { name: "People", exact: true })).toBeVisible();
    await nav.getByRole("button", { name: "Expand Projects & routines", exact: true }).click();
    await expect(nav.getByRole("link", { name: "Operations", exact: true })).toBeVisible();
    await expect(nav.getByRole("link", { name: "Finance", exact: true })).toHaveCount(0);
    await expect(nav.getByRole("button", { name: /^(?:Expand|Collapse) Finance$/ })).toHaveCount(0);
  } else {
    await nav.getByRole("button", { name: "App launcher", exact: true }).click();
    const more = page.getByRole("dialog", { name: "App launcher", exact: true });
    const appSearch = more.getByRole("searchbox", { name: "Search apps and pages", exact: true });
    await appSearch.fill("People");
    await expect(more.getByRole("link", { name: /^People(?:\s|$)/ })).toBeVisible();
    await appSearch.fill("Operations");
    await expect(more.getByRole("link", { name: /^Operations(?:\s|$)/ })).toBeVisible();
    await appSearch.fill("Finance");
    await expect(more.getByRole("link", { name: /^Finance(?:\s|$)/ })).toHaveCount(0);
    await page.keyboard.press("Escape");
  }
  await page.getByRole("button", { name: "Search workspace" }).click();
  const search = page.getByRole("dialog", { name: "Go to…" });
  await search.getByRole("textbox").fill("Finance");
  await expect(search.getByRole("link")).toHaveCount(0);
  await page.keyboard.press("Escape");
  await page.goto("/holdings/acme/branches/bangkok/inbox");
  await expect(
    workspace(page).getByRole("heading", { name: "Live work notifications" }),
  ).toHaveCount(0);
  await expect(
    workspace(page).getByRole("navigation", { name: "Inbox sections" }),
  ).toBeVisible();
});
test("post drafts survive failures and collapsed UI; dialogs retain keyboard focus", async ({
  page,
}) => {
  await login(page);
  await page.goto("/holdings/acme/feed");
  const openComposer = workspace(page).locator("details.beta-composer > summary");
  await openComposer.click();
  const composer = page.getByRole("dialog", { name: "Share an update", exact: true });
  await expect(composer).toBeVisible();
  const input = composer.getByLabel("Post message", { exact: true }),
    draft = "A detailed handover for tomorrow. ".repeat(60);
  await expect(input).toHaveAttribute("contenteditable", "true");
  await input.fill(draft);
  await page.route("**/beta/drafts", (r) =>
    r.request().method() === "POST"
      ? r.fulfill({
          status: 503,
          contentType: "application/json",
          body: JSON.stringify({
            error: { message: "Drafts temporarily unavailable." },
          }),
        })
      : r.continue(),
  );
  await composer.getByRole("button", { name: "Save draft", exact: true }).click();
  await expect(
    composer
      .getByRole("status")
      .filter({ hasText: "Drafts temporarily unavailable" }),
  ).toBeVisible();
  await expect(input).toHaveText(draft);
  await composer.getByRole("button", { name: "Close dialog", exact: true }).click();
  const unsaved = page.getByRole("dialog", { name: "Unsaved changes", exact: true });
  if (await unsaved.isVisible()) {
    await unsaved.getByRole("button", { name: "Close without saving", exact: true }).click();
    await expect(unsaved).toBeHidden();
  }
  await expect(composer).not.toBeVisible();
  await expect(openComposer).toBeFocused();
  await openComposer.click();
  await expect(composer).toBeVisible();
  await expect(input).toHaveText(draft);
  await composer.getByRole("button", { name: "Close dialog", exact: true }).click();
  if (await unsaved.isVisible()) {
    await unsaved.getByRole("button", { name: "Close without saving", exact: true }).click();
    await expect(unsaved).toBeHidden();
  }
  await expect(composer).not.toBeVisible();
  const trigger = page.getByRole("button", { name: "Search workspace" });
  await trigger.click();
  const dialog = page.getByRole("dialog", { name: "Go to…" });
  for (let i = 0; i < 8; i++) {
    await page.keyboard.press("Tab");
    expect(
      await dialog.evaluate((el) => el.contains(document.activeElement)),
    ).toBeTruthy();
  }
  await page.keyboard.press("Escape");
  await expect(trigger).toBeFocused();
  await page.emulateMedia({ reducedMotion: "reduce", colorScheme: "dark" });
  await page.context().addCookies([
    {
      name: "erpx_appearance",
      value: "system",
      url: new URL(page.url()).origin,
    },
  ]);
  await page.reload();
  expect(
    await page
      .locator("html")
      .evaluate((el) => getComputedStyle(el).colorScheme),
  ).toBe("dark");
});
test("Administration consolidates tools without granting staff approval management", async ({
  page,
  browser,
}, info) => {

  await login(page);
  await page.goto("/holdings/acme/settings?section=organization");
  await expect(page).toHaveURL("/holdings/acme/people?company=acme-co&tab=settings");
  await expect(workspace(page).getByRole("heading", { name: "People", exact: true, level: 1 })).toBeVisible();
  if (info.project.name === "mobile") await workspace(page).getByRole("dialog", { name: "Acme Corporation details", exact: true }).getByRole("button", { name: "Close details", exact: true }).click();
  await workspace(page).getByRole("button", { name: "HR settings", exact: true }).click();
  const hrSettings = workspace(page).getByTestId("people-detail");
  await hrSettings.getByRole("navigation", { name: "HR settings topics", exact: true })
    .getByRole("button", { name: /^Staff levels/ }).click();
  await expect(
    hrSettings.getByRole("heading", { name: "Staff levels", exact: true }),
  ).toBeVisible();
  await expect(
    workspace(page).getByRole("heading", { name: "Assign current level" }),
  ).toHaveCount(0);
  await expect(
    workspace(page).getByRole("heading", { name: "Approval routing administration" }),
  ).toHaveCount(0);
  await hrSettings.getByRole("button", { name: "Close details", exact: true }).click();
  await page.getByRole("navigation", { name: /^(Primary|Mobile navigation)$/ }).filter({ visible: true })
    .getByRole("button", { name: "App launcher", exact: true }).click();
  const ownerLauncher = page.getByRole("dialog", { name: "App launcher", exact: true });
  await ownerLauncher.getByRole("searchbox", { name: "Search apps and pages", exact: true }).fill("Approval routing");
  const approvalRouting = ownerLauncher.getByRole("link", { name: /^Approval routing(?:\s|$)/ });
  await expect(approvalRouting).toHaveAttribute("href", "/holdings/acme/settings?section=approvals");
  await approvalRouting.click();
  await workspace(page).getByRole("navigation", { name: "Approval routing views", exact: true }).getByRole("link", { name: "Routing issues", exact: true }).click();
  await expect(
    workspace(page).getByRole("heading", { name: "Approval issues", exact: true }),
  ).toBeVisible();
  await expect(
    page
      .locator("#workspace-content:visible")
      .getByRole("link", { name: "Routine templates", exact: true }),
  ).toBeVisible();
  await page.goto("/holdings/acme/settings?section=people");
  await expect(page).toHaveURL("/holdings/acme/people?panel=settings");
  await expect(
    workspace(page).getByRole("heading", { name: "People", exact: true, level: 1 }),
  ).toBeVisible();
  await expect(workspace(page).getByTestId("people-detail").getByRole("heading", { name: "HR settings", exact: true })).toBeVisible();
  await expect(workspace(page).getByRole("combobox", { name: "Teammate" })).toHaveCount(0);
  const c = await browser.newContext(info.project.use);
  try {
    await c.request.post("/api/auth/sign-in/email", {
      data: { email: "noah@erpx.dev", password: "Noah-ERPX-2026!" },
    });
    const p = await c.newPage();
    await p.goto("/");
    await expect(p).toHaveURL(/branches\/new-york\/home$/);
    const primaryNavigation = p
      .getByRole("navigation", { name: /^(Primary|Mobile navigation)$/ })
      .filter({ visible: true });
    await primaryNavigation.getByRole("button", { name: "App launcher", exact: true }).click();
    const staffLauncher = p.getByRole("dialog", { name: "App launcher", exact: true });
    const appSearch = staffLauncher.getByRole("searchbox", { name: "Search apps and pages", exact: true });
    await appSearch.fill("Approval routing");
    await expect(staffLauncher.getByRole("link", { name: /^Approval routing(?:\s|$)/ })).toHaveCount(0);
    await appSearch.fill("Help using ERPX");
    await expect(staffLauncher.getByRole("link", { name: /^Help using ERPX(?:\s|$)/ }))
      .toHaveAttribute("href", "/holdings/acme/branches/new-york/settings?section=feedback");
    await appSearch.fill("Routine templates");
    const staffRoutines = staffLauncher.getByRole("link", { name: /^Routine templates(?:\s|$)/ });
    await expect(staffRoutines).toHaveAttribute("href", "/holdings/acme/branches/new-york/routine-templates");
    await staffRoutines.click();
    await expect(
      workspace(p).getByRole("heading", { name: "Routine templates", exact: true, level: 1 }),
    ).toBeVisible();
    await expect(
      workspace(p).getByRole("heading", { name: "Approval issues", exact: true }),
    ).toHaveCount(0);
    await p.goto(
      "/holdings/acme/branches/new-york/settings?section=approvals&kind=leave&record=leave_001",
    );
    await expect(
      p.getByRole("heading", { name: "Your role cannot open this view", exact: true, level: 1 }),
    ).toBeVisible();
    await expect(
      p.getByRole("button", { name: "Record rerouting" }),
    ).toHaveCount(0);
  } finally {
    await c.close();
  }
});
