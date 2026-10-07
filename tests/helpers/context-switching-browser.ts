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

export async function roleSearchInput(page: Page) {
  const standard = page.getByRole("combobox", { name: "Role for actions & approvals", exact: true });
  const legacy = page.locator(".role-combobox");
  await expect(standard.or(legacy), "Exactly one rendered role selector is required").toHaveCount(1);
  if (await legacy.count() === 1) {
    await expect(legacy).toBeVisible();
    const search = legacy.getByRole("combobox", { name: "Search roles", exact: true });
    if (await search.count() === 0) {
      const trigger = legacy.getByRole("button", { name: /Role for actions & approvals/ });
      await expect(trigger).toHaveCount(1);
      await expect(trigger).toBeEnabled();
      await trigger.click();
    }
    await expect(search).toHaveCount(1);
    await expect(search).toBeVisible();
    await expect(search).toBeEnabled();
    return search;
  }
  await expect(standard).toBeVisible();
  await expect(standard).toBeEnabled();
  return standard;
}

export async function searchRoleOptions(page: Page, query: string) {
  const search = await roleSearchInput(page);
  await search.fill(query);
  await search.press("ArrowDown");
  await expect(search).toHaveAttribute("aria-expanded", "true");
  const roles = await page.locator(".role-combobox").count() === 1
    ? page.locator(".role-combobox").getByRole("listbox", { name: "Available roles", exact: true })
    : page.locator('[data-slot="combobox-popup"]').getByRole("listbox");
  await expect(roles).toHaveCount(1);
  return roles;
}

export async function chooseRole(page: Page, role: string) {
  await openAccountMenu(page);
  const roles = await searchRoleOptions(page, role.replaceAll("_", " "));
  await roles.getByRole("option").first().click();
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
