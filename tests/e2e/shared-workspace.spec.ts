import { captureCaseScreenshot } from "../helpers/case-screenshot";
import {
  test,
  expect as baseExpect,
  type BrowserContext,
  type Locator,
  type Page,
} from "@playwright/test";
import { readFileSync, existsSync } from "node:fs";
import { randomUUID } from "node:crypto";
import { selectFixtureAssignment } from "./helpers/select-fixture-assignment";
const path = process.env.ERPX_SERVICES_FIXTURE ?? ".scratch/shared-workspace/fixture.json";
type Fixture = {
  beta?: boolean;
  company: string;
  branch: string;
  department: string;
  product: string;
  warehouse: string;
  supplier: string;
  password: string;
  actors: Record<
    string,
    { email: string; userId: string; assignmentId: string }
  >;
};
const fixture: Fixture = existsSync(path)
  ? JSON.parse(readFileSync(path, "utf8"))
  : null;
test.skip(!fixture, "Run bun scripts/seed-services-e2e.ts first.");
test.setTimeout(180000);
test.use({ actionTimeout: 20000 });
const expect = baseExpect.configure({ timeout: 20000 }),
  base = "/holdings/acme",
  api = `/api/v1${base}`;
const commandPages = new WeakMap<BrowserContext, Page>();
const commandActors = new WeakMap<BrowserContext, Fixture["actors"][string]>();
async function login(context: BrowserContext, who: string) {
  const response = await context.request.post("/api/auth/sign-in/email", {
    data: { email: fixture.actors[who].email, password: fixture.password },
  });
  expect(response.ok(), await response.text()).toBeTruthy();
  await selectFixtureAssignment(context, fixture.company, fixture.actors[who].assignmentId);
  commandActors.set(context, fixture.actors[who]);
}
async function commandHeaders(context: BrowserContext) {
  const actor = commandActors.get(context);
  expect(actor, "An existing fixture actor must be selected before a command.").toBeTruthy();
  let page = commandPages.get(context);
  if (!page) {
    page = await context.newPage();
    await page.goto(`${base}/home`);
    commandPages.set(context, page);
  }
  const work = page.locator("[data-work-assignment]");
  await expect(work).toHaveCount(1);
  await expect(work).toHaveAttribute("data-work-stale", "false");
  await expect(work).toHaveAttribute("data-work-company", fixture.company);
  await expect(work).toHaveAttribute("data-work-user", actor!.userId);
  await expect(work).toHaveAttribute("data-work-assignment", actor!.assignmentId);
  const stamp = await work.evaluate((element) => {
    const node = element as HTMLElement;
    return { revision: node.dataset.workRevision!, company: node.dataset.workCompany!, assignment: node.dataset.workAssignment! };
  });
  expect(stamp.revision).toMatch(/^\d+$/);
  return {
    Origin: new URL(page.url()).origin,
    "x-erpx-context-revision": stamp.revision,
    "x-erpx-context-company": stamp.company,
    "x-erpx-context-assignment": stamp.assignment,
  };
}
async function post(
  context: BrowserContext,
  route: string,
  data: unknown,
  key = randomUUID(),
) {
  const response = await context.request.post(`${api}/${route}`, {
    headers: {
      "Idempotency-Key": key,
      ...await commandHeaders(context),
    },
    data,
  });
  expect(response.ok(), await response.text()).toBeTruthy();
  return (await response.json()).data;
}
async function read(context: BrowserContext, id: string) {
  const r = await context.request.get(`${api}/requests/${id}`);
  expect(r.ok(), await r.text()).toBeTruthy();
  return (await r.json()).data;
}
async function teams(context: BrowserContext) {
  const response = await context.request.get(`${api}/service-teams`);
  expect(response.ok(), await response.text()).toBeTruthy();
  const data = (await response.json()).data;
  for (const category of ["it", "operations", "purchasing"])
    if (
      !data.teams.some(
        (t: { active: number; coverage: { category: string }[] }) =>
          t.active && t.coverage.some((c) => c.category === category),
      )
    )
      await post(context, "service-teams", {
        name: `Shared ${category}`,
        members: [{ userId: fixture.actors.it.userId, responsibility: "lead" }],
        coverage: [
          { category, scopeKind: "holding", scopeNodeId: fixture.company },
        ],
      });
}
async function clickAction(
  page: Page,
  label: string,
  note = "Reviewed with the requester and confirmed evidence",
) {
  const evidence = page.getByLabel("Message or decision evidence");
  await expect(evidence).toBeVisible();
  await expect(evidence).toBeEnabled();
  let action: Locator;
  if (label === "Approve" || label === "Return for correction") {
    const decision = page.getByRole("region", { name: "Your decision", exact: true });
    await expect(decision).toHaveCount(1);
    await decision.getByRole("button", { name: label, exact: true }).click();
    const confirmation = decision.locator("form");
    await expect(confirmation).toHaveCount(1);
    action = confirmation.getByRole("button", { name: `Confirm ${label}`, exact: true });
    await expect(action).toHaveAttribute("data-client-managed-ready", "true");
    const reason = confirmation.getByLabel("Reason", { exact: true });
    await reason.fill(note);
    await expect(reason).toHaveValue(note);
  } else {
    await evidence.fill(note);
    await expect(evidence).toHaveValue(note);
    action = page.getByRole("button", { name: label, exact: true });
  }
  await expect(action).toHaveCount(1);
  await expect(action).toBeVisible();
  const response = page.waitForResponse(
    (r) =>
      r.url().includes("/requests/") &&
      r.url().endsWith("/actions") &&
      r.request().method() === "POST",
  );
  await action.click();
  const r = await response;
  expect(r.ok(), await r.text()).toBeTruthy();
  await page.reload();
}
async function saveUi(page: Page) {
  const pending = page.waitForResponse(
    (r) => r.url().includes("/requests") && r.request().method() === "POST",
  );
  await page.getByRole("button", { name: "Save draft", exact: true }).click();
  const r = await pending;
  expect(r.ok(), await r.text()).toBeTruthy();
  const id = (await r.json()).data.id;
  await expect(page).toHaveURL(new RegExp(`/requests/${id}`));
  await page.reload();
  return String(id);
}
async function openNewRequest(page: Page, kind: string) {
  await page.getByRole("button", { name: "New", exact: true }).click();
  await page
    .getByRole("dialog", { name: "New work", exact: true })
    .getByRole("link", { name: `${kind} request`, exact: true })
    .click();
  await expect(page.getByLabel("Title", { exact: true })).toBeVisible();
}
async function submitUi(page: Page) {
  await page.getByRole("button", { name: "Review & submit", exact: true }).click();
  const confirm = page.getByRole("button", { name: "Confirm & submit", exact: true });
  await expect(confirm).toBeVisible();
  const pending = page.waitForResponse(
    response => response.url().includes("/requests/") && response.url().endsWith("/actions") && response.request().method() === "POST",
  );
  await confirm.click();
  const response = await pending;
  expect(response.ok(), await response.text()).toBeTruthy();
  await page.reload();
}
async function createUi(page: Page, kind: string, title: string) {
  await page.goto(`${base}/work?queue=all&responsibility=requested`);
  await openNewRequest(page, kind);
  await page.getByLabel("Title", { exact: true }).fill(title);
  await page
    .getByLabel(
      kind === "Service" ? "Describe the issue" : "Why is it needed?",
    )
    .fill(
      "Office printer and equipment required for the customer support team.",
    );
  await page.locator("summary").filter({ hasText: "Change branch or department (if needed)" }).click();
  await page
    .getByRole("combobox", { name: "Branch", exact: true })
    .selectOption(fixture.branch);
  await page
    .getByRole("combobox", { name: "Department", exact: true })
    .selectOption(fixture.department);
  await page.getByLabel("Location", { exact: true }).fill("Office desk 12");
  if (kind !== "Service") {
    await page.getByLabel("Needed by").fill("2026-09-20");
    await page.getByLabel("Item 1", { exact: true }).fill("Office laptop");
    await page.getByLabel("Quantity", { exact: true }).fill("1");
  }
  return saveUi(page);
}
test("owner configures coverage through Organization and members retain drafts without a profile", async ({
  page,
  context,
  browser,
}) => {
  await login(context, "owner");
  await page.goto(`${base}/settings?section=approvals&tab=templates#service-teams`);
  await expect(
    page.getByRole("heading", { name: "Service teams", exact: true }),
  ).toBeVisible();
  const card = page.locator("article").filter({
    has: page.getByRole("heading", { name: "IT support", exact: true }),
  });
  await card.getByRole("button", { name: /Set up team|Add coverage/ }).click();
  const dialog = page.getByRole("dialog");
  await expect(dialog.getByLabel("Team name")).toBeVisible();
  await page.keyboard.press("Escape");
  await expect(dialog).toBeHidden();
  await teams(context);
  const member = await browser.newContext(),
    memberPage = await member.newPage();
  await login(member, "missing");
  await memberPage.goto(
    `${base}/work?queue=all&responsibility=requested&type=service_request&q=Draft`,
  );
  await openNewRequest(memberPage, "Service");
  await memberPage
    .getByLabel("Title", { exact: true })
    .fill(`Draft recovery ${randomUUID()}`);
  const title = await memberPage
    .getByLabel("Title", { exact: true })
    .inputValue();
  await memberPage.reload();
  await openNewRequest(memberPage, "Service");
  await expect(memberPage.getByLabel("Title", { exact: true })).toHaveValue(
    title,
  );
  const id = await saveUi(memberPage);
  expect((await read(member, id)).request.state).toBe("draft");
  await memberPage.getByRole("link", { name: "← Back to Work" }).click();
  await expect(memberPage).toHaveURL(/responsibility=requested/);
  await expect(memberPage).toHaveURL(/type=service_request/);
  await expect(memberPage).toHaveURL(/q=Draft/);
  await memberPage.goBack();
  await expect(memberPage.getByLabel("Title", { exact: true })).toHaveValue(
    title,
  );
  await member.close();
});
test("service request moves through a shared team, questions, evidence, reopening and requester confirmation", async ({
  page,
  context,
  browser,
}) => {
  const owner = await browser.newContext();
  await login(owner, "owner");
  await teams(owner);
  await owner.close();
  await login(context, "missing");
  const title = `Service journey ${randomUUID()}`,
    id = await createUi(page, "Service", title);
  await page
    .getByLabel("Describe the issue")
    .fill(
      "Printer stops after one page. Preserve this human description if AI is unavailable.",
    );
  await saveUi(page);
  const png = Buffer.from(
    await page.evaluate(() => {
      const canvas = document.createElement("canvas");
      canvas.width = 32;
      canvas.height = 32;
      const ctx = canvas.getContext("2d")!;
      ctx.fillStyle = "white";
      ctx.fillRect(0, 0, 32, 32);
      ctx.fillStyle = "black";
      ctx.fillText("Tray", 0, 16);
      return canvas.toDataURL("image/png").split(",")[1];
    }),
    "base64",
  );
  const uploaded = page.waitForResponse(
    (r) =>
      r.url().includes("/uploads/") &&
      r.url().endsWith("/content") &&
      r.request().method() === "PUT",
  );
  await page.getByRole("tab", { name: "Evidence", exact: true }).click();
  await page
    .getByLabel("Attach photo or document")
    .setInputFiles({
      name: "printer-tray.png",
      mimeType: "image/png",
      buffer: png,
    });
  expect((await uploaded).ok()).toBeTruthy();
  await page.reload();
  const evidence = (await read(context, id)).attachments;
  expect(evidence).toHaveLength(1);
  const stranger = await browser.newContext();
  await login(stranger, "claimant");
  const inaccessible = await stranger.request.get(
    `${api}/attachments/${evidence[0].id}/content`,
  );
  expect(inaccessible.status()).toBe(404);
  await stranger.close();
  await page.getByRole("tab", { name: "Overview", exact: true }).click();
  await page.locator("summary").filter({ hasText: "Ask AI about this request" }).click();
  await page.getByText("AI preparation", { exact: true }).click();
  await expect(
    page.getByText(/unavailable|not configured|set up|configure/i).first(),
  ).toBeVisible();
  await submitUi(page);
  expect((await read(context, id)).request.state).toBe("queued");
  const handler = await browser.newContext(),
    hp = await handler.newPage();
  await login(handler, "it");
  await hp.goto(`${base}/work?responsibility=team`);
  await expect(hp.getByText(title, { exact: true })).toBeVisible();
  await hp.goto(`${base}/requests/${id}`);
  await hp
    .getByRole("combobox", { name: "Handler", exact: true })
    .selectOption(fixture.actors.it.userId);
  await clickAction(hp, "Assign handler");
  await clickAction(hp, "Start work");
  await clickAction(hp, "Ask requester", "Which printer tray is affected?");
  await page.reload();
  await expect(
    page.getByText("Which printer tray is affected?", { exact: true }).first(),
  ).toBeVisible();
  await clickAction(page, "Send reply", "The lower paper tray is affected.");
  await hp.reload();
  await clickAction(
    hp,
    "Resolve issue",
    "Replaced the tray roller and verified five test pages.",
  );
  await page.reload();
  await clickAction(
    page,
    "Reopen request",
    "Paper still jams with duplex printing.",
  );
  await hp.reload();
  await clickAction(
    hp,
    "Resolve issue",
    "Adjusted the duplex tray and tested together.",
  );
  await page.reload();
  await clickAction(page, "Confirm completion");
  expect((await read(context, id)).request.state).toBe("closed");
  await handler.close();
});
test("equipment requires independent approval then records a handover without a financial posting", async ({
  page,
  context,
  browser,
}) => {
  const owner = await browser.newContext();
  await login(owner, "owner");
  await teams(owner);
  await owner.close();
  await login(context, "claimant");
  const id = await createUi(
    page,
    "Equipment",
    `Equipment journey ${randomUUID()}`,
  );
  await submitUi(page);
  await expect
    .poll(async () => (await read(context, id)).request.state)
    .toBe("awaiting_approval");
  const head = await browser.newContext(),
    headPage = await head.newPage();
  await login(head, "head");
  await headPage.goto(`${base}/requests/${id}`);
  await clickAction(
    headPage,
    "Return for correction",
    "Specify the device for the support desk.",
  );
  await page.reload();
  await expect(page.getByLabel("Why is it needed?")).toHaveValue(
    /customer support/,
  );
  await saveUi(page);
  await submitUi(page);
  await expect
    .poll(async () => (await read(context, id)).request.state)
    .toBe("awaiting_approval");
  await headPage.reload();
  await clickAction(headPage, "Approve");
  const handler = await browser.newContext(),
    hp = await handler.newPage();
  await login(handler, "it");
  await hp.goto(`${base}/requests/${id}`);
  await hp
    .getByRole("combobox", { name: "Handler", exact: true })
    .selectOption(fixture.actors.it.userId);
  await clickAction(hp, "Assign handler");
  await clickAction(hp, "Start work");
  await hp.getByLabel("Existing asset reference (optional)").fill("LAPTOP-42");
  await clickAction(
    hp,
    "Record handover",
    "Handed one laptop and power adapter to the requester.",
  );
  await page.reload();
  await clickAction(page, "Confirm completion");
  const d = await read(context, id);
  expect(JSON.parse(d.request.handover_json).assetReference).toBe("LAPTOP-42");
  expect(d.request.state).toBe("closed");
  await head.close();
  await handler.close();
});
test("purchase requests hand off once to the existing approved and received order workflow", async ({
  page,
  context,
  browser,
}) => {
  await login(context, "claimant");
  const owner = await browser.newContext(),
    handler = await browser.newContext(),
    head = await browser.newContext();
  await login(owner, "owner");
  await login(handler, "it");
  await login(head, "head");
  await teams(owner);
  const r = await post(context, "requests", {
    kind: "purchase",
    title: `Purchase journey ${randomUUID()}`,
    description: "Two laptops for the customer support desk.",
    location: "Office desk",
    branchId: fixture.branch,
    departmentId: fixture.department,
    neededDate: "2026-09-20",
    items: [{ description: "Office laptop", quantity: "2" }],
  });
  async function action(c: BrowserContext, action: string, extra = {}) {
    const d = await read(c, r.id);
    return post(c, `requests/${r.id}/actions`, {
      action,
      expectedVersion: d.request.version,
      note: "Purpose and supporting evidence reviewed",
      ...extra,
    });
  }
  await action(context, "submit");
  await action(head, "approve");
  await action(handler, "assign", { assigneeUserId: fixture.actors.it.userId });
  await action(handler, "start");
  const hp = await handler.newPage();
  await hp.goto(`${base}/requests/${r.id}`);
  if (fixture.beta) {
    await expect(hp.getByText("Purchasing execution is unavailable in this beta workspace. Your approved request remains saved.")).toBeVisible();
    await expect(hp.getByRole("link", { name: "Prepare purchase order in Purchasing" })).toHaveCount(0);
    const denied = await handler.request.post(`${api}/commerce/orders`, {
      headers: { ...await commandHeaders(handler), "Idempotency-Key": randomUUID() },
      data: { kind: "purchase", requestId: r.id },
    });
    expect(denied.status()).toBe(403);
    expect((await denied.json()).error.code).toBe("BETA_CAPABILITY_UNAVAILABLE");
    expect((await read(context, r.id)).request.state).toBe("in_progress");
    await owner.close();
    await handler.close();
    await head.close();
    return;
  }
  await hp
    .getByRole("link", { name: "Prepare purchase order in Purchasing" })
    .click();
  await expect(
    hp.getByRole("heading", { name: "Approved request", exact: true }),
  ).toBeVisible();
  const detail = await read(handler, r.id),
    input = {
      kind: "purchase",
      requestId: r.id,
      requestVersion: detail.request.version,
      partyId: fixture.supplier,
      warehouseId: fixture.warehouse,
      orderDate: "2026-09-08",
      dueDate: "2026-09-20",
      note: "Prepared from the independently approved request",
      lines: [
        { productId: fixture.product, quantity: "2", price: "100", taxBps: 0 },
      ],
    },
    key = randomUUID();
  const order = await post(handler, "commerce/orders", input, key);
  expect(await post(handler, "commerce/orders", input, key)).toEqual(order);
  expect((await read(handler, r.id)).request.purchase_order_id).toBe(order.id);
  async function orderData() {
    const response = await handler.request.get(
      `${api}/commerce/orders/${order.id}`,
    );
    expect(response.ok(), await response.text()).toBeTruthy();
    return (await response.json()).data;
  }
  let data = await orderData();
  expect(data.order.branch_id).toBe(fixture.branch);
  expect(data.order.department_id).toBe(fixture.department);
  await post(handler, `commerce/orders/${order.id}/transition`, {
    action: "submit",
    expectedVersion: data.order.version,
  });
  data = await orderData();
  await post(owner, `commerce/orders/${order.id}/transition`, {
    action: "approve",
    expectedVersion: data.order.version,
  });
  data = await orderData();
  await post(handler, `commerce/orders/${order.id}/fulfill`, {
    expectedVersion: data.order.version,
    date: "2026-09-08",
    lines: [{ lineId: data.lines[0].id, quantity: "1" }],
  });
  const partial = await read(handler, r.id),
    denied = await handler.request.post(`${api}/requests/${r.id}/actions`, {
      headers: { ...await commandHeaders(handler), "Idempotency-Key": randomUUID() },
      data: {
        action: "resolve",
        expectedVersion: partial.request.version,
        note: "Not all items received",
      },
    });
  expect(denied.status()).toBe(409);
  expect((await denied.json()).error.code).toBe("PURCHASE_NOT_RECEIVED");
  data = await orderData();
  await post(handler, `commerce/orders/${order.id}/fulfill`, {
    expectedVersion: data.order.version,
    date: "2026-09-08",
    lines: [{ lineId: data.lines[0].id, quantity: "1" }],
  });
  await action(handler, "resolve");
  await page.goto(`${base}/requests/${r.id}`);
  await clickAction(page, "Confirm completion");
  expect((await read(context, r.id)).request.state).toBe("closed");
  await owner.close();
  await handler.close();
  await head.close();
});
test("handbook editors publish a revision while employees cannot read or search the next draft", async ({
  page,
  context,
  browser,
}) => {
  await login(context, "owner");
  const title = `Guide ${randomUUID()}`;
  await page.goto(`${base}/handbook/new`);
  await page.getByLabel("Article title", { exact: true }).fill(title);
  await page
    .getByLabel("Article content")
    .fill("Published instructions: report office issues through Work.");
  const saved = page.waitForResponse(
    (r) => r.url().includes("/handbook") && r.request().method() === "POST",
  );
  await page.getByRole("button", { name: "Save draft", exact: true }).click();
  expect((await saved).ok()).toBeTruthy();
  await expect(page).toHaveURL(/handbook\/article-/);
  await page.reload();
  const published = page.waitForResponse(
    (r) => r.url().includes("/handbook/") && r.request().method() === "POST",
  );
  await page
    .getByRole("button", { name: "Publish saved revision", exact: true })
    .click();
  expect((await published).ok()).toBeTruthy();
  await page.reload();
  const url = new URL(page.url()),
    articleId = url.pathname.split("/").at(-1)!;
  await page.getByLabel("Article content").fill(`UNPUBLISHED-${title}`);
  await page.getByRole("button", { name: "Save draft", exact: true }).click();
  await page.reload();
  const reader = await browser.newContext(),
    rp = await reader.newPage();
  await login(reader, "missing");
  await rp.goto(`${base}/handbook/${articleId}`);
  await expect(
    rp.getByText("Published instructions: report office issues through Work.", {
      exact: true,
    }),
  ).toBeVisible();
  await expect(rp.getByText(`UNPUBLISHED-${title}`)).toHaveCount(0);
  const forbidden = await reader.request.get(
    `${api}/handbook/${articleId}?edit=1`,
  );
  expect(forbidden.status()).toBe(403);
  await rp.goto(
    `${base}/handbook?q=${encodeURIComponent("UNPUBLISHED-" + title)}`,
  );
  await expect(rp.getByRole("link", { name: title, exact: true })).toHaveCount(
    0,
  );
  await reader.close();
});
test("shared workspace supports keyboard access and 390, 768 and 1440px light and dark layouts", async ({
  page,
  context,
}, info) => {
  await login(context, "owner");

  for (const width of [390, 768, 1440])
    for (const theme of ["light", "dark"]) {
      await page.setViewportSize({ width, height: 900 });
      await page.addInitScript((theme) => {
        localStorage.setItem("erpx-theme", theme);
        document.documentElement.dataset.theme = theme;
      }, theme);
      for (const [label, route] of [
        ["work", `${base}/work?queue=all`],
        ["new", `${base}/work?new=equipment_request`],
        ["home", `${base}/home`],
        ["teams", `${base}/settings?section=approvals&tab=templates#service-teams`],
        ["handbook", `${base}/handbook`],
      ]) {
        await page.goto(route);
        await expect(page.locator("main")).toBeVisible();
        await page.evaluate(
          (theme) => (document.documentElement.dataset.theme = theme),
          theme,
        );
        const overflow = await page.evaluate(
          () => document.documentElement.scrollWidth > window.innerWidth + 1,
        );
        expect(overflow, `${label} ${width}px ${theme}`).toBe(false);
        const small = await page
          .locator(
            "main button:visible,main input:visible,main select:visible,main textarea:visible",
          )
          .evaluateAll((nodes) =>
            nodes
              .filter((n) => {
                const r = n.getBoundingClientRect();
                return (
                  r.height > 0 &&
                  r.height < 43.5 &&
                  !(
                    n instanceof HTMLInputElement &&
                    ["checkbox", "radio", "hidden"].includes(n.type)
                  )
                );
              })
              .map((n) => ({
                label: n.getAttribute("aria-label") || n.textContent,
                height: n.getBoundingClientRect().height,
              })),
          );
        if (width === 390)
          expect(small, `${label} mobile controls`).toEqual([]);
        await captureCaseScreenshot(page, `${info.project.name}-${label}-${width}-${theme}.png`, { fullPage: true });
      }
    }
  await page.goto(`${base}/work`);
  const button = page.getByRole("button", { name: "New", exact: true });
  await button.focus();
  await page.keyboard.press("Enter");
  await expect(page.getByRole("dialog")).toBeVisible();
  await page.keyboard.press("Tab");
  await expect(page.getByRole("dialog")).toContainText("Service request");
  await page.keyboard.press("Escape");
  await expect(button).toBeFocused();
});
