import { test, expect, type Page } from "@playwright/test";
test.use({ actionTimeout: 15000 });
async function login(page: Page) {
  await page.goto("/login");
  await page.getByLabel("Work email").fill(
    process.env.ERPX_E2E_ADMIN_EMAIL ?? "jesadakorn.kirtnu@gmail.com",
  );
  await page.getByLabel("Password", { exact: true }).fill(
    process.env.ERPX_E2E_ADMIN_PASSWORD ?? "12345678",
  );
  const signIn = page.waitForResponse((response) =>
    response.url().endsWith("/api/auth/sign-in/email"),
  );
  await page.getByRole("button", { name: "Sign in", exact: true }).click();
  expect((await signIn).ok()).toBe(true);
  await page.goto("/holdings/acme/home");
  await expect(page).toHaveURL(/\/holdings\/acme\/home$/);
  if (!(await page.getByText(/Working as holding owner/i).count())) {
    await page.getByRole("button", { name: /Account menu for/ }).click();
    const role = page.getByRole("combobox", {
      name: "Role for actions & approvals",
    });
    await role.fill("holding owner");
    await Promise.all([
      page.waitForEvent("load"),
      page.getByRole("listbox").getByRole("option").click(),
    ]);
  }
}
test("canonical workspace routes and legacy aliases retain their scope", async ({
  page,
}, info) => {
  test.skip(
    info.project.name !== "desktop",
    "The same route contract is checked once.",
  );
  await login(page);
  for (const path of [
    "/platform/home",
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
  await expect(page).toHaveURL(/settings\?section=organization$/);
  expect((await page.request.get("/h/h001/home")).status()).toBe(404);
});
test("development module and generic record UI routes are unavailable without redirects", async ({
  page,
}, info) => {
  test.skip(
    info.project.name !== "desktop",
    "Route coverage is independent of viewport.",
  );
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
  if (info.project.name === "desktop")
    await page.setViewportSize({ width: 1440, height: 650 });
  await login(page);
  const nav = page
    .getByRole("navigation", { name: /^(Primary|Mobile navigation)$/ })
    .filter({ visible: true });
  for (const destination of ["Home", "Work", "Inbox"])
    await expect(
      nav.getByRole("link", { name: new RegExp(`^${destination}(?:\\s|$)`) }),
    ).toBeVisible();
  for (const destination of ["People", "Operations", "Finance", "Projects"])
    await expect(nav.getByRole("link", { name: destination, exact: true })).toHaveCount(info.project.name === "desktop" ? 1 : 0);
  await expect(
    nav.getByRole("link", { name: "Inbox", exact: true }),
  ).toBeVisible();
  const moreButton = nav.getByRole("button", { name: "More navigation" });
  if (info.project.name === "desktop")
    await expect(moreButton).toHaveAttribute("aria-expanded", "true");
  else await moreButton.click();
  const more =
    info.project.name === "desktop"
      ? nav
      : page.getByRole("dialog", { name: "More", exact: true });
  for (const destination of ["People", "Operations", "Finance", "Projects"])
    await expect(
      more.getByRole("link", { name: destination, exact: true }),
    ).toHaveCount(1);
  await expect(
    more.getByRole("button", { name: "Collapse People" }),
  ).toHaveAttribute("aria-expanded", "true");
  const peopleParentIcon = more
    .getByRole("link", { name: "People", exact: true })
    .locator("svg")
    .first();
  const organizationChildIcon = more
    .getByRole("link", { name: "Organization", exact: true })
    .locator("svg")
    .first();
  await expect(peopleParentIcon).toHaveAttribute("width", "18");
  await expect(organizationChildIcon).toHaveAttribute("width", "15");
  await expect(organizationChildIcon).toHaveAttribute("stroke-width", "1.7");
  await more.getByRole("button", { name: "Collapse People" }).click();
  await expect(
    more.getByRole("button", { name: "Expand People" }),
  ).toHaveAttribute("aria-expanded", "false");
  await more.getByRole("button", { name: "Expand People" }).click();
  await expect(
    more.getByRole("link", { name: "Administration" }),
  ).toBeVisible();
  await expect(
    more.getByRole("link", { name: "Updates", exact: true }),
  ).toHaveCount(0);
  if (info.project.name === "desktop") {
    await moreButton.click();
    await expect(moreButton).toHaveAttribute("aria-expanded", "false");
    await expect(
      nav.getByRole("link", { name: "Administration" }),
    ).toHaveCount(0);
    await page.waitForTimeout(300);
    await expect(moreButton).toHaveCount(1);
    await expect(moreButton).toHaveAttribute("aria-expanded", "false");
    await expect(
      nav.getByRole("link", { name: "Administration" }),
    ).toHaveCount(0);
    await moreButton.click();
    await expect(moreButton).toHaveAttribute("aria-expanded", "true");
    await expect(
      nav.getByRole("link", { name: "Administration" }),
    ).toBeVisible();
    await page.setViewportSize({ width: 1440, height: 2200 });
    await expect(moreButton).toHaveCount(0);
    await expect(
      nav.getByRole("link", { name: "Administration" }),
    ).toBeVisible();
  } else {
    await page.keyboard.press("Escape");
    await expect(moreButton).toBeFocused();
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
  await page
    .getByRole("navigation", { name: /^(Primary|Mobile navigation)$/ })
    .filter({ visible: true })
    .getByRole("link", { name: "Inbox", exact: true })
    .click();
  await expect(page.getByRole("heading", { name: "Your inbox" })).toBeVisible();
});
test("breadcrumb returns to live business parents", async ({ page }, info) => {
  test.skip(
    info.project.name !== "desktop",
    "Mobile uses the compact workspace menu.",
  );
  await login(page);
  await page.goto("/holdings/acme/people/organization");
  const crumb = page.getByRole("navigation", { name: "Breadcrumb" });
  await expect(crumb).toContainText("People");
  await expect(crumb).toContainText("Organization");
});
test("live workspace pages fit three widths and both themes", async ({
  page,
}, info) => {
  test.skip(
    info.project.name !== "desktop",
    "This matrix explicitly includes mobile widths.",
  );
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
        await expect(page.locator("h1").first()).toBeVisible();
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
  const video = page.locator("video").first();
  await expect(video).toBeVisible();
  await expect(video.locator("track")).toHaveAttribute("kind", "captions");
  const source = await video.locator("source").getAttribute("src");
  expect((await page.request.get(source!)).ok()).toBeTruthy();
});
test("restricted staff sees only authorized live destinations", async ({
  page,
}, info) => {
  await login(page);
  await page.getByRole("button", { name: /Account menu for/ }).click();
  const role = page.getByRole("combobox", {
    name: "Role for actions & approvals",
  });
  await role.fill("operations manager");
  await Promise.all([
    page.waitForEvent("load"),
    page.getByRole("listbox").getByRole("option").click(),
  ]);
  const nav = page
    .getByRole("navigation", { name: /^(Primary|Mobile navigation)$/ })
    .filter({ visible: true });
  if (info.project.name === "desktop") {
    await expect(nav.getByRole("link", { name: "People", exact: true })).toBeVisible();
    await expect(nav.getByRole("link", { name: "Operations", exact: true })).toBeVisible();
    await expect(nav.getByRole("link", { name: "Finance", exact: true })).toHaveCount(0);
  } else {
    await nav.getByRole("button", { name: "More navigation" }).click();
    const more = page.getByRole("dialog", { name: "More", exact: true });
    await expect(more.getByRole("link", { name: "People", exact: true })).toBeVisible();
    await expect(more.getByRole("link", { name: "Operations", exact: true })).toBeVisible();
    await expect(more.getByRole("link", { name: "Finance", exact: true })).toHaveCount(0);
    await page.keyboard.press("Escape");
  }
  await page.getByRole("button", { name: "Search workspace" }).click();
  const search = page.getByRole("dialog", { name: "Go to…" });
  await search.getByRole("textbox").fill("Finance");
  await expect(search.getByRole("link")).toHaveCount(0);
  await page.keyboard.press("Escape");
  await page.goto("/holdings/acme/branches/bangkok/inbox");
  await expect(
    page.getByRole("heading", { name: "Live work notifications" }),
  ).toHaveCount(0);
  await expect(
    page.getByRole("navigation", { name: "Inbox sections" }),
  ).toBeVisible();
});
test("post drafts survive failures and collapsed UI; dialogs retain keyboard focus", async ({
  page,
}) => {
  await login(page);
  await page.goto("/holdings/acme/feed");
  await page.getByText("Share an update", { exact: true }).click();
  const input = page.getByLabel("Post message", { exact: true }),
    draft = "A detailed handover for tomorrow. ".repeat(60);
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
  await page.getByRole("button", { name: "Save draft", exact: true }).click();
  await expect(
    page
      .getByRole("status")
      .filter({ hasText: "Drafts temporarily unavailable" }),
  ).toBeVisible();
  await expect(input).toHaveValue(draft);
  await page.getByText("Share an update", { exact: true }).click();
  await expect(input).not.toBeVisible();
  await page.getByText("Share an update", { exact: true }).click();
  await expect(input).toHaveValue(draft);
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
  test.skip(
    info.project.name !== "desktop",
    "Administration policy is viewport-independent; mobile hierarchy is covered separately.",
  );
  await login(page);
  await page.goto("/holdings/acme/settings?section=organization");
  await expect(
    page.getByRole("heading", { name: "Company level catalog" }),
  ).toBeVisible();
  await expect(
    page.getByRole("heading", { name: "Assign current level" }),
  ).toHaveCount(0);
  await expect(
    page.getByRole("heading", { name: "Approval routing administration" }),
  ).toHaveCount(0);
  await page
    .getByRole("navigation", { name: "Administration sections" })
    .getByRole("link", { name: "Approvals & routines", exact: true })
    .click();
  await expect(
    page.getByRole("heading", { name: "Approval issues", exact: true }),
  ).toBeVisible();
  await expect(
    page
      .locator("#workspace-content")
      .getByRole("link", { name: "Routine templates", exact: true }),
  ).toBeVisible();
  await page.goto("/holdings/acme/settings?section=people");
  await expect(
    page.getByRole("link", { name: "Open people directory" }),
  ).toBeVisible();
  await expect(page.getByRole("combobox", { name: "Teammate" })).toHaveCount(0);
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
    const secondaryToggle = primaryNavigation.getByRole("button", {
      name: "More navigation",
    });
    if (info.project.name !== "desktop") await secondaryToggle.click();
    const secondaryNavigation =
      info.project.name === "desktop"
        ? primaryNavigation
        : p.getByRole("dialog", { name: "More", exact: true });
    await expect(
      secondaryNavigation.getByRole("link", {
        name: "Administration",
        exact: true,
      }),
    ).toBeVisible();
    await expect(
      secondaryNavigation.getByRole("link", { name: "Help & feedback" }),
    ).toBeVisible();
    await p.getByRole("link", { name: "Administration", exact: true }).click();
    await expect(
      p.getByRole("link", { name: "Routine templates", exact: true }),
    ).toBeVisible();
    await expect(
      p.getByRole("heading", { name: "Approval issues", exact: true }),
    ).toHaveCount(0);
    await p.goto(
      "/holdings/acme/branches/new-york/settings?section=approvals&kind=leave&record=leave_001",
    );
    await expect(
      p.getByRole("heading", { name: "This view isn’t available" }),
    ).toBeVisible();
    await expect(
      p.getByRole("button", { name: "Record rerouting" }),
    ).toHaveCount(0);
  } finally {
    await c.close();
  }
});
