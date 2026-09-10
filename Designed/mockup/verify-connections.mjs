import { reveal, clickAction } from "./verify-helpers.mjs";
import assert from "node:assert/strict";
import path from "node:path";
import { pathToFileURL } from "node:url";
const { chromium } = await import(
  process.env.ERPX_PLAYWRIGHT
    ? pathToFileURL(path.join(process.env.ERPX_PLAYWRIGHT, "index.mjs")).href
    : "playwright"
);
const b = await chromium.launch({
  headless: true,
  ...(process.env.ERPX_CHROME_PATH
    ? { executablePath: process.env.ERPX_CHROME_PATH }
    : {}),
});
const p = await b.newPage({ viewport: { width: 1440, height: 1000 } }),
  errors = [];
p.on("pageerror", (e) => errors.push(e.message));
const click = (a, root = p) => clickAction(p, a, root);
const close = () => click("close-dialog", p.locator("dialog"));
const nav = async (h) => {
  await p.evaluate((h) => (location.hash = h), h);
  await p.waitForTimeout(90);
};
const persona = async (id) => {
  await click("demo");
  await click("persona", p.locator("dialog"));
  await p.locator(`[data-action="choose-persona"][data-id="${id}"]`).click();
};
const scenario = async (id) => {
  await click("demo");
  await p.locator(`[data-action="scenario"][data-id="${id}"]`).click();
};
const record = () =>
  p.evaluate(() => {
    const x = parseRoute();
    return structuredClone(findRecord(x.key, x.id));
  });
