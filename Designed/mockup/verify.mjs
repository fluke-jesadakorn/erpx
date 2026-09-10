import { reveal, clickAction } from "./verify-helpers.mjs";
/* Run against a local static server. See README.md for setup; writes screenshots only to /tmp by default. */
import assert from "node:assert/strict";
import fs from "node:fs";
import path from "node:path";
import { pathToFileURL } from "node:url";
const { chromium } = await import(
  process.env.ERPX_PLAYWRIGHT
    ? pathToFileURL(path.join(process.env.ERPX_PLAYWRIGHT, "index.mjs")).href
    : "playwright"
);
const base = process.env.ERPX_MOCKUP_URL || "http://127.0.0.1:8017";
const screenshots = process.env.ERPX_SCREENSHOTS || "/tmp/erpx-mockup-review";
(async () => {
  fs.mkdirSync(screenshots, { recursive: true });
  const browser = await chromium.launch({
    headless: true,
    ...(process.env.ERPX_CHROME_PATH
      ? { executablePath: process.env.ERPX_CHROME_PATH }
      : {}),
  });
  const page = await browser.newPage({
    viewport: { width: 1440, height: 1000 },
  });
  const errors = [],
    failed = [],
    checks = [];
  page.on("pageerror", (e) => errors.push(e.message));
  page.on("response", (r) => {
    if (r.status() >= 400) failed.push(`${r.status()} ${r.url()}`);
  });
  const snap = () => page.evaluate(() => window.ERPX_DEMO.snapshot());
  const click = (action, id, root = page) =>
    clickAction(page, action, root, id);
  const close = () => click("close-dialog", undefined, page.locator("dialog"));
  const persona = async (id) => {
    await click("demo");
    await click("persona", undefined, page.locator("dialog"));
    await click("choose-persona", id);
  };
  const navigate = async (hash) => {
    await page.evaluate((h) => {
      location.hash = h;
    }, hash);
    await page.waitForTimeout(45);
  };
  const scenario = async (id) => {
    await click("demo");
    await click("scenario", id);
  };
  const check = async (name, fn) => {
    await fn();
    checks.push(name);
    console.log("PASS", name);
  };
  try {
    await page.goto(base);
    await page.waitForFunction(() => !!window.ERPX_DEMO);
    await check(
      "Initial personalized Home and static mockup",
      async () => {
        assert.equal((await snap()).state.persona, "mali");
        assert.equal((await snap()).moduleIds.length, 31);
        assert.equal(await page.locator(".feed-dim").count(), 0);
        assert.equal(
          await page.locator(".sidebar").getAttribute("inert"),
          null,
        );
        await page.screenshot({
          path: path.join(screenshots, "home-desktop.png"),
          fullPage: true,
        });
      },
    );
    await check(
      "Assigned role switching preserves person and narrows modules",
      async () => {
        await click("role");
        await click("choose-role", "sales");
        const s = await snap();
        assert.equal(s.state.persona, "mali");
        assert.equal(s.state.role, "sales");
        assert(!s.visibleRecordIds.includes("REQ-2026-014"));
        await click("role");
        await click("choose-role", "manager");
      },
    );
    await check(
      "Posts, comments, reactions, and follows change independently",
      async () => {
        const before = (await snap()).postCount;
        await click("compose");
        await page
          .locator("#post-text")
          .fill("QA branch handover — cartons checked.");
        await page.locator("#post-audience").selectOption("branch");
        await page.locator('#post-form [type="submit"]').click();
        await page.waitForFunction(
          (n) => ERPX_DEMO.snapshot().postCount === n + 1,
          before,
        );
        const article = page
          .locator("article.post")
          .filter({ hasText: "QA branch handover" });
        const id = await article.getAttribute("id");
        await article.locator('[data-action="like"]').click();
        assert.equal(
          await page
            .locator("#" + id + ' [data-action="like"]')
            .getAttribute("aria-pressed"),
          "true",
        );
        await page
          .locator("#" + id + ' [data-action="comments"]')
          .last()
          .click();
        await page.locator("#" + id + " input").fill("QA comment is kept.");
        await page.locator("#" + id + ' form [type="submit"]').click();
        assert(
          await page
            .locator("#" + id)
            .textContent()
            .then((t) => t.includes("QA comment is kept.")),
        );
        await page.locator("#" + id + " .post-follow-menu summary").click();
        await page.locator("#" + id + ' [data-action="follow"]').click();

        assert.equal(await page.locator("#" + id).count(), 1);
      },
    );
    await check(
      "Composer drafts survive role changes without leaking across roles",
      async () => {
        await click("compose");
        await page.locator("#post-text").fill("QA private manager draft");
        await close();
        await click("role");
        await click("choose-role", "sales");
        await click("compose");
        assert.equal(await page.locator("#post-text").inputValue(), "");
        await close();
        await click("role");
        await click("choose-role", "manager");
        await click("compose");
        assert.equal(
          await page.locator("#post-text").inputValue(),
          "QA private manager draft",
        );
        await close();
      },
    );
    await check("A failed publish keeps its draft and can retry", async () => {
      await scenario("failure");
      await click("compose");
      await page.locator("#post-text").fill("QA retry succeeds");
      const before = (await snap()).postCount;
      await page.locator('#post-form [type="submit"]').click();
      await page.waitForFunction(() =>
        document
          .querySelector("#post-status")
          ?.textContent.includes("Could not save"),
      );
      assert.equal((await snap()).postCount, before);
      assert.equal(
        await page.locator("#post-text").inputValue(),
        "QA retry succeeds",
      );
      await page.locator('#post-form [type="submit"]').click();
      await page.waitForFunction(
        (n) => ERPX_DEMO.snapshot().postCount === n + 1,
        before,
      );
    });
    await check("Story creation and previous / next navigation", async () => {
      await click("new-story");
      await page.locator("#post-text").fill("QA Story for our branch");
      await page.locator('#post-form [type="submit"]').click();
      await page.waitForSelector(
        '[data-action="story"][aria-label*="QA Story"]',
      );
      await page
        .locator('[data-action="story"][aria-label*="QA Story"]')
        .click();
      assert((await page.locator("dialog").textContent()).includes("QA Story"));
      await page
        .getByRole("button", { name: "Next Story", exact: true })
        .click();
      assert(
        !(await page.locator("dialog h3").textContent()).includes("QA Story"),
      );
      await page.keyboard.press("ArrowLeft");
      assert(
        (await page.locator("dialog h3").textContent()).includes("QA Story"),
      );
      await close();
    });
    await check(
      "Local Shorts play, have captions, and save for later",
      async () => {
        await navigate("#/holdings/acme/learn");
        for (const id of ["receive", "handover", "welcome"]) {
          await click("short", id);
          await page.waitForFunction(
            () => document.querySelector("video")?.readyState >= 1,
          );
          const media = await page.locator("video").evaluate(async (v) => {
            await v.play();
            return { duration: v.duration, tracks: v.textTracks.length };
          });
          assert(media.duration >= 11.9);
          assert.equal(media.tracks, 1);
          await page.waitForTimeout(150);
          assert(
            await page.locator("video").evaluate((v) => v.currentTime > 0),
          );
          await close();
        }
        await click("save-short", "receive");
        await click("learn-filter", "saved");
        assert.equal(await page.locator(".learn-card").count(), 1);
      },
    );
    await check(
      "Reactions and saved items belong to the correct demo person",
      async () => {
        await persona("nisa");
        await navigate("#/holdings/acme/learn");
        await click("learn-filter", "saved");
        assert.equal(await page.locator(".learn-card").count(), 0);
        await persona("mali");
        await navigate("#/holdings/acme/learn");
        await click("learn-filter", "saved");
        assert.equal(await page.locator(".learn-card").count(), 1);
      },
    );
    await check(
      "Branch restriction applies to records, feeds, and search",
      async () => {
        await navigate("#/holdings/acme/rec/sales/QT-2026-041");
        assert(
          (await page.locator("main").textContent()).includes(
            "Record unavailable",
          ),
        );
        await click("search");
        await page.locator("#search-input").fill("QT-2026-041");
        assert.equal(await page.locator(".search-result").count(), 0);
        await page.locator("#search-input").fill("confidential margin");
        assert.equal(await page.locator(".search-result").count(), 0);
        await close();
        await page.locator('.topbar [data-action="workspace"]').click();
        await page.locator('[name="branch"]').selectOption("cnx");
        await page.locator('#workspace-form [type="submit"]').click();
        assert.equal((await snap()).state.branch, "cnx");
        assert(!(await snap()).visibleRecordIds.includes("QT-2026-042"));
        await navigate("#/holdings/acme/rec/sales/QT-2026-041");
        assert(
          !(await page.locator("main").textContent()).includes(
            "Record unavailable",
          ),
        );
      },
    );
    await check(
      "Customer owner sees all branches and shared organization",
      async () => {
        await persona("nisa");
        assert.equal((await snap()).state.branch, "all");
        await navigate("#/holdings/acme/ctx/organization");
        assert.equal(await page.locator(".org-node.child").count(), 2);
        assert.equal(await page.locator(".org-node.grandchild").count(), 3);
      },
    );
    await check(
      "Customer isolation includes direct links, search, and previews",
      async () => {
        await page.locator('.topbar [data-action="workspace"]').click();
        await page.locator("#workspace-holding").selectOption("northstar");
        await page.locator('#workspace-form [type="submit"]').click();
        assert.equal((await snap()).state.holding, "northstar");
        assert(
          await page.evaluate(() =>
            visibleRecords().every((r) => r.holding === "northstar"),
          ),
        );
        assert(
          !(await page.locator("main").textContent()).includes(
            "QA branch handover",
          ),
        );
        await click("search");
        await page.locator("#search-input").fill("QA branch handover");
        assert.equal(await page.locator(".search-result").count(), 0);
        await close();
        await navigate("#/holdings/acme/rec/sales/QT-2026-042");
        assert(
          (await page.locator("main").textContent()).includes(
            "outside your workspace",
          ),
        );
        assert(await page.evaluate(() => ERPX_DEMO.assertScope()));
        await persona("nisa");
      },
    );
    await check(
      "All 31 modules and representative records are reachable",
      async () => {
        const keys = (await snap()).moduleIds;
        let visited = 0;
        for (const k of keys.filter((k) => k !== "platform_administration")) {
          await navigate("#/holdings/acme/ctx/" + k + "/records");
          assert.equal(await page.locator("main h1").count(), 1, k);
          const record = page.locator('a.work-record[href*="/rec/"]').first();
          assert(await record.count(), k);
          await record.click();
          await page.waitForSelector("#record-form");
          assert.equal(await page.locator("#record-form").count(), 1, k);
          visited++;
        }
        await persona("fluke");
        await navigate("#/platform/ctx/platform_administration/records");
        await page.locator('a.work-record[href*="/rec/"]').first().click();
        await page.waitForSelector("#record-form");
        assert.equal(await page.locator("#record-form").count(), 1);
        visited++;
        assert.equal(visited, 31);
        await navigate("#/platform");
        await page.screenshot({
          path: path.join(screenshots, "platform-desktop.png"),
          fullPage: true,
        });
        await persona("nisa");
      },
    );
    await check(
      "Typed forms create a draft and discussions remain attached",
      async () => {
        await navigate("#/holdings/acme/ctx/sales");
        await page.locator('[data-action="wf-new"]').click();
        await page.waitForFunction(() =>
          location.hash.includes("/tasks/sales/WF-"),
        );
        await reveal(page.locator('[data-form="task-draft"] [name="notes"]'));
        await page
          .locator('[data-form="task-draft"] [name="notes"]')
          .fill("QA complete quote details");
        await page.locator('[data-form="task-draft"] [value="save"]').click();
        await page.waitForTimeout(760);
        await reveal(page.locator('[data-form="wf-comment"] textarea'));
        await page
          .locator('[data-form="wf-comment"] textarea')
          .fill("QA new record discussion");
        await page.locator('[data-form="wf-comment"] [type="submit"]').click();
        assert(
          (await page.locator("main").textContent()).includes(
            "QA new record discussion",
          ),
        );
      },
    );
    await check(
      "Record failure preserves edits, retry saves, and role drafts stay scoped",
      async () => {
        await scenario("failure");
        await reveal(page.locator('[data-form="task-draft"] [name="notes"]'));
        await page
          .locator('[data-form="task-draft"] [name="notes"]')
          .fill("QA saved after retry");
        await page.locator('[data-form="task-draft"] [value="save"]').click();
        await page.waitForFunction(() =>
          document
            .querySelector("#task-status")
            ?.textContent.includes("Could not save"),
        );
        assert.equal(
          await page
            .locator('[data-form="task-draft"] [name="notes"]')
            .inputValue(),
          "QA saved after retry",
        );
        await page.locator('[data-form="task-draft"] [value="save"]').click();
        await page.waitForFunction(
          () =>
            document.querySelector("#task-status")?.textContent ===
            "Nothing submitted yet.",
        );
        assert(
          (await page.locator("main").textContent()).includes(
            "Saved task draft",
          ),
        );
      },
    );
    await check(
      "Consequential action requires review and records attribution",
      async () => {
        await page.locator('[name="recipient"]').fill("reviewer@example.test");
        await page
          .locator('[data-form="task-draft"] button[value="review"]')
          .click();
        assert.equal(await page.locator("#wf-confirm").count(), 0);
        await click("task-confirm");
        await page.waitForSelector(".task-outcome");
        await page.locator('[data-task-detail="details"] summary').click();
        assert(
          (await page.locator("main").textContent()).includes(
            "Nisa Chen · Customer owner",
          ),
        );
      },
    );
    await check("Stale record blocks action until refreshed", async () => {
      await scenario("stale");
      await navigate("#/holdings/acme/rec/sales/QT-2026-042");
      assert(
        (await page.locator("main").textContent()).includes(
          "This record has changed",
        ),
      );
      const review = page.locator('[data-action="review"]').first();
      await review.click();
      await page.locator("#review-confirm").check();
      await click("confirm-review");
      await page.waitForFunction(() =>
        document.querySelector("#review-status")?.textContent.includes("stale"),
      );
      await close();
      await click("refresh-record");
      assert.equal((await snap()).state.demoState, "normal");
    });
    await check("Denial, loading, empty, and success demo states", async () => {
      for (const id of ["denied", "saving", "empty"]) {
        await navigate("#/holdings/acme/home");
        await scenario(id);
        assert.equal((await snap()).state.demoState, id);
        await click("clear-state");
      }
      await scenario("success");
      assert.equal(await page.locator(".result").count(), 1);
      await close();
    });
    await check("Inbox read state and actionable deep links", async () => {
      await navigate("#/holdings/acme/inbox");
      await click("read-all");
      assert.equal(await page.locator(".notif-row .unread-dot").count(), 0);
      await page.locator('[data-action="notification"]').first().click();
      await page.waitForFunction(
        () =>
          location.hash.includes("/home") ||
          location.hash.includes("/rec/") ||
          location.hash.includes("/tasks/"),
      );
      await page.waitForTimeout(140);
    });
    await check(
      "Search keyboard shortcut, Escape, and focus return",
      async () => {
        await page.locator('.topbar [data-action="search"]').focus();
        await page.keyboard.press("Meta+k");
        assert(await page.locator("dialog").evaluate((d) => d.open));
        await page.keyboard.press("Escape");
        assert(!(await page.locator("dialog").evaluate((d) => d.open)));
        assert.equal(
          await page.evaluate(() => document.activeElement.dataset.action),
          "search",
        );
      },
    );
    await check(
      "Reset removes business state while keeping theme preferences",
      async () => {
        await click("theme");
        const dark = await page.evaluate(() =>
          document.documentElement.classList.contains("dark"),
        );
        await click("demo");
        await click("reset-confirm");
        await click("reset");
        assert.equal((await snap()).state.persona, "mali");
        assert.equal((await snap()).postCount, 12);
        assert.equal(
          await page.evaluate(() =>
            document.documentElement.classList.contains("dark"),
          ),
          dark,
        );
        await click("theme");
      },
    );
    await check(
      "Mobile and desktop layouts, hidden navigation focus, and dark theme",
      async () => {
        await page.setViewportSize({ width: 390, height: 844 });
        await page.waitForTimeout(100);
        assert.equal(
          await page.locator(".sidebar").evaluate((el) => el.inert),
          true,
        );
        const routes = [
          "home",
          "work",
          "community",
          "learn",
          "inbox",
          "ctx/sales",
          "rec/sales/QT-2026-042",
        ];
        for (const r of routes) {
          await navigate("#/holdings/acme/" + r);
          assert(
            await page.evaluate(
              () => document.documentElement.scrollWidth <= innerWidth,
            ),
            r,
          );
        }
        await navigate("#/holdings/acme/home");
        await page.screenshot({
          path: path.join(screenshots, "home-phone.png"),
          fullPage: true,
        });
        await page.locator(".mobile-menu").click();
        assert.equal(
          await page.locator(".sidebar").evaluate((el) => el.inert),
          false,
        );
        await page.locator('.sidebar [data-action="demo"]').click();
        await click("theme", undefined, page.locator("dialog"));
        await close();
        await page
          .locator(".mobile-overlay")
          .click({ position: { x: 300, y: 100 } });
        await page.screenshot({
          path: path.join(screenshots, "home-phone-dark.png"),
          fullPage: true,
        });
        await page.setViewportSize({ width: 1440, height: 1000 });
        await click("theme");
      },
    );
    assert.deepEqual(errors, [], "No browser runtime errors");
    assert.deepEqual(failed, [], "No failed asset requests");
    console.log(
      JSON.stringify(
        {
          passed: checks.length,
          browserErrors: errors,
          failedRequests: failed,
          screenshots,
        },
        null,
        2,
      ),
    );
  } catch (error) {
    await page.screenshot({
      path: path.join(screenshots, "failure.png"),
      fullPage: true,
    });
    console.error(error);
    console.error("Browser errors:", errors);
    process.exitCode = 1;
  } finally {
    await browser.close();
  }
})();
