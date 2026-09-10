import { test, expect, type Page } from "@playwright/test";
import {
  moduleRegistry,
  moduleHref,
  liveDestination,
  type WorkspaceKind,
} from "../../src/data/module-registry";
import samples from "../../src/data/preview/module-samples.json";
test.use({ actionTimeout: 15000 });
async function verifyResponsibility(page: Page, role: RegExp) {
  await page
    .getByRole("button", { name: "Workspace menu", exact: true })
    .click();
  const menu = page.getByRole("dialog", { name: "Workspace", exact: true });
  await expect(menu.getByText(role, { exact: true })).toBeVisible();
  await expect(
    menu.getByRole("link", { name: "All responsibilities", exact: true }),
  ).toBeVisible();
  await page.keyboard.press("Escape");
}
async function login(page: Page, owner = true) {
  await page.goto("/login");
  await page.getByLabel("Work email").fill("mali@erpx.dev");
  await page.getByLabel("Password", { exact: true }).fill("Mali-ERPX-2026!");
  await page.getByRole("button", { name: "Sign in", exact: true }).click();
  await expect(page).toHaveURL(/\/holdings\/acme\/home$/);
  if (owner) await verifyResponsibility(page, /holding owner/i);
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
    "/holdings/acme/departments/finance/home",
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
test("all 31 module entry points, live capabilities and sample records resolve", async ({
  page,
}, info) => {
  test.skip(
    info.project.name !== "desktop",
    "Route coverage is independent of viewport.",
  );
  test.setTimeout(300000);
  await login(page);
  for (const tool of moduleRegistry) {
    expect(
      (await page.request.get(`/holdings/acme/modules/${tool.id}`)).ok(),
      tool.id,
    ).toBeTruthy();
    for (const section of tool.sections.filter((s) => s.capability)) {
      const response = await page.request.get(
        moduleHref("/holdings/acme", tool, section.id),
      );
      expect(response.ok(), `${tool.id}/${section.id}`).toBeTruthy();
      if (!liveDestination(tool.id, section.id)) {
        const html = await response.text();
        expect(html).toContain(`data-section="${section.id}"`);
        expect(html).toContain('data-source="live"');
        expect(html).not.toContain("Persisted in local D1");
      }
    }
    const record = samples[tool.id as keyof typeof samples].rows[0][0];
    for (const view of ["details", "form", "review"]) {
      const response = await page.request.get(
        `/holdings/acme/preview/records/${tool.id}/${encodeURIComponent(record)}?view=${view}`,
      );
      expect(response.ok()).toBeTruthy();
      expect(await response.text()).toContain('data-source="sample"');
    }
  }
  await page.goto("/holdings/acme/modules/missing/unknown");
  await expect(
    page.getByRole("heading", { name: "This view isn’t available" }),
  ).toBeVisible();
});
test("browsing filters and preview pages do not issue business mutations", async ({
  page,
}) => {
  await login(page);
  const writes: string[] = [];
  page.on("request", (r) => {
    if (r.url().includes("/api/v1/") && !["GET", "HEAD"].includes(r.method()))
      writes.push(r.url());
  });
  const tool = moduleRegistry.find((m) => m.id === "organization")!,
    section = tool.sections[0];
  await page.goto(moduleHref("/holdings/acme", tool, section.id));
  await page.getByRole("searchbox").fill("no-such-record");
  await page.getByRole("button", { name: "Apply", exact: true }).click();
  await expect(page).toHaveURL(/q=no-such-record/);
  await page.goBack();
  await page.goto(
    `/holdings/acme/preview/records/finance/${encodeURIComponent(samples.finance.rows[0][0])}?view=form`,
  );
  await expect(page.locator('main[data-source="sample"]')).toBeVisible();
  expect(writes).toEqual([]);
});
test("four primary destinations, More, search and account appearance work with keyboard", async ({
  page,
}) => {
  await login(page);
  const nav = page
    .getByRole("navigation", { name: /^(Primary|Mobile navigation)$/ })
    .filter({ visible: true });
  await expect(nav.getByRole("link")).toHaveCount(3);
  await expect(
    nav.getByRole("link", { name: "Inbox", exact: true }),
  ).toBeVisible();
  await nav.getByRole("button", { name: "More navigation" }).click();
  const more = page.getByRole("dialog", { name: "More", exact: true });
  await expect(
    more.getByRole("link", { name: "Administration" }),
  ).toBeVisible();
  await expect(
    more.getByRole("link", { name: "Updates", exact: true }),
  ).toHaveCount(0);
  await page.keyboard.press("Escape");
  await expect(nav.getByRole("button")).toBeFocused();
  await page.getByRole("button", { name: "Search workspace" }).click();
  const dialog = page.getByRole("dialog", { name: "Go to…" });
  await dialog.getByRole("textbox").fill("bank reconciliation");
  await dialog.getByRole("link").first().click();
  await expect(page).toHaveURL(/finance\/bank/);
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
test("breadcrumb returns to module parents", async ({ page }, info) => {
  test.skip(
    info.project.name !== "desktop",
    "Mobile uses the compact workspace menu.",
  );
  await login(page);
  const m = moduleRegistry.find((m) => m.id === "organization")!;
  await page.goto(moduleHref("/holdings/acme", m, m.primary[0]));
  const crumb = page.getByRole("navigation", { name: "Breadcrumb" });
  await expect(crumb).toContainText(m.name);
  await crumb.getByRole("link", { name: "Modules", exact: true }).click();
  await expect(page).toHaveURL(/\/modules$/);
});
test("nine workspace layouts and daily administration pages fit three widths and both themes", async ({
  page,
}, info) => {
  test.skip(
    info.project.name !== "desktop",
    "This matrix explicitly includes mobile widths.",
  );
  test.setTimeout(300000);
  await login(page);
  const representatives = new Map<WorkspaceKind, string>();
  for (const m of moduleRegistry)
    for (const section of m.sections)
      if (
        !liveDestination(m.id, section.id) &&
        !representatives.has(section.kind)
      )
        representatives.set(
          section.kind,
          moduleHref("/holdings/acme", m, section.id),
        );
  expect(representatives.size).toBe(9);
  const routes = [
    ...representatives.entries(),
    ...[
      "home",
      "work",
      "people",
      "inbox",
      "settings?section=organization",
      "settings?section=approvals",
    ].map((x) => [x, `/holdings/acme/${x}`]),
  ];
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
test("module directory filters are shareable and inaccessible tools are absent", async ({
  page,
}) => {
  await login(page);
  await page.goto("/holdings/acme/modules");
  await page.getByLabel("Search modules").fill("finance");
  await page.getByRole("button", { name: "Search", exact: true }).click();
  await expect(page).toHaveURL(/q=finance/);
  await expect(
    page.getByRole("heading", { name: "Finance", exact: true }),
  ).toBeVisible();
  await expect(
    page.getByRole("heading", { name: "Inventory", exact: true }),
  ).toHaveCount(0);
  await page.context().clearCookies();
  const memberLogin = await page.request.post("/api/auth/sign-in/email", {
    data: { email: "ava@erpx.dev", password: "Ava-ERPX-2026!" },
  });
  expect(memberLogin.status(), await memberLogin.text()).toBe(200);
  await page.goto("/holdings/acme/branches/bangkok/modules");
  await expect(page.getByRole("button", { name: /Account menu for Ava/ })).toBeVisible();
  await expect(
    page.getByRole("link", { name: /People & Payroll/ }),
  ).toBeVisible();
  await expect(page.getByRole("link", { name: /Finance/ })).toHaveCount(0);
  await expect(page.getByText("No access", { exact: false })).toHaveCount(0);
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
    page.getByRole("navigation", { name: "Inbox views" }),
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
    .getByRole("link", { name: "Approvals & routines", exact: true })
    .click();
  await expect(
    page.getByRole("heading", { name: "Approval issues", exact: true }),
  ).toBeVisible();
  await expect(
    page.getByRole("link", { name: "Routine templates", exact: true }),
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
    await p
      .getByRole("button", { name: "More navigation" })
      .filter({ visible: true })
      .click();
    await expect(
      p
        .getByRole("dialog", { name: "More", exact: true })
        .getByRole("link", { name: "Administration", exact: true }),
    ).toBeVisible();
    await expect(
      p
        .getByRole("dialog", { name: "More", exact: true })
        .getByRole("link", { name: "Help & feedback" }),
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
