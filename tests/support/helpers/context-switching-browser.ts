import { expect, type BrowserContext, type Page, type TestInfo } from "@playwright/test";

export type ActiveAssignment = { id: string; role: string; href: string; companyId: string };
export type ActiveRoleState = {
  assignmentId: string | null;
  companyId: string;
  revision: number;
  options: ActiveAssignment[];
};

export const activeAssignmentEndpoint = "/api/v1/holdings/acme/assignments/active";

export async function activeRoles(context: BrowserContext) {
  const response = await context.request.get(activeAssignmentEndpoint);
  expect(response.ok(), "Active work roles are available").toBe(true);
  return (await response.json()).data as ActiveRoleState;
}

export async function signIn(context: BrowserContext, email: string, password: string) {
  const origin = new URL(process.env.ERPX_E2E_BASE_URL ?? "http://127.0.0.1:3100").origin;
  const response = await context.request.post("/api/auth/sign-in/email", { headers: { Origin: origin }, data: { email, password } });
  expect(response.status(), `Authorized ${email} account can sign in`).toBe(200);
  return (await response.json()).user as { id: string; name: string; email: string };
}

/**
 * Prepare the owner scope without disturbing the page's signed-in account.
 * `context.request` shares cookies with the page, so signing in as the owner
 * on the same context would replace the persona these cases are asserting on.
 */
export async function ownerScope(context: BrowserContext, ownerEmail: string, ownerPassword: string) {
  const setup = await context.browser()!.newContext();
  try {
    const user = await signIn(setup, ownerEmail, ownerPassword);
    expect((await setup.request.post("/api/v1/holdings/acme/companies/active", { data: { companyId: "acme-co" } })).ok()).toBe(true);
    return { user, assignment: await selectRoleForSetup(setup, "holding_owner") };
  } finally {
    await setup.close();
  }
}

export async function selectRoleForSetup(context: BrowserContext, role: string) {
  const state = await activeRoles(context);
  const assignment = state.options.find(option => option.role === role && !option.href.includes("/branches/"));
  expect(assignment, `${role} assignment covers the Company workspace`).toBeTruthy();
  if (assignment!.id !== state.assignmentId) {
    const response = await context.request.post(activeAssignmentEndpoint, {
      headers: { "x-erpx-context-company": state.companyId },
      data: { assignmentId: assignment!.id, revision: state.revision },
    });
    expect(response.ok(), `Select ${role} for test setup`).toBe(true);
  }
  return assignment!;
}

/**
 * Register a second persona through the real account menu. Signing the second
 * account in via API would make it the active session, so the menu flow keeps
 * the test in control of which account is current for each step.
 */
export async function addAccountFromMenu(page: Page, email: string, password: string) {
  const menu = await openAccountMenu(page);
  const trigger = menu.getByRole("button", { name: "Add another account", exact: true });
  await trigger.click();
  const form = menu.locator("form.add-account-form");
  await expect(form).toBeVisible();
  await form.locator('input[name="email"]').fill(email);
  await form.locator('input[name="password"]').fill(password);
  await form.getByRole("button", { name: "Add account", exact: true }).click();
  // Adding an account completes a context switch, which closes the menu. Reopen
  // it so the caller observes the same menu state a person would see.
  await expect(page.getByRole("dialog", { name: "Account switcher", exact: true })).toBeHidden();
  return openAccountMenu(page);
}

export async function openAccountMenu(page: Page) {
  const menu = page.getByRole("dialog", { name: "Account switcher", exact: true });
  if (!await menu.isVisible()) {
    const trigger = page.getByRole("button", { name: /Account menu for/ });
    await expect(trigger).toBeEnabled();
    await expect(trigger).toBeVisible();
    const box = await trigger.boundingBox(), viewport = page.viewportSize();
    // Chromium's DOM.scrollIntoViewIfNeeded moves the document even when this
    // sticky trigger is visible. A real pointer preserves the user's scroll.
    if (box && viewport && box.y >= 0 && box.y + box.height <= viewport.height && box.x >= 0 && box.x + box.width <= viewport.width)
      await page.mouse.click(box.x + box.width / 2, box.y + box.height / 2);
    else await trigger.click();
  }
  await expect(menu).toBeVisible();
  return menu;
}

