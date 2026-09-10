import { reveal, clickAction, clearDemoStorage } from "./verify-helpers.mjs";
import assert from "node:assert/strict";
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
  errors = [];
page.setDefaultTimeout(5000);
page.on("pageerror", (e) => errors.push(e.message));
const base = process.env.ERPX_MOCKUP_URL || "http://127.0.0.1:8017";
const click = (a, root = page) => clickAction(page, a, root);
const nav = async (h) => {
  await page.evaluate((x) => (location.hash = x), h);
  await page.waitForTimeout(60);
};
const person = async (id) => {
  await click("demo");
  await click("persona", page.locator("dialog"));
  await page.locator(`[data-action="choose-persona"][data-id="${id}"]`).click();
};
const clean = async () => {
  await page.goto(base);
  await clearDemoStorage(page);
  await page.reload();
  await page.waitForFunction(() => !!window.ERPX_TASKS);
};
const get = () =>
  page.evaluate(() => {
    const p = parseRoute(),
      r = findRecord(p.key, p.id);
    return {
      record: r,
      owner: taskOwner(r),
      draft: wfData(r),
      todos: taskItems().map((x) => x.id),
      waiting: taskItems("waiting").map((x) => x.id),
      count: records.length,
    };
  });
const commit = async () => {
  if (await page.locator('[data-form="task-draft"]').count())
    await page
      .locator('[data-form="task-draft"] button[value="review"]')
      .click();
  if (await page.locator('[data-action="task-confirm"]').count())
    await click("task-confirm");
  await page.waitForTimeout(720);
  assert(await page.locator(".task-outcome").isVisible());
};
let checks = 0;
try {
  await clean();
  await nav("#/holdings/acme/tasks/people_payroll/WF-PEOPLE_PAYROLL");
  await commit();
  let s = await get();
  assert.equal(s.record.status, "Pending approval");
  assert.equal(s.owner, "nisa");
  assert(s.waiting.includes(s.record.id));
  assert(!s.todos.includes(s.record.id));
  await click("role");
  await page.locator('[data-action="choose-role"][data-id="sales"]').click();
  await nav("#/holdings/acme/tasks/people_payroll/WF-PEOPLE_PAYROLL");
  assert.equal(await page.locator('[data-form="task-draft"]').count(), 0);
  await person("nisa");
  await nav("#/holdings/acme/tasks/people_payroll/WF-PEOPLE_PAYROLL");
  await commit();
  assert.equal((await get()).record.status, "Approved");
  checks++;
  console.log("PASS requester / reviewer responsibility and role preservation");

  await clean();
  await nav("#/holdings/acme/tasks/procurement/WF-PROCUREMENT");
  await commit();
  await person("nisa");
  await nav("#/holdings/acme/tasks/procurement/WF-PROCUREMENT");
  await click("demo");
  await page
    .locator('[data-action="scenario"][data-id="handoff-failure"]')
    .click();
  await commit();
  s = await get();
  assert.equal(s.record.status, "Approved");
  assert(s.record.pendingHandoff);
  const count = s.count,
    events = s.record.taskEvents.length;
  await click("task-retry-handoff");
  s = await get();
  assert.equal(s.count, count + 1);
  assert.equal(s.record.taskEvents.length, events);
  assert.equal(s.record.pendingHandoff, null);
  await page.locator(".prepared-draft-card").click();
  s = await get();
  assert.equal(s.record.status, "Draft");
  assert.equal(s.record.recordType, "purchase_order");
  assert.equal(s.record.fields.quantity, 100);
  assert.equal(s.record.fields.amount, 2400);
  checks++;
  console.log(
    "PASS failed handoff retries only draft creation and preserves copied information",
  );

  await clean();
  await person("nisa");
  await nav("#/holdings/acme/tasks/finance/WF-FINANCE");
  await commit();
  await click("task-continue");
  await page.locator('[name="reconciled"]').check();
  await commit();
  await page.locator(".prepared-draft-card").click();
  s = await get();
  assert.equal(s.owner, "pim");
  assert.equal(await page.locator('[data-form="task-draft"]').count(), 0);
  const period = s.record.id;
  await click("role");
  await page.locator('[data-action="choose-role"][data-id="finance"]').click();
  await nav("#/holdings/acme/tasks/finance/" + period);
  assert.equal(await page.locator('[data-form="task-draft"]').count(), 0);
  await person("pim");
  await nav("#/holdings/acme/tasks/finance/" + period);
  await commit();
  assert.equal((await get()).record.status, "Closed");
  await person("nisa");
  await nav("#/holdings/acme/ctx/finance");
  await click("wf-new");
  await page.waitForTimeout(70);
  if (await page.locator('[data-action="task-action-mode"]').count())
    await click("task-action-mode");
  await page.waitForTimeout(70);
  assert.match(
    await page.locator("#task-issues").innerText(),
    /period is closed/,
  );
  checks++;
  console.log("PASS independent close and closed-period posting gate");

  await clean();
  await nav("#/holdings/acme/tasks/sales/WF-SALES");
  await reveal(page.locator('[name="notes"]'));
  await page
    .locator('[name="notes"]')
    .fill("Keep this draft through an interruption.");
  await click("demo");
  await click("task-demo-change", page.locator("dialog"));
  if (await page.locator('[data-form="task-draft"]').count())
    await page
      .locator('[data-form="task-draft"] button[value="review"]')
      .click();
  assert(
    await page.getByRole("heading", { name: "The record changed" }).isVisible(),
  );
  assert.equal(
    (await get()).draft.notes,
    "Keep this draft through an interruption.",
  );
  await click("task-rebase");
  assert.equal(
    (await get()).draft.amount,
    2401,
    "Untouched fields use the newer source value",
  );
  if (await page.locator('[data-form="task-draft"]').count())
    await page
      .locator('[data-form="task-draft"] button[value="review"]')
      .click();
  await click("task-edit");
  assert.equal(
    await page.locator('[name="notes"]').inputValue(),
    "Keep this draft through an interruption.",
  );
  checks++;
  console.log(
    "PASS source version conflict retains the draft and requires a new review",
  );

  await click("task-help");
  const comment = page.locator('[data-form="wf-comment"] textarea');
  assert((await comment.inputValue()).startsWith("Could you help"));
  assert.equal((await get()).record.comments.length, 0);
  await comment.fill("Please review this task with me.");
  await click("theme");
  assert.equal(await comment.inputValue(), "Please review this task with me.");
  await page.locator('[data-form="wf-comment"] button[type="submit"]').click();
  assert.equal((await get()).record.comments.length, 1);
  checks++;
  console.log("PASS help is a draft and discussion survives interface changes");

  await clean();
  await nav("#/holdings/acme/tasks/sales/WF-SALES");
  await click("task-assign");
  await page.locator('dialog select[name="person"]').selectOption("nisa");
  await page.locator('dialog button[type="submit"]').click();
  assert.equal((await get()).owner, "nisa");
  assert.equal(await page.locator('[data-form="task-draft"]').count(), 0);
  await person("arun");
  await nav("#/holdings/acme/tasks/sales/WF-SALES");
  assert.match(
    await page.locator("main").innerText(),
    /outside your workspace/,
  );
  assert(!(await page.locator("main").innerText()).includes("Riverstone"));
  checks++;
  console.log("PASS reassignment and cross-customer task isolation");

  await clean();
  await nav("#/holdings/acme/tasks/assets/WF-ASSETS");
  await commit();
  const asset = await get();
  assert(!asset.record.finished);
  assert(asset.record.taskWaitFor);
  assert(!asset.todos.includes(asset.record.id));
  await page.locator(".prepared-draft-card").click();
  // Prepared maintenance drafts intentionally do not invent a date or technician.
  await page.getByLabel("Scheduled date", { exact: true }).fill("2026-09-06");
  await page.locator('[data-field="technician"] input').fill("Dao Arun");
  await page.getByLabel("Downtime (minutes)", { exact: true }).fill("45");
  await commit();
  await click("task-continue");
  await commit();
  await click("task-continue");
  await click("task-evidence");
  await commit();
  await nav("#/holdings/acme/tasks/assets/WF-ASSETS");
  await click("task-continue");
  await commit();
  assert.equal((await get()).record.status, "In service");
  checks++;
  console.log(
    "PASS asset handoff waits for maintenance evidence before return to service",
  );
  await clean();
  await nav("#/holdings/acme/tasks/sales/WF-SALES");
  await click("demo");
  await click("task-demo-unassigned", page.locator("dialog"));
  assert.match(await page.locator("main").innerText(), /needs an owner/);
  assert.equal(await page.locator('[data-form="task-draft"]').count(), 0);
  await click("task-assign");
  await page.locator('dialog select[name="person"]').selectOption("mali");
  await page.locator('dialog button[type="submit"]').click();
  assert(await page.locator('[data-form="task-draft"]').isVisible());
  checks++;
  console.log(
    "PASS unavailable assignee recovers through an eligible explicit assignment",
  );

  await clean();
  await nav("#/holdings/acme/tasks/pos/WF-POS");
  await commit();
  assert(
    !(await get()).todos.includes("WF-POS"),
    "A completed sale does not create a routine refund task",
  );
  await click("task-continue");
  await page.locator('[name="refund"]').fill("12.5");
  if (await page.locator('[data-form="task-draft"]').count())
    await page
      .locator('[data-form="task-draft"] button[value="review"]')
      .click();
  const refundReview = await page.locator(".task-review").innerText();
  assert(refundReview.includes("Requested refund (USD)"));
  assert(refundReview.includes("12.5"));
  assert(refundReview.includes("$48.00"));
  checks++;
  console.log(
    "PASS optional refund review shows the requested amount and remaining settlement",
  );
  assert.deepEqual(errors, []);
  console.log(JSON.stringify({ checks, errors }));
} finally {
  await browser.close();
}
