import { expect, test } from "@playwright/test";
import { readFile } from "node:fs/promises";
import { resolve } from "node:path";
import { frontOrigin, frontWorkspacePath, signInFrontRole, type SimulatedFrontCredentials } from "../helpers/front-simulation-session";

type DemoState = { fixture: { hospitality: { companyId: string } } };

test("Front POS shortcuts stay local and change the visible tab", async ({ browser }, testInfo) => {
  const run = process.env.ERPX_DEMO_RUN;
  expect(run, "The Thai Hotel mock runner must supply an exact prepared simulation run.").toMatch(/^[a-f0-9]{12}$/);
  const directory = resolve(`.runtime/browser/${run}`);
  const credentials = JSON.parse(await readFile(resolve(directory, "credentials.json"), "utf8")) as SimulatedFrontCredentials;
  const state = JSON.parse(await readFile(resolve(directory, "state.json"), "utf8")) as DemoState;
  const companyId = state.fixture.hospitality.companyId;
  const context = await browser.newContext(testInfo.project.use);
  try {
    const page = await context.newPage();
    await page.setViewportSize({ width: 1440, height: 900 });
    const destination = frontWorkspacePath(companyId, "?tab=sale");
    await signInFrontRole(context, page, credentials, "pos_cashier", "pos", companyId, destination);
    const shelf = page.locator("[data-quick-access-shelf]");
    await expect(shelf).toBeVisible();
    const [shelfBox, contentBox] = await Promise.all([shelf.boundingBox(), page.locator("#front-content").boundingBox()]);
    expect(shelfBox && contentBox && shelfBox.x > contentBox.x + contentBox.width - 2).toBe(true);
    await expect(shelf.locator('[data-quick-access-id="sale"]')).toBeVisible();
    await expect(shelf.locator('[data-quick-access-id="shifts"]')).toBeVisible();
    await expect(shelf.locator('[data-quick-access-id="kitchen"]')).toBeVisible();
    const shiftLink = shelf.locator('[data-quick-access-id="shifts"] a').first();
    await expect(shiftLink).toHaveAttribute("href", /\?tab=shifts$/);
    await shiftLink.click();
    await expect(page).toHaveURL(/\?tab=shifts$/);
    await expect(page.locator('nav[aria-label="Point of sale"] button[aria-current="page"]')).toHaveText("Cash shifts");
    expect(new URL(page.url()).origin).toBe(frontOrigin("pos"));
  } finally {
    await context.close();
  }
});
