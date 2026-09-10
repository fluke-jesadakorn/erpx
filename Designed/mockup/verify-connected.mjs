import assert from "node:assert/strict";
import fs from "node:fs";
import path from "node:path";
import { pathToFileURL } from "node:url";
import { reveal, clickAction, clearDemoStorage } from "./verify-helpers.mjs";

const { chromium } = await import(pathToFileURL(path.join(process.env.ERPX_PLAYWRIGHT || "/tmp/erpx-browser-tools/node_modules/playwright", "index.mjs")).href);
const browser = await chromium.launch({ headless: true, executablePath: process.env.ERPX_CHROME_PATH || "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome" });
const context = await browser.newContext();
const page = await context.newPage();
const base = process.env.ERPX_MOCKUP_URL || "http://127.0.0.1:8017";
const errors = [];
const shots = "/tmp/erpx-connected-review";
fs.mkdirSync(shots, { recursive: true });
page.on("pageerror", (e) => errors.push(e.message));

async function ready(url = base + "/#/holdings/acme/home") {
  await page.goto(url);
  await page.waitForFunction(() => window.ERPX_READY === true);
}
async function persona(id) {
  await clickAction(page, "demo");
  await clickAction(page, "persona", page.locator("dialog"));
  await page.locator(`[data-action="choose-persona"][data-id="${id}"]`).click();
}

try {
  await ready();
  await clearDemoStorage(page);
  await page.reload();
  await page.waitForFunction(() => window.ERPX_READY === true);

  const coverage = await page.evaluate(() => Object.fromEntries(Object.entries(CAPABILITY_MANIFEST).map(([k, xs]) => [k, xs.length])));
  assert.equal(Object.keys(coverage).length, 31);
  assert(Object.values(coverage).every((count) => count >= 3));

  await persona("nisa");
  for (const [module, count] of Object.entries(coverage)) {
    if (["platform_administration", "commercial"].includes(module)) continue;
    await page.evaluate(({ module }) => { location.hash = route(`ctx/${module}/${capabilityTabs(module)[0].id}`); }, { module });
    await page.waitForTimeout(60);
    assert(!(await page.locator("main").innerText()).includes("Section unavailable"), module);
    assert(await page.locator("main section, main .workflow-board, main .org-tree").count(), module);
    assert(count === await page.evaluate((module) => capabilityTabs(module).length, module));
  }

  await page.evaluate(() => { location.hash = route("tasks/support/WF-SUPPORT"); });
  await page.waitForTimeout(80);
  const discussion = page.locator('[data-task-detail="discussion"]');
  await discussion.locator(":scope > summary").click();
  const imagePath = path.resolve("Designed/mockup/assets/team.svg");
  // Browser input supports SVG selection, but the component correctly rejects it.
  await discussion.locator('[data-attachment-input]').setInputFiles(imagePath);
  assert.equal(await page.evaluate(() => Object.values(state.attachmentDrafts).flat().length), 0);
  await discussion.locator('[data-attachment-input]').setInputFiles({ name: "delivery.png", mimeType: "image/png", buffer: Buffer.from("iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mNk+A8AAQUBAScY42YAAAAASUVORK5CYII=", "base64") });
  await reveal(discussion.locator("textarea"));
  await discussion.locator("textarea").fill("Attached the delivery evidence for the shared record conversation.");
  await discussion.locator('[type="submit"]').click();
  await page.waitForTimeout(1300);
  assert.equal(await page.locator('.discussion-entry .attachment-card').count(), 1);
  await page.reload();
  await page.waitForFunction(() => window.ERPX_READY === true);
  const restored = page.locator('[data-task-detail="discussion"]');
  if (!(await restored.evaluate((el) => el.open))) await restored.locator(":scope > summary").click();
  assert.equal(await page.locator('.discussion-entry .attachment-card').count(), 1);
  assert((await page.locator("main").innerText()).includes("delivery.png"));

  await page.evaluate(() => { location.hash = route(`ctx/crm/${capabilityTabs("crm")[0].id}`); });
  await page.waitForTimeout(80);
  await page.locator('[data-action="capability-run"]').click();
  assert((await page.locator("main").innerText()).includes("Updated"));
  assert(await page.evaluate(() => posts[0].module === "crm"));
  await page.setViewportSize({ width: 1440, height: 950 });
  await page.screenshot({ path: `${shots}/crm-account-feature.png`, fullPage: true });
  await page.evaluate(() => { location.hash = route(`ctx/inventory/${capabilityTabs("inventory").find((x) => x.label === "Availability").id}`); document.documentElement.classList.add("dark"); });
  await page.waitForTimeout(100);
  await page.screenshot({ path: `${shots}/inventory-availability-dark.png`, fullPage: true });
  await page.evaluate(() => { location.hash = route("tasks/projects/WF-PROJECTS"); });
  await page.waitForTimeout(100);
  await page.screenshot({ path: `${shots}/project-task-expanded-dark.png`, fullPage: true });
  await page.evaluate(() => document.documentElement.classList.remove("dark"));
  await page.evaluate(() => { location.hash = route("tasks/support/WF-SUPPORT"); });
  await page.waitForTimeout(80);
  const mobileDiscussion = page.locator('[data-task-detail="discussion"]');
  if (!(await mobileDiscussion.evaluate((el) => el.open))) await mobileDiscussion.locator(":scope > summary").click();
  await page.setViewportSize({ width: 390, height: 844 });
  await page.waitForTimeout(450);
  await page.screenshot({ path: `${shots}/support-conversation-mobile.png`, fullPage: true });
  await page.evaluate(() => { location.hash = route("tasks/finance/WF-FINANCE"); });
  await page.waitForTimeout(100);
  assert(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth + 1), "finance phone overflow");

  assert.deepEqual(errors, []);
  console.log(JSON.stringify({ modules: Object.keys(coverage).length, features: Object.values(coverage).reduce((a, b) => a + b, 0), persistedAttachment: true, screenshots: shots, errors }, null, 2));
} finally {
  await browser.close();
}
