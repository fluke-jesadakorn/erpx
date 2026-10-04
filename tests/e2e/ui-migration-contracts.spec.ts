import { expect, test } from "@playwright/test";

function developmentConsole(baseURL?: string) {
  return !!baseURL && ["localhost", "127.0.0.1", "console-dev-erpx.jesadakorn.com"].includes(new URL(baseURL).hostname);
}

test.beforeEach(async ({ baseURL }) => {
  test.skip(!developmentConsole(baseURL), "Uses the development-only component gallery.");
});

test("native form values, submitter and keyboard submit survive the UI migration", async ({ page }) => {
  await page.goto("/dev/ui");
  await expect(page.getByRole("button", { name: "Submit contract form" })).toBeEnabled();
  await page.getByLabel("Contract title").fill("ทดสอบภาษาไทย");
  await page.getByLabel("Contract team").selectOption("people");
  await page.getByLabel("Contract notes").fill("บันทึกที่ยังแก้ไขได้");
  await page.getByRole("button", { name: "Submit contract form" }).click();
  await expect(page.getByTestId("ui-form-result")).toHaveText("submit · ทดสอบภาษาไทย · people · บันทึกที่ยังแก้ไขได้");
  await page.getByRole("button", { name: "Non-submit action" }).click();
  await expect(page.getByTestId("ui-form-result")).toHaveText("Action only");
  await page.getByLabel("Contract title").press("Enter");
  await expect(page.getByTestId("ui-form-result")).toHaveText("submit · ทดสอบภาษาไทย · people · บันทึกที่ยังแก้ไขได้");
});

test("controlled dialogs retain dirty drafts, trap focus and restore the opener", async ({ page }) => {
  await page.goto("/dev/ui");
  const opener = page.getByRole("button", { name: "Open draft dialog" });
  await opener.click();
  await expect(page.getByLabel("Dialog draft")).toBeFocused();
  await page.getByLabel("Dialog draft").fill("ยังไม่บันทึก");
  await page.keyboard.press("Escape");
  const confirmation = page.getByRole("dialog", { name: "Discard changes?", exact: true });
  await expect(confirmation).toBeVisible();
  await confirmation.getByRole("button", { name: "Keep editing" }).click();
  await expect(confirmation).toBeHidden();
  await expect(page.getByLabel("Dialog draft")).toHaveValue("ยังไม่บันทึก");
  const draft = page.getByRole("dialog", { name: "Draft dialog", exact: true });
  for (let count = 0; count < 5; count++) {
    await page.keyboard.press("Tab");
    await expect.poll(() => draft.evaluate(node => node.contains(document.activeElement))).toBe(true);
  }
  await page.keyboard.press("Escape");
  await confirmation.getByRole("button", { name: "Discard draft" }).click();
  await expect(draft).toBeHidden();
  await expect(opener).toBeFocused();
});

test("light, dark and system appearance share the UI tokens and remain responsive", async ({ page, context, baseURL }) => {
  test.setTimeout(180_000);
  for (const [theme, scheme] of [["light", "light"], ["dark", "dark"], ["system", "light"], ["system", "dark"]] as const) {
    await context.addCookies([{ name: "erpx_appearance", value: theme, url: baseURL! }]);
    await page.emulateMedia({ colorScheme: scheme });
    for (const width of [390, 768, 1440]) {
      await page.setViewportSize({ width, height: 900 });
      await page.goto("/dev/ui");
      await expect(page.getByRole("heading", { name: "ERPX interface foundation" })).toBeVisible();
      await expect(page.locator("html")).toHaveAttribute("data-theme", theme);
      const expected = theme === "dark" || theme === "system" && scheme === "dark" ? "rgb(21, 26, 33)" : "rgb(242, 245, 248)";
      expect(await page.locator("body").evaluate(node => getComputedStyle(node).backgroundColor)).toBe(expected);
      expect(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth + 1)).toBe(true);
      expect(await page.getByRole("button", { name: "Submit contract form" }).evaluate(node => node.getBoundingClientRect().height)).toBeGreaterThanOrEqual(44);
      const register = page.getByRole("region", { name: "Example invoices", exact: true });
      const dimensions = await register.evaluate(node => ({
        frame: node.getBoundingClientRect().width,
        font: getComputedStyle(node.querySelector("td")!).fontSize,
        header: node.querySelector("thead tr")!.getBoundingClientRect().height,
        rows: [...node.querySelectorAll("tbody tr")].map(row => row.getBoundingClientRect().height),
      }));
      expect(dimensions.frame).toBeLessThanOrEqual(width);
      expect(dimensions.font).toBe("14px");
      expect(dimensions.header).toBeGreaterThanOrEqual(44);
      expect(dimensions.rows.every(height => height >= 56)).toBe(true);
    }
  }
});
