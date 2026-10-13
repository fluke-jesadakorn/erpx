import { expect, test } from "@playwright/test";
import { selectFixtureAssignment } from "../../support/browser/select-fixture-assignment";

const base = "/holdings/acme";
test.setTimeout(120_000);
test.beforeEach(async ({ context }) => {
  const response = await context.request.post("/api/auth/sign-in/email", {
    headers: { Origin: process.env.ERPX_E2E_BASE_URL ?? "http://127.0.0.1:3100" },
    data: {
      email: process.env.ERPX_E2E_ADMIN_EMAIL ?? "jesadakorn.kirtnu@gmail.com",
      password: process.env.ERPX_E2E_ADMIN_PASSWORD ?? "12345678",
    },
  });
  expect(response.status(), await response.text()).toBe(200);
  const company = await context.request.post(`/api/v1${base}/companies/active`, {headers:{Origin:process.env.ERPX_E2E_BASE_URL ?? "http://127.0.0.1:3100"},data:{companyId:"acme-co"}});
  expect(company.status()).toBe(200);
  await selectFixtureAssignment(context,"acme-co","dev_usr_jesadakorn_acme-co_holding_owner");
});

async function fitsViewport(page: import("@playwright/test").Page) {
  expect(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth + 1)).toBe(true);
}

async function chooseTopic(page: import("@playwright/test").Page, name: RegExp) {
  const topics = page.getByRole("navigation", {name:"HR settings topics"});
  if(!await topics.isVisible()) await page.getByRole("button",{name:/^Choose hr settings/}).click();
  await topics.getByRole("button",{name}).click();
}

test("HR settings uses a full workspace with six topics and form drawers", async ({ page }) => {
  test.setTimeout(180_000);
  await page.goto(`${base}/people/settings`);
  await expect(page).toHaveURL(/\/people\/settings$/);
  const workspace = page.getByTestId("hr-settings-workspace"), sections = workspace.getByRole("navigation", {name:"HR settings topics"});
  await expect(workspace.getByRole("heading",{name:"Staff levels",exact:true})).toBeVisible({timeout:30_000});
  if(!await sections.isVisible()) await page.getByRole("button",{name:/^Choose hr settings/}).click();
  await expect(sections.getByRole("button")).toHaveCount(6);
  await expect(workspace.getByRole("heading",{name:"Staff levels",exact:true})).toBeVisible();
  await expect(page.getByTestId("people-detail")).toHaveCount(0);
  await fitsViewport(page);
  await chooseTopic(page,/^Leave & overtime/);
  await expect(page).toHaveURL(/section=leave/);
  await workspace.locator("summary").filter({hasText:/^Create leave type/}).click();
  const editor = page.getByRole("dialog",{name:"Create leave type",exact:true});
  await expect(editor).toHaveAttribute("data-presentation","form-drawer");
  await expect(editor.getByLabel("Code",{exact:true})).toBeVisible();
  await editor.getByRole("button",{name:"Close dialog",exact:true}).click();
  await chooseTopic(page,/^Organization templates/);
  await expect(workspace.getByRole("heading",{name:"Organization templates",exact:true})).toBeVisible();
  await expect(workspace.locator("summary").filter({hasText:/^Create organization template/})).toBeVisible();
  await chooseTopic(page,/^Lifecycle handoff/);
  await workspace.locator("summary").filter({hasText:/^Configure lifecycle handoff/}).click();
  const lifecycle = page.getByRole("dialog",{name:"Configure lifecycle handoff",exact:true});
  await expect(lifecycle.getByRole("combobox",{name:"IT owner",exact:true})).toBeVisible();
  await lifecycle.getByRole("button",{name:"Close dialog",exact:true}).click();
  await page.reload();
  await expect(workspace.locator("summary").filter({hasText:/^Configure lifecycle handoff/})).toBeVisible();
  await page.goBack();
  await expect(workspace.getByRole("heading",{name:"Organization templates",exact:true})).toBeVisible();
  for (const theme of ["light","dark"]) {
    await page.evaluate(theme => {document.documentElement.dataset.theme = theme;},theme);
    for (const title of ["Staff levels","Leave & overtime","Holiday calendars","Pay groups","Organization templates","Lifecycle handoff"]) {
      await chooseTopic(page,new RegExp(`^${title}`));
      await expect(workspace.getByRole("heading",{name:title,exact:true})).toBeVisible();
      await fitsViewport(page);
    }
  }
});

