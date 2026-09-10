import assert from "node:assert/strict";
import fs from "node:fs";
import path from "node:path";
import { pathToFileURL } from "node:url";
const { chromium } = await import(
  pathToFileURL(
    path.join(
      process.env.ERPX_PLAYWRIGHT ||
        "/tmp/erpx-browser-tools/node_modules/playwright",
      "index.mjs",
    ),
  ).href
);
import { reveal, clickAction, clearDemoStorage } from "./verify-helpers.mjs";
const browser = await chromium.launch({
  headless: true,
  executablePath:
    process.env.ERPX_CHROME_PATH ||
    "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome",
});
const page = await browser.newPage({ viewport: { width: 1440, height: 1000 } }),
  errors = [],
  assets = [],
  counts = {};
page.setDefaultTimeout(5000);
page.on("pageerror", (e) => errors.push(e.message));
page.on("response", (r) => {
  if (r.status() >= 400) assets.push(r.url());
});
const base = process.env.ERPX_MOCKUP_URL || "http://127.0.0.1:8017";
const click = async (a) => {
  await clickAction(page, a);
  await page.waitForTimeout(80);
};
const nav = async (hash) => {
  await page.goto(base + hash);
  await page.waitForFunction(() => !!window.ERPX_TASKS);
};
// Hash navigation preserves the temporary business data.
const route = async (hash) => {
  await page.evaluate((h) => (location.hash = h), hash);
  await page.waitForTimeout(60);
};
const reset = async () => {
  await nav("#/holdings/acme/home");
  await clearDemoStorage(page);
  await page.reload();
  await page.waitForFunction(() => !!window.ERPX_TASKS);
};
const persona = async (id) => {
  await click("demo");
  await click("persona");
  await page.locator(`[data-action="choose-persona"][data-id="${id}"]`).click();
};
const task = async (module, id = "WF-" + module.toUpperCase()) =>
  route(`#/holdings/acme/tasks/${module}/${id}`);
const record = () =>
  page.evaluate(() => {
    const p = parseRoute();
    return structuredClone(findRecord(p.key, p.id));
  });