/** Open the role disclosure and match its role buttons without a search field. */
export async function matchingRoleOptions(page: Page, query: string) {
  const trigger = page.getByRole("button", { name: /^Switch role:/ });
  await expect(trigger).toBeEnabled();
  if (await trigger.getAttribute("aria-expanded") !== "true") await trigger.click();
  const roles = page.getByRole("group", { name: "Available roles", exact: true });
  await expect(roles).toBeVisible();
  const buttons = roles.getByRole("button");
  const escaped = query.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
  return query ? buttons.filter({ hasText: new RegExp(escaped, "i") }) : buttons;
}

export async function chooseRole(page: Page, role: string) {
  await openAccountMenu(page);
  const wanted = await matchingRoleOptions(page, role.replaceAll("_", " "));
  await expect(wanted.first()).toBeVisible();
  await wanted.first().click();
}

export async function expectWorkAssignment(page: Page, assignmentId: string) {
  await expect(page.locator("[data-work-assignment]")).toHaveAttribute("data-work-assignment", assignmentId, { timeout: 30_000 });
  await expect(page.locator("html")).not.toHaveAttribute("data-work-switching", "true");
  await expect(page.getByRole("dialog", { name: "Account switcher", exact: true })).not.toBeVisible();
  await expect(page.getByRole("button", { name: /Account menu for/ })).toBeEnabled();
}

/** A document-local marker and main-frame request count distinguish refresh from reload. */
export async function observeDocument(page: Page) {
  const marker = crypto.randomUUID();
  await page.evaluate(value => { document.documentElement.dataset.contextSwitchDocument = value; }, marker);
  const navigations: string[] = [];
  page.on("request", request => {
    if (request.isNavigationRequest() && request.frame() === page.mainFrame()) navigations.push(new URL(request.url()).pathname);
  });
  return async () => {
    expect(navigations, "Context switches make no document requests").toEqual([]);
    await expect(page.locator("html")).toHaveAttribute("data-context-switch-document", marker);
  };
}

/** Retain browser metrics only when scroll preservation fails. */
export async function observeScroll(page: Page) {
  await page.evaluate(() => {
    const events: Record<string, unknown>[] = [];
    const browser = window as typeof window & { contextSwitchScrollEvents: typeof events };
    browser.contextSwitchScrollEvents = events;
    function record(kind: string, extra: Record<string, unknown> = {}) {
      events.push({ kind, time: performance.now(), x: scrollX, y: scrollY, height: document.documentElement.scrollHeight, viewport: innerHeight, ...extra });
      if (events.length > 100) events.shift();
    }
    const original = window.scrollTo.bind(window);
    window.scrollTo = ((first: number | ScrollToOptions, second?: number) => {
      record("scrollTo", { target: typeof first === "number" ? { x: first, y: second } : first, caller: new Error().stack?.split("\n").slice(1, 5) });
      if (typeof first === "number") original(first, second ?? 0);
      else original(first);
    }) as typeof window.scrollTo;
    for (const kind of ["scroll", "pointerdown", "keydown", "wheel", "touchstart"])
      window.addEventListener(kind, () => record(kind), { passive: true });
    const trigger = document.querySelector<HTMLElement>('button[aria-label^="Account menu for"]');
    const topbar = trigger?.closest("header");
    record("initial", { trigger: trigger?.getBoundingClientRect().toJSON(), topbarPosition: topbar ? getComputedStyle(topbar).position : null });
  });
}

export async function expectPreservedScroll(page: Page, expected: number, info: TestInfo) {
  try {
    await expect.poll(() => page.evaluate(() => window.scrollY)).toBeCloseTo(expected, 0);
  } catch (error) {
    const evidence = await page.evaluate(() => (window as typeof window & { contextSwitchScrollEvents?: unknown }).contextSwitchScrollEvents);
    await info.attach("context-switch-scroll-metrics", { body: JSON.stringify({ expected, events: evidence }, null, 2), contentType: "application/json" });
    throw error;
  }
}
