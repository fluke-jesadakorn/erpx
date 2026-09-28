import { expect, type BrowserContext, type Page, type Locator } from "@playwright/test";
import type { OrganizationAccessData } from "../../src/lib/organization-access";
import type { DirectoryAutomationData } from "../../src/lib/directory-operations";

export async function organizationSession(context: BrowserContext, role: "hr_admin" | "it_admin") {
  const login = await context.request.post("/api/auth/sign-in/email", { data: {
    email: role === "hr_admin" ? process.env.ERPX_E2E_HR_EMAIL ?? "lara.suwan@erpx.dev" : process.env.ERPX_E2E_ADMIN_EMAIL ?? "jesadakorn.kirtnu@gmail.com",
    password: role === "hr_admin" ? process.env.ERPX_E2E_HR_PASSWORD ?? "12345678" : process.env.ERPX_E2E_ADMIN_PASSWORD ?? "12345678",
  } });
  expect(login.ok(), await login.text()).toBe(true);
  const response = await context.request.get("/api/v1/holdings/acme/assignments/active");
  expect(response.ok(), await response.text()).toBe(true);
  const state = (await response.json()).data as { companyId: string; revision: number; options: { id: string; role: string; href: string }[] };
  const assignment = state.options.find(option => option.role === role && !option.href.includes("/branches/"));
  expect(assignment, `Seeded account needs Company ${role} authority`).toBeTruthy();
  const selection = await context.request.post("/api/v1/holdings/acme/assignments/active", { headers: { "x-erpx-context-company": state.companyId }, data: { assignmentId: assignment!.id, revision: state.revision } });
  expect(selection.ok(), await selection.text()).toBe(true);
  return assignment!.href.replace(/\/home$/, "");
}

export async function readOrganization(context: BrowserContext, base: string) {
  const response = await context.request.get(`/api/v1${base}/technology/organization`);
  expect(response.ok(), await response.text()).toBe(true);
  return (await response.json()).data as OrganizationAccessData;
}

export async function readDirectory(context: BrowserContext, base: string) {
  const response = await context.request.get(`/api/v1${base}/people/directory-operations`);
  expect(response.ok(), await response.text()).toBe(true);
  return (await response.json()).data as DirectoryAutomationData;
}

/** The initial moves fetch runs after React attaches the HR interaction handlers. */
export async function openHrOrganization(page: Page, url: string) {
  const hydrated = page.waitForResponse(response => response.request().method() === "GET" && response.url().endsWith("/people/organization/position-moves"));
  await page.goto(url);
  await hydrated;
}

/** Browser confirmations never move real staff or change Directory access. */
export async function interceptOrganizationWrites(page: Page) {
  const writes: { path: string; body: Record<string, unknown> }[] = [];
  await page.route(/\/api\/v1\/.*\/(?:technology\/(?:gpo-rules|position-gpo)|people\/(?:organization\/position-moves|lifecycle\/placements))\/(?:submit|publish|remove|unmap|retry|retry-config|[^/]+\/cancel)$/, async route => {
    const request = route.request();
    if (request.method() !== "POST") return route.continue();
    const body = request.postDataJSON() as Record<string, unknown>;
    writes.push({ path: new URL(request.url()).pathname, body });
    await route.fulfill({ status: 200, contentType: "application/json", body: JSON.stringify({ data: { id: "browser-confirmation-only", version: 1, status: "scheduled", effectiveAt: body.effectiveAt } }) });
  });
  return writes;
}

export async function selectExplorerItem(page: Page, kind: "position" | "person" | "department" | "level", id: string, keyboard = false) {
  const row = page.getByTestId(`org-${kind}-${id}`), selectionButton = row.locator("button").last();
  if (keyboard) { await selectionButton.focus(); await page.keyboard.press("Enter"); }
  else await selectionButton.click();
  await expect(row).toHaveAttribute("data-selected", "true");
  return page.getByRole("region", { name: "Selected item details" }).filter({ visible: true });
}

export async function dragOrganizationItem(page: Page, source: Locator, target: Locator) {
  const dataTransfer = await page.evaluateHandle(() => new DataTransfer());
  await source.dispatchEvent("dragstart", { dataTransfer });
  await target.dispatchEvent("dragover", { dataTransfer });
  await target.dispatchEvent("drop", { dataTransfer });
  await source.dispatchEvent("dragend", { dataTransfer });
  await dataTransfer.dispose();
}
