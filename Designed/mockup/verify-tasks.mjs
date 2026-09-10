import { reveal, clickAction, clearDemoStorage } from "./verify-helpers.mjs";
/* Task-first browser acceptance: UI actions against the local demo only. */
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
const browser = await chromium.launch({
  headless: true,
  executablePath:
    process.env.ERPX_CHROME_PATH ||
    "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome",
});
const page = await browser.newPage({ viewport: { width: 1440, height: 1000 } }),
  errors = [],
  assets = [];
page.on("pageerror", (e) => errors.push(e.message));
page.on("response", (r) => {
  if (r.status() >= 400) assets.push(r.url());
});
const base = process.env.ERPX_MOCKUP_URL || "http://127.0.0.1:8017",
  shots = "/tmp/erpx-tasks-review";
fs.mkdirSync(shots, { recursive: true });
page.setDefaultTimeout(5000);
const click = (a, scope = page) => clickAction(page, a, scope);
const nav = async (url) => {
  await page.evaluate((h) => (location.hash = h), url);
  await page.waitForTimeout(70);
};
const persona = async (id) => {
  if ((await page.evaluate(() => state.persona)) === id) return;
  if (
    page.viewportSize().width <= 760 &&
    !(await page.evaluate(() => state.mobileOpen))
  )
    await page.locator(".mobile-menu").click();
  await click("demo");
  await click("persona", page.locator("dialog"));
  await page.locator(`[data-action="choose-persona"][data-id="${id}"]`).click();
  await page.waitForTimeout(70);
};
const scenario = async (id) => {
  await click("demo");
  await page.locator(`[data-action="scenario"][data-id="${id}"]`).click();
};
const snapshot = () =>
  page.evaluate(() => {
    const p = parseRoute(),
      r = findRecord(p.key, p.id);
    return r
      ? {
          id: r.id,
          k: r.module,
          holding: r.holding,
          status: r.status,
          version: r.version,
          stage: r.stage,
          action: taskAction(r),
          bundle: taskBundle(r),
          owner: taskOwner(r),
          fields: wfData(r),
          keys: taskFieldList(r).map((f) => f.key),
          done: r.finished,
          pending: r.pendingHandoff,
          handoff: r.taskHandoffRef,
          history: r.history,
          events: r.taskEvents,
          issues: taskIssues(r),
          type: r.recordType,
        }
      : null;
  });
const edit = async (key, val) => {
  const el = page.locator(`[data-form="task-draft"] [name="${key}"]`);
  if (!(await el.count()) || (await el.isDisabled())) return;
  await reveal(el);
  if (typeof val === "boolean") await el.setChecked(val);
  else if ((await el.evaluate((e) => e.tagName)) === "SELECT")
    await el.selectOption(String(val));
  else await el.fill(String(val));
};
const fix = async (r) => {
  const gates = r.bundle.map((x) => x.gate);
  if (gates.includes("enrollment")) await edit("validated", true);
  if (gates.includes("quality")) {
    await edit("result", "Pass");
    await edit("reading", 3);
  }
  if (gates.includes("failed-check")) {
    await edit("result", "Fail");
    await edit("reading", 9);
  }
  if (gates.includes("verified")) await edit("verified", true);
  if (gates.includes("materials")) await edit("materials", true);
  if (gates.includes("manufacturing")) {
    await edit("quality", true);
    await edit("produced", r.fields.quantity);
  }
  if (gates.includes("progress")) await edit("progress", 100);
  if (gates.includes("attended")) await edit("attended", true);
  if (gates.includes("score")) await edit("score", 90);
  if (gates.includes("reconciled") || gates.includes("independent"))
    await edit("reconciled", true);
  if (r.type === "goods_receipt") await edit("quantity", r.fields.expected);
  if (
    gates.includes("evidence") &&
    (await page.locator('[data-action="task-evidence"]').count())
  )
    await click("task-evidence");
  if (r.action?.action === "Record acceptance") await edit("acceptance", true);
  if (r.action?.action === "Confirm fulfillment") await edit("delivery", true);
};
let modules = 0,
  sections = 0,
  actions = 0,
  retries = 0;
