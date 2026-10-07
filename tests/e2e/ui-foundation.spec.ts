import { captureCaseScreenshot } from "../helpers/case-screenshot";
import { expect, test } from "@playwright/test";

test("canonical UI controls remain accessible across themes and widths", async ({ page, context, baseURL }) => {
  test.skip(!["localhost", "127.0.0.1", "console-dev-erpx.jesadakorn.com"].includes(new URL(baseURL!).hostname), "The component gallery is development-only.");
  test.setTimeout(120_000);

  for (const theme of ["light", "dark"])
    for (const width of [390, 768, 1024, 1440]) {
      await context.addCookies([{ name: "erpx_appearance", value: theme, url: baseURL! }]);
      await page.setViewportSize({ width, height: 900 });
      await page.goto("/dev/ui");
      await expect(page.getByRole("heading", { name: "ERPX interface foundation" })).toBeVisible();
      await expect(page.locator("html")).toHaveAttribute("data-theme", theme);
      expect(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth + 1)).toBe(true);
      for (const name of ["Primary action", "Secondary", "Delete", "Saving", "Unavailable"])
        await expect(page.getByRole("button", { name, exact: true })).toBeVisible();
      await expect(page.getByRole("button", { name: "Delete record" })).toHaveAttribute("title", "Delete record");
      await expect(page.getByLabel("Legal name")).toHaveAttribute("required", "");
      await expect(page.getByLabel("Reference")).toHaveAttribute("aria-invalid", "true");
      await expect(page.getByRole("alert").filter({ hasText: "Could not save" })).toContainText("Your edits are still here");
      const undersized = await page.locator("main button:visible, main a:visible, main input:not([type=checkbox]):not([type=radio]):visible, main select:visible, main textarea:visible").evaluateAll((nodes) => nodes.filter((node) => {
        const rect = node.getBoundingClientRect();
        return rect.width > 0 && rect.height > 0 && rect.height < 35.5;
      }).map((node) => node.getAttribute("aria-label") || node.textContent || node.tagName));
      expect(undersized, `${width}px ${theme} controls`).toEqual([]);
      await captureCaseScreenshot(page, `gallery-${width}-${theme}.png`, { fullPage: true });
    }
});

test("foundation supports keyboard focus and reduced motion", async ({ page, baseURL }) => {
  test.skip(!["localhost", "127.0.0.1", "console-dev-erpx.jesadakorn.com"].includes(new URL(baseURL!).hostname), "The component gallery is development-only.");
  await page.emulateMedia({ reducedMotion: "reduce", colorScheme: "dark" });
  await page.goto("/dev/ui");
  await page.keyboard.press("Tab");
  await expect(page.locator(":focus")).toBeVisible();
  await page.getByRole("button", { name: "Primary action" }).focus();
  expect(await page.getByRole("button", { name: "Primary action" }).evaluate((node) => getComputedStyle(node).outlineStyle)).not.toBe("none");
  expect(await page.getByRole("button", { name: "Primary action" }).evaluate((node) => parseFloat(getComputedStyle(node).transitionDuration))).toBeLessThanOrEqual(0.001);
});