let count = 0;
const check = async (name, f) => {
  await f();
  count++;
  console.log("PASS", name);
};
try {
  await p.goto(process.env.ERPX_MOCKUP_URL || "http://127.0.0.1:8017");
  await p.waitForFunction(() => !!window.ERPX_DEMO);
  await persona("nisa");
  await check(
    "Multiple references, deduplication, ordering and backlinks",
    async () => {
      await nav("#/holdings/acme/rec/sales/WF-SALES");
      const before = (await record()).refs.length;
      for (let n = 0; n < 2; n++) {
        await click("cx-add-ref");
        await p.locator("#cx-query").fill("WF-QUALITY");
        await click("cx-insert-ref");
      }
      assert.equal((await record()).refs.length, before + 1);
      const id = (await record()).refs.at(-1).id;
      await p
        .locator('[data-action="cx-move-ref"][data-direction="-1"]')
        .last()
        .click();
      assert.equal((await record()).refs.at(-2).id, id);
      await nav("#/holdings/acme/rec/quality/WF-QUALITY");
      assert(
        (await p.locator(".reference-panel").innerText()).includes(
          "Riverstone · autumn store order",
        ),
      );
    },
  );
  await check(
    "Assistant proposal requires explicit application; source conflict preserves data",
    async () => {
      await nav("#/holdings/acme/rec/sales/WF-SALES");
      await click("task-assign");
      await p
        .locator('[data-form="task-assignment"] select')
        .selectOption("nisa");
      await p.locator('[data-form="task-assignment"] button').click();
      await click("task-action-mode");
      const before = (await record()).fields.notes;
      await click("wf-suggest");
      await p.waitForSelector('[data-action="wf-apply-suggestion"]');
      assert.equal((await record()).fields.notes, before);
      await scenario("source-conflict");
      await click("wf-apply-suggestion");
      assert(
        (await p.locator(".assistant-card").innerText()).includes("Conflicted"),
      );
      assert.equal((await record()).fields.notes, before);
      if (await p.locator("dialog").evaluate((d) => d.open)) await close();
      await click("clear-state");
      await click("wf-suggest");
      await p.waitForSelector('[data-action="wf-apply-suggestion"]');
      await click("wf-apply-suggestion");
      assert.equal((await record()).fields.notes, before);
      await close();
      await p.locator('[data-form="task-draft"] [value="save"]').click();
      await p.waitForTimeout(760);
      assert.notEqual((await record()).fields.notes, before);
      await click("wf-reject-suggestion");
      assert(
        (await p.locator(".assistant-card").innerText()).includes("Rejected"),
      );
    },
  );
  await check(
    "Scoped mention picker and a real mention inbox item",
    async () => {
      if (await p.locator("dialog").evaluate((d) => d.open)) await close();
      const f = p.locator('[data-form="wf-comment"]');
      await reveal(f.locator("textarea"));
      await f.locator("textarea").fill("Pim, please review the handoff.");
      await click("cx-picker", f);
      await p.locator("#cx-query").fill("Pim Wong");
      await click("cx-insert-ref");
      await f.locator('[type="submit"]').click();
      assert((await record()).comments.at(-1).refs.some((x) => x.id === "pim"));
      await persona("pim");
      await nav("#/holdings/acme/inbox");
      // Pim cannot see Sales under the Finance assignment; mention content stays hidden.
      assert(
        !(await p.locator("main").innerText()).includes(
          "Riverstone · autumn store order",
        ),
      );
      await nav("#/holdings/acme/rec/finance/WF-FINANCE");
      const ff = p.locator('[data-form="wf-comment"]');
      await reveal(ff.locator("textarea"));
      await ff.locator("textarea").fill("Pim, inspect the journal sources.");
      await click("cx-picker", ff);
      await p.locator("#cx-query").fill("Pim Wong");
      await click("cx-insert-ref");
      await ff.locator('[type="submit"]').click();
      await nav("#/holdings/acme/inbox");
      assert(
        (await p.locator("main").innerText()).includes("You were mentioned"),
      );
      await persona("nisa");
    },
  );
  await check(
    "Internal support notes stay out of general feeds; moderation retains evidence",
    async () => {
      await nav("#/holdings/acme/rec/support/WF-SUPPORT");
      if (await p.locator("dialog").evaluate((d) => d.open)) await close();
      const f = p.locator('[data-form="wf-comment"]');
      await reveal(f.locator("textarea"));
      await f
        .locator("textarea")
        .fill("PRIVATE internal cold-room supplier phone 555-demo");
      await f.locator('[type="submit"]').click();
      assert.equal((await record()).comments.at(-1).visibility, "internal");
      await click("cx-moderate");
      await click("cx-confirm-moderation");
      assert(
        (await p.locator(".discussion-entry").innerText()).includes(
          "moderated",
        ),
      );
      assert(
        !(await p.locator(".discussion-entry").innerText()).includes(
          "555-demo",
        ),
      );
      await nav("#/holdings/acme/home");
      assert(!(await p.locator("main").innerText()).includes("555-demo"));
    },
  );
  await check(
    "Record, person and topic follows remain scoped and chronological",
    async () => {
      await nav("#/holdings/acme/rec/sales/WF-SALES");
      await click("cx-follow");
      assert(await p.evaluate(() => isFollowed("record", "sales/WF-SALES")));
      await nav("#/holdings/acme/tags/quote");
      await p.locator('.page-heading [data-action="cx-follow"]').click();
      assert(await p.evaluate(() => isFollowed("tag", "#quote")));
      await nav("#/holdings/acme/home");
      const timestamps = await p.evaluate(() =>
        visiblePosts().map((x) => x.created),
      );
      assert.deepEqual(
        timestamps,
        [...timestamps].sort((a, b) => b - a),
      );
      await persona("mali");
      assert(!(await p.evaluate(() => isFollowed("record", "sales/WF-SALES"))));
      await persona("nisa");
    },
  );
  await check(
    "Restricted multi-reference posts hidden from feed, search, Stories and customer previews",
    async () => {
      await nav("#/holdings/acme/home");
      await click("compose");
      await p.locator("#post-text").fill("QA restricted finance memo");
      await click("cx-picker", p.locator("#post-form"));
      await p.locator("#cx-query").fill("WF-FINANCE");
      await click("cx-insert-ref");
      await p.locator('#post-form [type="submit"]').click();
      await p.waitForTimeout(760);
      assert(
        (await p.locator("main").innerText()).includes(
          "QA restricted finance memo",
        ),
      );
      await click("new-story");
      await p.locator("#post-text").fill("QA private finance Story");
      await click("cx-picker", p.locator("#post-form"));
      await p.locator("#cx-query").fill("WF-FINANCE");
      await click("cx-insert-ref");
      await p.locator('#post-form [type="submit"]').click();
      await p.waitForTimeout(760);
      await persona("mali");
      assert(
        !(await p.evaluate(() =>
          visibleStories().some((s) => s.text === "QA private finance Story"),
        )),
      );
      assert(
        !(await p.locator("main").innerText()).includes(
          "QA restricted finance memo",
        ),
      );
      await click("search");
      await p.locator("#search-input").fill("QA restricted finance memo");
      assert.equal(await p.locator(".search-result").count(), 0);
      await close();
      await nav("#/holdings/acme/rec/finance/WF-FINANCE");
      assert((await p.locator("main").innerText()).includes("unavailable"));
      await nav("#/holdings/acme/journeys/revenue");
      assert(
        !(await p.locator("main").innerText()).includes(
          "September stock receipt journal",
        ),
      );
      assert(
        (await p.locator("main").innerText()).includes("Unavailable step"),
      );
      await persona("arun");
      await nav("#/holdings/acme/posts/test");
      assert(
        (await p.locator("main").innerText()).includes(
          "outside your workspace",
        ),
      );
    },
  );
  await check(
    "Offline, degraded, expired and prerequisite states retain temporary work",
    async () => {
      await persona("nisa");
      await nav("#/holdings/acme/rec/sales/WF-SALES");
      for (const id of ["offline", "degraded", "prerequisite"]) {
        await scenario(id);
        await nav("#/holdings/acme/tasks/sales/WF-SALES");
        assert((await p.locator("main").innerText()).includes(id));
        if (await p.locator("dialog").evaluate((d) => d.open)) await close();
        await click("clear-state");
      }
      await scenario("expired");
      assert.equal(await p.locator('[data-action="wf-review"]').count(), 0);
      await click("search");
      await p.locator("#search-input").fill("WF-SALES");
      assert.equal(await p.locator(".search-result").count(), 0);
      await close();
      if (await p.locator("dialog").evaluate((d) => d.open)) await close();
      await click("clear-state");
    },
  );
  await check(
    "Keyboard reference picker and read-only Customer 360",
    async () => {
      await nav("#/holdings/acme/rec/sales/WF-SALES");
      await click("cx-add-ref");
      await p.locator("#cx-query").fill("WF-INVENTORY");
      await p.keyboard.press("ArrowDown");
      assert.equal(
        await p.evaluate(() => document.activeElement.getAttribute("role")),
        "option",
      );
      await p.keyboard.press("Escape");
      assert.equal(await p.locator("#reference-picker").count(), 0);
      await nav("#/holdings/acme/rec/customer_360/WF-CUSTOMER_360");
      assert.equal(await p.locator('main form [type="submit"]').count(), 0);
    },
  );
  await check(
    "Business limits block unbalanced journals, excessive refunds, expired grants and closed enrollment",
    async () => {
      await persona("nisa");
      const set = async (key, value) => {
        const el = p.locator(`[data-form="task-draft"] [name="${key}"]`);
        await reveal(el);
        if (typeof value === "boolean") await el.setChecked(value);
        else await el.fill(String(value));
      };
      const blocked = async (text) => {
        assert((await p.locator("#task-issues").innerText()).includes(text));
        const before = (await record()).version;
        await p.locator('[value="review"]').click();
        assert.equal(
          await p.locator('[data-action="task-confirm"]').count(),
          0,
        );
        assert.equal((await record()).version, before);
      };
      await nav("#/holdings/acme/tasks/finance/WF-FINANCE");
      await set("debit", 2500);
      await blocked("balance");
      await set("debit", 2400);
      await nav("#/holdings/acme/tasks/identity_access/WF-IDENTITY_ACCESS");
      await set("days", 120);
      await blocked("90 days");
      await persona("mali");
      await nav("#/holdings/acme/tasks/benefits/WF-BENEFITS");
      await click("demo");
      await click("task-demo-window");
      await blocked("window");
      await nav("#/holdings/acme/tasks/pos/WF-POS");
      await set("tender", 10);
      await blocked("tender");
      await set("tender", 50);
      await p.locator('[value="review"]').click();
      await click("task-confirm");
      await p.waitForSelector(".task-outcome");
      await click("task-continue");
      await set("refund", 500);
      await blocked("original settlement");
    },
  );
  assert.deepEqual(errors, []);
  console.log(JSON.stringify({ checks: count, errors }));
} finally {
  await b.close();
}
