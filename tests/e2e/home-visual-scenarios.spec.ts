import { expect, test } from "@playwright/test";
import { mkdir } from "node:fs/promises";

const picture = '<svg xmlns="http://www.w3.org/2000/svg" width="640" height="420"><rect width="640" height="420" fill="#b7c7db"/><circle cx="320" cy="210" r="92" fill="#6e91bf"/></svg>';

test("Home layout with empty, long and multi-image feed content", async ({ page, context }) => {
  const email = process.env.ERPX_REVIEW_EMAIL;
  const password = process.env.ERPX_REVIEW_PASSWORD;
  test.skip(!email || !password, "Development review account required.");
  await page.goto("/login?next=%2Fholdings%2Facme%2Fhome");
  await page.getByRole("textbox", { name: /Work email/ }).fill(email!);
  await page.getByLabel("Password").fill(password!);
  await page.getByRole("button", { name: "Sign in" }).click();
  await page.waitForURL("**/holdings/acme/home");
  await mkdir(".scratch/dev/home-scenarios", { recursive: true });
  await page.screenshot({ path: ".scratch/dev/home-scenarios/home-top-desktop-light.png" });
  const response = await context.request.get("/api/v1/holdings/acme/beta/home?filter=all&limit=8");
  expect(response.ok()).toBe(true);
  const payload = await response.json() as { data: Array<Record<string, unknown>> };
  expect(payload.data.length).toBeGreaterThan(0);
  const sample = payload.data[0];
  const longPost = {
    ...sample,
    id: "home-visual-review",
    body: `A thoughtful handoff for the whole team.\n\n${"A long uninterrupted reference must wrap safely across every screen. ".repeat(7)}\n\n${"LongReference".repeat(18)}`,
    mentions: [],
    files: [1, 2, 3].map(number => ({ id: `home-review-${number}`, original_name: `Project image ${number}.png` })),
  };
  let empty = true;
  await page.route("**/api/v1/holdings/acme/beta/home?**", route => route.fulfill({
    status: 200,
    contentType: "application/json",
    body: JSON.stringify({ data: empty ? [] : [longPost] }),
  }));
  await page.route(/home-review-[123]\/content/, route => route.fulfill({ status: 200, contentType: "image/svg+xml", body: picture }));

  await page.setViewportSize({ width: 390, height: 844 });
  await page.evaluate(() => window.scrollTo(0, 0));
  await page.screenshot({ path: ".scratch/dev/home-scenarios/home-top-mobile-light.png" });
  const composer = page.locator("details.beta-composer");
  await composer.locator("summary").first().click();
  await expect(composer.getByLabel("Post message", { exact: true })).toBeVisible();
  await composer.scrollIntoViewIfNeeded();
  await page.screenshot({ path: ".scratch/dev/home-scenarios/composer-mobile-light.png" });
  await composer.locator("summary").first().click();
  await page.getByRole("button", { name: "Following", exact: true }).click();
  await expect(page.getByText("No conversations here yet")).toBeVisible();
  await page.getByRole("heading", { name: "Feed", exact: true }).scrollIntoViewIfNeeded();
  await page.screenshot({ path: ".scratch/dev/home-scenarios/empty-mobile-light.png" });

  empty = false;
  await page.getByRole("button", { name: "Latest updates", exact: true }).click();
  await expect(page.getByText("LongReference", { exact: false })).toBeVisible();
  await expect(page.locator(".beta-images img")).toHaveCount(3);
  for (const [name, width, height] of [["mobile", 390, 844], ["tablet", 1024, 768], ["desktop", 1440, 900]] as const) {
    await page.setViewportSize({ width, height });
    await expect.poll(() => page.evaluate(() => document.documentElement.scrollWidth <= innerWidth)).toBe(true);
    await page.locator("article#home-visual-review").scrollIntoViewIfNeeded();
    await page.screenshot({ path: `.scratch/dev/home-scenarios/${name}-light.png` });
  }
  await page.goto("/login");
  await page.getByRole("combobox", { name: "Theme" }).selectOption("dark");
  await page.goto("/holdings/acme/home");
  empty = false;
  await page.setViewportSize({ width: 390, height: 844 });
  await page.evaluate(() => window.scrollTo(0, 0));
  await page.screenshot({ path: ".scratch/dev/home-scenarios/home-top-mobile-dark.png" });
  const darkComposer = page.locator("details.beta-composer");
  await darkComposer.locator("summary").first().click();
  await expect(darkComposer.getByLabel("Post message", { exact: true })).toBeVisible();
  await darkComposer.scrollIntoViewIfNeeded();
  await page.screenshot({ path: ".scratch/dev/home-scenarios/composer-mobile-dark.png" });
  await darkComposer.locator("summary").first().click();
  await page.getByRole("button", { name: "Following", exact: true }).click();
  await expect(page.locator("article#home-visual-review")).toBeVisible();
  for (const [name, width, height] of [["mobile", 390, 844], ["tablet", 1024, 768], ["desktop", 1440, 900]] as const) {
    await page.setViewportSize({ width, height });
    await expect.poll(() => page.evaluate(() => document.documentElement.scrollWidth <= innerWidth)).toBe(true);
    await page.locator("article#home-visual-review").scrollIntoViewIfNeeded();
    await page.screenshot({ path: `.scratch/dev/home-scenarios/${name}-dark.png` });
  }
});