const edit = async (key, value) => {
  const el = page.locator(`[data-form="task-draft"] [name="${key}"]`);
  await reveal(el);
  if (typeof value === "boolean") await el.setChecked(value);
  else if ((await el.evaluate((e) => e.tagName)) === "SELECT")
    await el.selectOption(value);
  else await el.fill(String(value));
};
const act = async () => {
  await page.waitForTimeout(80);
  const before = await record();
  let clicks = 0;
  if (await page.locator('[data-form="task-draft"]').count()) {
    await page.locator('[value="review"]').click();
    await page.waitForTimeout(80);
    clicks++;
  }
  if (await page.locator('[data-action="task-confirm"]').count()) {
    await click("task-confirm");
    clicks++;
  }
  await page.waitForTimeout(750);
  assert(
    (await record()).version > before.version,
    `${before.module}: ${await page.locator("main").innerText()}`,
  );
  return clicks;
};
try {
  await reset();
  // Home owns social updates; old bookmarks replace their own history entry.
  assert.equal(await page.locator('.sidebar nav a').allTextContents().then((xs) => xs.some((x) => x.includes("Community"))), false);
  assert.equal(await page.locator('#home-department').count(), 1);
  assert(await page.locator('svg[data-icon="House"]').count());
  const initialHistory = await page.evaluate(() => history.length);
  await route("#/holdings/acme/community");
  assert.equal(await page.evaluate(() => location.hash), "#/holdings/acme/home");
  assert.equal(await page.evaluate(() => history.length), initialHistory + 1);
  await page.locator('#home-department').selectOption('Sales');
  assert.equal(await page.locator('#home-department').inputValue(), 'Sales');
  assert(await page.evaluate(() => [...document.querySelectorAll('.home-updates article.post')].every((el) => {
    const post = posts.find((p) => 'post-' + p.id === el.id);
    return !post.department || post.department === 'Sales';
  })));
  await page.locator('[data-action="feed-tab"][data-id="records"]').click();
  assert(await page.evaluate(() => [...document.querySelectorAll('.home-updates article.post')].every((el) => posts.find((p) => 'post-' + p.id === el.id).kind === 'record')));
  assert.equal(await page.evaluate(() => document.activeElement.dataset.id), 'records');
  await persona('dao');
  await route('#/holdings/acme/home');
  assert.deepEqual(await page.locator('#home-department option').evaluateAll((xs) => xs.map((x) => x.value)), ['all', 'Operations']);
  assert.equal(await page.locator('#home-department').inputValue(), 'all');
  await route('#/holdings/northstar/community');
  assert.equal(await page.evaluate(() => location.hash), '#/holdings/northstar/home');
  assert(await page.getByText('This link is outside your workspace').isVisible());
  await reset();
  await task("people_payroll");
  let n = await act();
  await persona("nisa");
  await task("people_payroll");
  assert.equal(await page.locator('[data-form="task-draft"]').count(), 0);
  assert.equal(await page.locator(".task-review input").count(), 0);
  n += await act();
  assert.equal(n, 2);
  counts.leave = n;
  await reset();
  await task("support");
  n = await act();
  assert(await page.locator('[data-form="task-draft"]').isVisible());
  n += await act();
  assert.equal(n, 3);
  counts.support = n;
  await reset();
  await task("procurement");
  n = await act();
  await persona("nisa");
  await task("procurement");
  n += await act();
  await page.locator(".prepared-draft-card").click();
  await page.waitForTimeout(80);
  n += await act();
  await page.locator(".prepared-draft-card").click();
  await page.waitForTimeout(80);
  let r = await record();
  let owner = await page.evaluate(() =>
    taskOwner(findRecord(parseRoute().key, parseRoute().id)),
  );
  if (owner !== "nisa") {
    await persona(owner);
    await task(r.module, r.id);
  }
  await edit("quantity", 100);
  await click("task-evidence");
  n += await act();
  await page.locator(".prepared-draft-card").click();
  await page.waitForTimeout(80);
  n += await act();
  assert.equal(n, 8);
  counts.supply = n;
  await reset();
  await persona("nisa");
  await task("finance");
  n = await act();
  assert.equal(await page.locator('[data-action="task-continue"]').count(), 0);
  await edit("reconciled", true);
  n += await act();
  r = await record();
  const period = r.taskHandoffRef.id;
  await persona("pim");
  await task("finance", period);
  n += await act();
  assert.equal(n, 5);
  counts.finance = n;
  await reset();
  await task("quality");
  n = await act();
  await edit("result", "Pass");
  await edit("reading", 3);
  n += await act();
  n += await act();
  assert.equal(n, 4);
  counts.qualityRecheck = n;
  await reset();
  await route("#/holdings/acme/ctx/quality");
  await click("wf-new");
  await edit("result", "Pass");
  await edit("reading", 3);
  n = await act();
  assert.equal((await record()).status, "Passed");
  assert.equal((await record()).stage, 2);
  n += await act();
  assert.equal(n, 3);
  counts.qualityPass = n;
  await reset();
  await persona("nisa");
  await task("customer_authority");
  assert.equal((await record()).version, 1);
  assert.equal(
    await page.locator('input:not([disabled]):not([type="file"])').count(),
    0,
  );
  n = await act();
  assert.equal(n, 2);
  assert(
    !(await record()).history.some((h) =>
      h.text.startsWith("Compare profiles"),
    ),
  );
  counts.merge = n;
  await reset();
  await task("pos");
  n = await act();
  assert.equal(n, 2);
  counts.pos = n;
  // A changed approval cannot confirm a newer version invisibly.
  await reset();
  await task("people_payroll");
  await act();
  await persona("nisa");
  await task("people_payroll");
  await click("demo");
  await click("task-demo-change");
  await click("task-confirm");
  assert(
    await page.getByRole("heading", { name: "The record changed" }).isVisible(),
  );
  assert.equal((await record()).status, "Pending approval");
  await click("task-rebase");
  assert.equal(await page.locator('[data-form="task-draft"]').count(), 0);
  await act();
  // Direct destinations and return position.
  await reset();
  assert.equal(await page.locator(".context-control").count(), 1);
  await page.getByRole("link", { name: "Manage favorite modules" }).click();
  await page.waitForTimeout(80);
  assert(
    await page
      .getByRole("heading", { name: "Modules", exact: true })
      .isVisible(),
  );
  await route("#/holdings/acme/ctx/sales/overview");
  const attention = page.locator(".attention-card").first();
  assert((await attention.getAttribute("href")).includes("/tasks/"));
  await attention.click();
  await page.waitForTimeout(80);
  assert(await page.locator('[data-form="task-draft"]').isVisible());
  await click("task-back");
  assert(page.url().endsWith("/ctx/sales/overview"));
  await route("#/holdings/acme/work");
  await page.locator("#task-search").fill("quote");
  assert.equal(await page.locator(".task-card").count(), 1);
  await page.locator(".task-card").click();
  await page.waitForTimeout(80);
  await click("task-back");
  assert.equal(await page.locator("#task-search").inputValue(), "quote");
  assert.equal(await page.locator(".task-card").count(), 1);
  assert(
    await page
      .locator(".task-card")
      .evaluate((e) => e === document.activeElement),
  );
  await route("#/holdings/acme/home");
  await page
    .locator('#post-acme-quote [data-action="comments"]')
    .last()
    .click();
  await page.waitForTimeout(80);
  assert(page.url().includes("view=discussion"));
  assert(
    await page
      .locator('input[name="text"],textarea[name="text"]')
      .evaluateAll((es) => es.some((e) => e === document.activeElement)),
  );
  // Home acceptance and one phone action bar; emulate keyboard viewport contraction.
  await page.setViewportSize({ width: 390, height: 844 });
  await reset();
  assert.equal(await page.locator(".task-home .task-card").count(), 3);
  assert(
    await page
      .locator(".story-section")
      .evaluate(
        (e) =>
          e.getBoundingClientRect().top <
          document.querySelector(".mobile-nav").getBoundingClientRect().top,
      ),
  );
  await task("sales");
  assert.equal(await page.locator(".mobile-nav").count(), 0);
  await page.setViewportSize({ width: 390, height: 480 });
  for (const el of await page.locator(".task-action-bar button").all())
    assert(
      await el.evaluate((e) => {
        const r = e.getBoundingClientRect();
        return (
          r.top >= 0 &&
          r.bottom <= innerHeight &&
          e.contains(
            document.elementFromPoint(r.x + r.width / 2, r.y + r.height / 2),
          )
        );
      }),
    );
  const screenshots = "/tmp/erpx-calm-review";
  fs.mkdirSync(screenshots, { recursive: true });
  for (const width of [1440, 1024, 768, 390, 320]) {
    await page.setViewportSize({ width, height: 844 });
    await reset();
    for (const theme of ["light", "dark"]) {
      await page.evaluate(
        (t) => document.documentElement.classList.toggle("dark", t === "dark"),
        theme,
      );
      for (const destination of [
        "home",
        "work",
        "modules",
        "ctx/sales",
        "ctx/sales/overview",
        "tasks/sales/WF-SALES",
        "rec/sales/QT-2026-042",
        "community",
        "learn",
      ]) {
        await route("#/holdings/acme/" + destination);
        if (destination === "community") assert.equal(await page.evaluate(() => location.hash), "#/holdings/acme/home");
        assert(
          await page.evaluate(
            () => document.documentElement.scrollWidth <= innerWidth + 1,
          ),
          `${destination} ${width} ${theme}: page overflow`,
        );
        if (destination === "ctx/sales") {
          await page.locator(".section-more>summary").click();
          assert(
            await page.locator(".section-more>div").evaluate((el) => {
              const r = el.getBoundingClientRect();
              return r.left >= 0 && r.right <= innerWidth;
            }),
            "More menu fits viewport",
          );
          await page.locator(".section-more a").first().focus();
          await page.keyboard.press("Escape");
          assert(
            !(await page.locator(".section-more").evaluate((el) => el.open)),
          );
        }
        if (["home", "work", "tasks/sales/WF-SALES"].includes(destination))
          await page.screenshot({
            path: `${screenshots}/${destination.split("/")[0]}-${width}-${theme}.png`,
          });
      }
    }
  }
  assert.deepEqual(errors, []);
  assert.deepEqual(assets, []);
  console.log(
    JSON.stringify({
      counts,
      checks:
        "decision safety, direct navigation, draft scope, return focus, phone Home and action bar",
      errors,
      assets,
    }),
  );
} finally {
  await browser.close();
}