try {
  await page.goto(base);
  await page.waitForFunction(() => !!window.ERPX_TASKS);
  assert.equal(await page.locator(".task-home .task-card").count(), 3);
  assert.equal(
    await page.getByText("On your radar", { exact: true }).count(),
    0,
  );
  assert.equal(
    await page.locator('.top-actions [data-action="persona"]').count(),
    0,
  );
  await nav("#/holdings/acme/work");
  assert(await page.locator(".task-tabs").isVisible());
  const catalog = await page.evaluate(() =>
    Object.entries(MODULES).map(([id, m]) => ({ id, tabs: m.tabs.map(slug) })),
  );
  for (const { id: k, tabs } of process.env.ERPX_LAYOUT_ONLY ? [] : catalog) {
    await clearDemoStorage(page);
    await page.reload();
    await page.waitForFunction(() => !!window.ERPX_TASKS);
    const platform = ["commercial", "platform_administration"].includes(k),
      prefix = platform ? "#/platform" : "#/holdings/acme";
    await persona(platform ? "fluke" : "nisa");
    for (const section of ["overview", ...tabs, "records", "activity"]) {
      await nav(`${prefix}/ctx/${k}/${section}`);
      assert.equal(await page.locator("main h1").count(), 1, `${k}/${section}`);
      assert(
        !(await page.locator("main").innerText()).includes(
          "Section unavailable",
        ),
      );
      sections++;
    }
    await nav(`${prefix}/tasks/${k}/WF-${k.toUpperCase()}`);
    if (k === "customer_360") {
      assert.equal(await page.locator('[data-form="task-draft"]').count(), 0);
      modules++;
      continue;
    }
    let failed = false;
    for (let n = 0; n < 14; n++) {
      let r = await snapshot();
      assert(r, k + " record exists");
      if (!r.action) break;
      if (r.owner !== (await page.evaluate(() => state.persona))) {
        await persona(r.owner);
        await nav(`${prefix}/tasks/${r.k}/${r.id}`);
        r = await snapshot();
      }
      if (await page.locator('[data-action="task-switch-role"]').count())
        await click("task-switch-role");
      if (await page.locator('[data-action="task-continue"]').count())
        await click("task-continue");
      // A completed outcome can be reopened as its next task without a success popup.
      assert(
        (await page.locator('[data-form="task-draft"]').count()) ||
          (await page.locator(".task-review").count()),
        k + " task or decision reachable",
      );
      if (r.bundle.some((s) => s.gate === "hold")) {
        assert.match((await snapshot()).issues.join(" "), /legal hold/);
        break;
      }
      await fix(r);
      if (!failed) {
        await scenario("failure");
        failed = true;
      }
      const expected = (await snapshot()).version;
      if (await page.locator('[data-form="task-draft"]').count())
        await page
          .locator('[data-form="task-draft"] button[value="review"]')
          .click();
      const needsReview = await page.evaluate(() =>
        taskNeedsReview(findRecord(parseRoute().key, parseRoute().id)),
      );
      if (needsReview) {
        if (!(await page.locator('[data-action="task-confirm"]').count()))
          throw Error(
            k +
              " " +
              r.action.action +
              " blocked: " +
              (await page.locator("#task-issues").innerText()),
          );
        assert.equal(
          await page.locator("#wf-confirm").count(),
          0,
          "No generic checkbox",
        );
        await click("task-confirm");
      }
      await page.waitForTimeout(740);
      if ((await snapshot()).version === expected) {
        assert.match(
          await page.locator("#task-status").innerText(),
          /Could not save/,
        );
        assert.equal((await snapshot()).version, expected);
        retries++;
        if (needsReview) await click("task-confirm");
        else
          await page
            .locator('[data-form="task-draft"] button[value="review"]')
            .click();
        await page.waitForTimeout(740);
      }
      assert(await page.locator(".task-outcome").isVisible(), k + " outcome");
      actions++;
      const after = await snapshot();
      assert(after.version > expected);
      assert(after.events.some((x) => x.action === r.action.action));
      if (after.handoff) {
        if (
          after.handoff.module === k ||
          (k === "procurement" && after.handoff.module === "inventory")
        )
          await nav(
            `${prefix}/tasks/${after.handoff.module}/${after.handoff.id}`,
          );
        else break;
      } else if (!after.action) break;
    }
    modules++;
    console.log("PASS " + k);
  }
  // Phone/tablet/desktop layout and keyboard-readable controls.
  for (const width of [1440, 1024, 768, 390, 320]) {
    await page.setViewportSize({ width, height: 900 });
    await clearDemoStorage(page);
    await page.reload();
    await page.waitForFunction(() => !!window.ERPX_TASKS);
    for (const route of [
      "home",
      "work",
      "tasks/sales/WF-SALES",
      "tasks/people_payroll/WF-PEOPLE_PAYROLL",
      "ctx/sales",
      "community",
      "learn",
    ]) {
      await nav("#/holdings/acme/" + route);
      assert(
        await page.evaluate(
          () => document.documentElement.scrollWidth <= innerWidth + 1,
        ),
        `${width} overflow ${route}`,
      );
    }
    await nav("#/holdings/acme/tasks/sales/WF-SALES");
    await page.screenshot({
      path: `${shots}/quote-${width}.png`,
      fullPage: true,
    });
    await page.evaluate(() => document.documentElement.classList.add("dark"));
    await page.screenshot({
      path: `${shots}/quote-dark-${width}.png`,
      fullPage: true,
    });
  }
  for (const width of [390, 320]) {
    await page.setViewportSize({ width, height: 900 });
    await clearDemoStorage(page);
    await page.reload();
    await page.waitForFunction(() => !!window.ERPX_TASKS);
    for (const { id: k } of catalog) {
      const platform = ["commercial", "platform_administration"].includes(k),
        prefix = platform ? "#/platform" : "#/holdings/acme";
      await persona(platform ? "fluke" : "nisa");
      await nav(`${prefix}/tasks/${k}/WF-${k.toUpperCase()}`);
      const r = await snapshot();
      if (r?.owner && r.owner !== (await page.evaluate(() => state.persona))) {
        await persona(r.owner);
        await nav(`${prefix}/tasks/${k}/WF-${k.toUpperCase()}`);
      }
      for (const theme of ["light", "dark"]) {
        await page.evaluate(
          (t) =>
            document.documentElement.classList.toggle("dark", t === "dark"),
          theme,
        );
        assert(
          await page.evaluate(
            () => document.documentElement.scrollWidth <= innerWidth + 1,
          ),
          `${k} ${width} ${theme} overflow`,
        );
        for (const button of await page
          .locator(".task-action-bar button")
          .all()) {
          assert(
            await button.evaluate((el) => {
              const box = el.getBoundingClientRect();
              return (
                box.top >= 0 &&
                box.bottom <= innerHeight &&
                el.contains(
                  document.elementFromPoint(
                    box.x + box.width / 2,
                    box.y + box.height / 2,
                  ),
                )
              );
            }),
            `${k} ${width} ${theme} task action is covered`,
          );
        }
      }
      if (k === "finance")
        await page.screenshot({
          path: `${shots}/finance-dark-${width}.png`,
          fullPage: true,
        });
    }
  }
  assert.deepEqual(errors, []);
  assert.deepEqual(assets, []);
  console.log(
    JSON.stringify({ modules, sections, actions, retries, errors, assets }),
  );
} catch (e) {
  await page.screenshot({ path: `${shots}/failure.png`, fullPage: true });
  console.error(JSON.stringify(await snapshot()));
  throw e;
} finally {
  await browser.close();
}
