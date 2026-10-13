import { expect, test, type Locator, type Page } from "@playwright/test";
import { investorJourneys } from "../../../scripts/testing/investor-selection";
import { captureCaseScreenshot } from "./case-screenshot";

/** Adds evidence to existing journeys only when the curated runner explicitly opts in. */
export async function captureInvestorCheckpoint(page: Page, id: string, focus?: Locator) {
  if (process.env.ERPX_INVESTOR_CAPTURE !== "1") return;
  const info = test.info();
  const journey = investorJourneys.find(item => item.title === info.title);
  const point = journey?.checkpoints.find(item => item.id === id);
  if (!journey || !point) throw new Error(`UNKNOWN_INVESTOR_CHECKPOINT:${id}`);
  if (info.project.name === "mobile" && !point.mobile) return;
  if (!["desktop", "mobile"].includes(info.project.name)) throw new Error("UNKNOWN_INVESTOR_PROJECT");
  const name = `investor-${id}-${info.project.name}`;
  if (info.attachments.some(item => item.name === `${name}.json`)) throw new Error(`DUPLICATE_INVESTOR_CHECKPOINT:${id}`);
  // Extend once, only for a checkpoint captured on this device; normal E2E timing is unchanged.
  if (!info.attachments.some(item => item.name.startsWith("investor-") && item.contentType === "application/json")) info.setTimeout(info.timeout + 60_000);
  const previous = page.viewportSize();
  try {
    if (info.project.name === "desktop") await page.setViewportSize({ width: 1440, height: 900 });
    const search = page.getByRole("button", { name: "Search workspace", exact: true });
    if (await search.count()) await expect(search).toBeEnabled();
    await expect(page.locator('[aria-label="Loading shortcuts"]')).toBeHidden();
    await page.evaluate(async () => { await document.fonts.ready; });
    if (focus) await focus.scrollIntoViewIfNeeded();
    await expect.poll(() => page.evaluate(() => document.documentElement.scrollWidth <= innerWidth), { message: `${id}: page must fit its viewport` }).toBe(true);
    const buffer = await captureCaseScreenshot(page, `${name}.png`, { animations: "disabled", fullPage: false }, info);
    await info.attach(`${name}.json`, { body: JSON.stringify({ id, project: info.project.name, capturedAt: new Date().toISOString(), viewport: page.viewportSize(), route: new URL(page.url()).pathname }), contentType: "application/json" });
    return buffer;
  } finally {
    if (previous && info.project.name === "desktop") await page.setViewportSize(previous);
  }
}