test("leave policy wizard preserves draft and submits one reviewed command", async ({ page }) => {
  await page.goto(`${base}/people?panel=settings&section=leave`);
  const workspace = page.locator("#workspace-content:visible");
  await expect(workspace).toHaveCount(1);
  await workspace.getByRole("button", { name: "Create policy", exact: true }).click();
  const wizard = page.getByRole("dialog", { name: "Create leave policy", exact: true });
  await expect(wizard).toHaveCount(1);
  await expect(wizard).toBeVisible();
  await expect(wizard.getByRole("heading", { name: "Basics" })).toBeVisible();
  await wizard.getByLabel("Policy key").fill("browser-review-policy");
  await wizard.getByLabel("Version").fill("1");
  await wizard.getByLabel("Policy name").fill("Browser review policy");
  await wizard.getByLabel("Effective from").fill("2026-10-01");
  await wizard.getByLabel("Timezone").fill("Asia/Bangkok");
  await wizard.getByRole("button", { name: "Continue" }).click();
  await expect(wizard.getByRole("heading", { name: "Working days" })).toBeVisible();
  await wizard.getByLabel("Saturday").check();
  await wizard.getByRole("button", { name: "Back" }).click();
  await expect(wizard.getByLabel("Policy name")).toHaveValue("Browser review policy");
  await wizard.getByRole("button", { name: "Continue" }).click();
  await expect(wizard.getByLabel("Saturday")).toBeChecked();
  await wizard.getByRole("button", { name: "Continue" }).click();
  await expect(wizard.getByRole("heading", { name: "Leave rules" })).toBeVisible();
  await wizard.getByLabel("Leave type").selectOption({ index: 1 });
  await wizard.getByLabel("Grant units").fill("12");
  await wizard.getByRole("button", { name: "Continue" }).click();
  await expect(wizard.getByRole("heading", { name: "Review" })).toBeVisible();
  let calls = 0;
  let posted: Record<string, unknown> | null = null;
  await page.route("**/leave/admin", async route => {
    calls++;
    posted = route.request().postDataJSON();
    await route.fulfill({ status: calls === 1 ? 422 : 200, contentType: "application/json", body: calls === 1 ? JSON.stringify({ error: { message: "Review API error" } }) : JSON.stringify({ data: { id: "reviewed" } }) });
  });
  await wizard.getByRole("button", { name: "Create policy version" }).click();
  await expect(wizard.getByRole("alert").filter({ hasText: "Review API error" })).toBeVisible();
  await expect(wizard.getByText("Browser review policy", { exact: false }).first()).toBeVisible();
  await wizard.getByRole("button", { name: "Back" }).click();
  await expect(wizard.getByLabel("Grant units")).toHaveValue("12");
  await wizard.getByRole("button", { name: "Continue" }).click();
  await wizard.getByRole("button", { name: "Create policy version" }).click();
  await expect(page).toHaveURL(/section=leave/);
  expect(calls).toBe(2);
  expect(posted).toMatchObject({ resource: "policy", policyKey: "browser-review-policy", name: "Browser review policy", workweek: [1, 2, 3, 4, 5, 6] });
  await fitsViewport(page);
});

test("custom attributes live in Technology and old direct links redirect", async ({ page }) => {
  await page.goto(`${base}/people/settings?section=attributes`, { waitUntil: "domcontentloaded" });
  await expect(page).toHaveURL(/\/technology\?.*(?:view=settings.*section=attributes|section=attributes.*view=settings)/, { timeout: 60_000 });
  await expect(page.getByRole("heading", { name: "Access settings", exact: true, level:1 })).toBeVisible();
  const workspace = page.locator("#workspace-content:visible");
  await expect(workspace).toHaveCount(1);
  await expect(workspace.getByText("Custom attributes", { exact: true }).last()).toBeVisible();
  await fitsViewport(page);
});

test("pay groups and holiday calendars use existing commands from the People settings panel", async ({ page }) => {
  const posted: { path: string; body: Record<string, unknown> }[] = [];
  await page.route(/\/api\/v1\/.*\/(?:people\/pay-groups|leave\/admin)$/, async route => {
    if (route.request().method() !== "POST") return route.continue();
    posted.push({ path: new URL(route.request().url()).pathname, body: route.request().postDataJSON() });
    await route.fulfill({ status: 200, contentType: "application/json", body: JSON.stringify({ data: { id: "browser-settings-command" } }) });
  });
  await page.goto(`${base}/people?panel=settings&section=pay-groups`);
  const workspace = page.locator("#workspace-content:visible");
  await expect(workspace).toHaveCount(1);
  const panel = workspace.getByTestId("hr-settings-workspace");
  await panel.locator("summary").filter({ hasText: /^Configure pay group/ }).click();
  await page.getByRole("dialog",{name:"Configure pay group",exact:true}).getByLabel("Name", { exact: true }).fill("Browser monthly payroll");
  await page.getByRole("dialog",{name:"Configure pay group",exact:true}).getByRole("button", { name: "Save pay group policy", exact: true }).click();
  await expect.poll(() => posted.length).toBe(1);
  expect(posted[0]).toMatchObject({ path: `/api/v1${base}/people/pay-groups`, body: { name: "Browser monthly payroll", countryCode: "TH", currency: "THB", frequency: "monthly", timezone: "Asia/Bangkok", overtimeMultiplierBps: 15000, prorationMethod: "calendar_days" } });
  await page.getByRole("dialog",{name:"Configure pay group",exact:true}).getByRole("button",{name:"Close dialog",exact:true}).click();
  await chooseTopic(page,/^Holiday calendars/);
  await panel.locator("summary").filter({ hasText: /^Create holiday calendar/ }).click();
  await page.getByRole("dialog",{name:"Create holiday calendar",exact:true}).getByLabel("Calendar name", { exact: true }).fill("Browser holiday calendar");
  await page.getByRole("dialog",{name:"Create holiday calendar",exact:true}).getByRole("button", { name: "Create calendar", exact: true }).click();
  await expect.poll(() => posted.length).toBe(2);
  expect(posted[1]).toMatchObject({ path: `/api/v1${base}/leave/admin`, body: { resource: "calendar", name: "Browser holiday calendar", timezone: "Asia/Bangkok" } });
  await fitsViewport(page);
  await page.getByRole("dialog",{name:"Create holiday calendar",exact:true}).getByRole("button",{name:"Close dialog",exact:true}).click();
  await page.goto(`${base}/people/payroll/settings`);
  await expect(workspace.getByRole("heading", { name: "Pay groups", exact: true })).toHaveCount(0);
});
