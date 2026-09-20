import { expect, test } from "@playwright/test";

const base = "/holdings/acme";

test.beforeEach(async ({ context, baseURL }) => {
  test.skip(
    !["localhost", "127.0.0.1"].includes(new URL(baseURL!).hostname),
    "Uses the existing local test account.",
  );
  const response = await context.request.post("/api/auth/sign-in/email", {
    data: { email: "mali@erpx.dev", password: "Mali-ERPX-2026!" },
  });
  expect(response.status()).toBe(200);
});

test("calendar renders one responsive active view without layout customization", async ({
  page,
}, testInfo) => {
  await page.goto(`${base}/calendar`);
  const workspace = page.locator("[data-calendar-workspace]");
  await expect(workspace).toHaveCount(1);
  await expect(page.getByRole("button", { name: "Customize layout" })).toHaveCount(0);

  const expected = testInfo.project.name === "mobile" ? "agenda" : "week";
  await expect(
    page.getByRole("group", { name: "Calendar view" }).getByRole("button", {
      name: expected[0].toUpperCase() + expected.slice(1),
    }),
  ).toHaveAttribute("aria-pressed", "true");
  await expect(page.locator("[data-calendar-view]")).toHaveCount(1);
  await expect(page.locator(`[data-calendar-view="${expected}"]`)).toBeVisible();
  if (expected === "agenda") await expect(page).toHaveURL(/(?:\?|&)view=agenda(?:&|$)/);

  await expect(page.getByRole("link", { name: "My calendar" })).toBeVisible();
  await expect(page.getByRole("link", { name: "Team calendar" })).toBeVisible();
  await expect(page.getByRole("link", { name: "Coverage" })).toBeVisible();
  expect(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth + 1)).toBe(true);
});

test("calendar view and filters follow URL history and event detail retains the view", async ({
  page,
}) => {
  await page.goto(`${base}/calendar?view=week`);
  const views = page.getByRole("group", { name: "Calendar view" });
  const appointments = page.getByLabel("Appointments", { exact: true });

  await page.getByText("Filters · layers, timezone and hierarchy").click();
  await appointments.click();
  await expect(page).not.toHaveURL(/layers=[^&]*appointments/);

  await views.getByRole("button", { name: "Month" }).click();
  await expect(page).toHaveURL(/(?:\?|&)view=month(?:&|$)/);
  await expect(page.locator('[data-calendar-view="month"]')).toBeVisible();
  await expect(appointments).not.toBeChecked();

  await views.getByRole("button", { name: "Agenda" }).click();
  await expect(page).toHaveURL(/(?:\?|&)view=agenda(?:&|$)/);
  await page.goBack();
  await expect(views.getByRole("button", { name: "Month" })).toHaveAttribute(
    "aria-pressed",
    "true",
  );
  await expect(appointments).not.toBeChecked();

  await views.getByRole("button", { name: "Week" }).click();
  const event = page.locator("[data-calendar-event]").first();
  await expect(event).toBeVisible();
  await event.click();
  await expect(page).toHaveURL(/(?:\?|&)view=week(?:&|$)/);
  await expect(page).toHaveURL(/(?:\?|&)event=[^&]+/);
  await expect(page.locator("dialog[open]")).toHaveCount(1);
  await page.goBack();
  await expect(page.locator("dialog[open]")).toHaveCount(0);
  await expect(views.getByRole("button", { name: "Week" })).toHaveAttribute(
    "aria-pressed",
    "true",
  );
  await page.goForward();
  await expect(page.locator("dialog[open]")).toHaveCount(1);
  await expect(page).toHaveURL(/(?:\?|&)event=[^&]+/);
});

test("day view renders a horizontally scrollable people timeline and opens event details", async ({ page }) => {
  await page.goto(`${base}/calendar?view=day&date=2026-09-18`);
  const timeline = page.locator('[data-calendar-view="day"]');
  await expect(timeline).toBeVisible();
  await expect(page.getByRole("button", { name: "Day", exact: true })).toHaveAttribute("aria-pressed", "true");
  await expect(timeline.locator("[data-calendar-event]").first()).toBeVisible();
  await timeline.locator("[data-calendar-event]").first().click();
  await expect(page).toHaveURL(/(?:\?|&)event=[^&]+/);
  await expect(page.locator("dialog[open]")).toHaveCount(1);
  expect(await timeline.evaluate((element) => element.scrollWidth > element.clientWidth)).toBe(true);
});
