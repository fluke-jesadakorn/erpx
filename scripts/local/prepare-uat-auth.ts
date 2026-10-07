import { chromium, type Page } from "@playwright/test";
import { mkdir, readFile, chmod } from "node:fs/promises";
import { resolve } from "node:path";
import { parseUatAuthProfiles, type UatProfileId } from "../testing/uat-auth";

type User = { email: string; role: string; roles?: string[]; scope?: string; scopeNode?: string; scopeParentNode?: string };
type Credentials = { password: string; users: User[] };
type Profile = {
  id: UatProfileId;
  email: string;
  role: string;
  scope?: string;
  app?: "pms" | "pos";
  holding: "simulation" | "riverside-lantern";
};

const root = resolve(import.meta.dir, "../..");
const run = process.env.ERPX_BROWSER_SIMULATION_RUN ?? "";
const baseURL = process.env.ERPX_E2E_BASE_URL ?? "http://127.0.0.1:3104";
if (!/^[a-f0-9]{12}$/.test(run)) throw new Error("SET_EXACT_BROWSER_SIMULATION_RUN");
const base = new URL(baseURL);
if (base.hostname !== "127.0.0.1" || base.port !== "3104") throw new Error("UAT_AUTH_REQUIRES_LOOPBACK_3104");

const directory = resolve(root, ".runtime/browser", run);
const state = JSON.parse(await readFile(resolve(directory, "state.json"), "utf8")) as { run: string; origin: string; port?: number };
if (state.run !== run || (state.port ?? 3102) !== 3104 || state.origin !== base.origin)
  throw new Error("UAT_SIMULATION_STATE_MISMATCH");
const credentials = JSON.parse(await readFile(resolve(directory, "credentials.json"), "utf8")) as Credentials;
const authDirectory = resolve(process.env.ERPX_UAT_AUTH_DIR ?? resolve(root, ".scratch/uat", run));
if (![resolve(root, ".scratch/uat", run), resolve(directory, "uat-auth")].includes(authDirectory))
  throw new Error("UAT_AUTH_DIRECTORY_NOT_OWNED_BY_RUN");
await mkdir(authDirectory, { recursive: true, mode: 0o700 });
await chmod(authDirectory, 0o700);

const profiles: Profile[] = [
  { id: "owner", email: "source@simulation.test", role: "holding_owner", holding: "simulation" },
  { id: "owner-branch", email: "source@simulation.test", role: "holding_owner", holding: "simulation" },
  { id: "owner-department", email: "source@simulation.test", role: "holding_owner", holding: "simulation" },
  { id: "employee", email: "support-agent@simulation.test", role: "employee", holding: "simulation" },
  { id: "finance", email: "finance@simulation.test", role: "finance_approver", holding: "simulation" },
  { id: "people", email: "receiver@simulation.test", role: "people_manager", holding: "simulation" },
  { id: "purchasing", email: "buyer@simulation.test", role: "purchasing_operator", holding: "simulation" },
  { id: "sales", email: "support-agent@simulation.test", role: "sales_operator", holding: "simulation" },
  { id: "support", email: "support-agent@simulation.test", role: "support_agent", holding: "simulation" },
  { id: "branch", email: "branch@simulation.test", role: "employee", scope: "branch", holding: "simulation" },
  { id: "department", email: "department@simulation.test", role: "employee", scope: "department", holding: "simulation" },
  { id: "hotel-owner", email: "admin@riverside-lantern.example", role: "holding_owner", holding: "riverside-lantern" },
  { id: "pms-holding", email: "frontdesk@riverside-lantern.example", role: "pms_frontdesk", holding: "riverside-lantern" },
  { id: "pos-holding", email: "cashier@riverside-lantern.example", role: "pos_cashier", holding: "riverside-lantern" },
  { id: "pms", email: "frontdesk@riverside-lantern.example", role: "pms_frontdesk", app: "pms", holding: "riverside-lantern" },
  { id: "pos", email: "cashier@riverside-lantern.example", role: "pos_cashier", app: "pos", holding: "riverside-lantern" },
];
const selectedProfileIds = parseUatAuthProfiles(process.env.ERPX_UAT_AUTH_PROFILES);
const selectedProfiles = profiles.filter(({ id }) => !selectedProfileIds || selectedProfileIds.includes(id));

function normalizedRole(value: string) {
  return value.toLowerCase().replaceAll("_", " ");
}

async function ensureWorkspacePath(page: Page, homePath: string) {
  if (new URL(page.url()).pathname === homePath) return;
  try {
    await page.goto(homePath, { waitUntil: "domcontentloaded" });
  } catch (error) {
    await page.waitForURL((url) => url.pathname === homePath, { timeout: 15_000 }).catch(() => {
      throw error;
    });
  }
}

async function selectRole(page: Page, profile: Profile, homePath: string) {
  const desired = normalizedRole(profile.role);
  const trigger = page.getByRole("button", { name: /^Role for actions & approvals/ });
  if (await trigger.isVisible()) {
    const current = (await trigger.innerText()).toLowerCase();
    if (current.includes(desired) && (!profile.scope || current.includes(profile.scope))) {
      await ensureWorkspacePath(page, homePath);
      return;
    }
    await trigger.click();
  }
  const picker = page.getByRole("combobox", { name: /^(?:Search roles|Role for actions & approvals)$/ });
  await picker.waitFor({ state: "visible", timeout: 15_000 });
  const current = (await picker.inputValue()).toLowerCase();
  if (current.includes(desired) && (!profile.scope || current.includes(profile.scope))) {
    await ensureWorkspacePath(page, homePath);
    return;
  }
  await picker.fill(profile.scope ?? desired);
  const roleOption = page.getByRole("option").filter({ hasText: new RegExp(profile.scope ?? desired, "i") }).first();
  await roleOption.waitFor({ state: "visible", timeout: 10_000 });
  const selection = page.waitForResponse((response) => {
    const url = new URL(response.url());
    return response.request().method() === "POST" && url.pathname.endsWith("/assignments/active");
  }, { timeout: 15_000 });
  await roleOption.click();
  const response = await selection;
  if (!response.ok()) throw new Error(`work role selection failed with HTTP ${response.status()}`);
  // The application navigates after consuming the API response. Reload the
  // intended scoped home after the persisted selection is confirmed so auth
  // setup cannot save a session before that redirect finishes.
  await ensureWorkspacePath(page, homePath);
}

async function configureProfile(page: Page, profile: Profile, homePath: string) {
  if (profile.app) return;
  if (page.url().includes("/work-role")) {
    await selectRole(page, profile, homePath);
    return;
  }
  const accountMenu = page.getByRole("button", { name: /Account menu for/ });
  await accountMenu.waitFor({ state: "visible", timeout: 15_000 });
  await accountMenu.click();
  await selectRole(page, profile, homePath);
}

async function waitForSignedInLanding(page: Page) {
  await page.waitForFunction(() => {
    if (location.pathname === "/work-role") return Boolean(document.querySelector('input[role="combobox"], button.work-role-trigger'));
    return location.pathname.startsWith("/holdings/") && Boolean(document.querySelector('button[aria-label^="Account menu for"]'));
  }, null, { timeout: 30_000 });
}

const browser = await chromium.launch({
  args: ["--host-resolver-rules=MAP dev-pms.localhost 127.0.0.1,MAP dev-pos.localhost 127.0.0.1"],
});
try {
  for (const profile of selectedProfiles) {
    const known = credentials.users.some((user) => user.email === profile.email && (user.role === profile.role || user.roles?.includes(profile.role)) && (!profile.scope || user.scope === profile.scope));
    if (!known) throw new Error(`UAT_PROFILE_MISSING:${profile.id}`);
    const profileBase = profile.app ? `http://dev-${profile.app}.localhost:3104` : base.origin;
    const actor = credentials.users.find((user) => user.email === profile.email && (user.role === profile.role || user.roles?.includes(profile.role)) && (!profile.scope || user.scope === profile.scope));
    if (!actor) throw new Error(`UAT_PROFILE_MISSING:${profile.id}`);
    const homePath = profile.scope === "branch"
      ? `/holdings/${profile.holding}/branches/${actor.scopeNode}/home`
      : profile.scope === "department"
        ? `/holdings/${profile.holding}/branches/${actor.scopeParentNode}/departments/${actor.scopeNode}/home`
        : `/holdings/${profile.holding}/home`;
    const context = await browser.newContext({ baseURL: profileBase });
    const diagnostics: string[] = [];
    let diagnosticPage: Page | undefined;
    try {
      const page = await context.newPage();
      diagnosticPage = page;
      const withoutQuery = (value: string) => value.replace(/https?:\/\/[^\s)]+/g, (raw) => {
        try { const url = new URL(raw); return `${url.origin}${url.pathname}`; } catch { return "<url>"; }
      });
      page.on("pageerror", (error) => diagnostics.push(`pageerror:${withoutQuery(error.message)}`));
      page.on("console", (message) => { if (message.type() === "error" && !message.text().includes("/_next/hmr")) diagnostics.push(`console:${withoutQuery(message.text())}`); });
      page.on("requestfailed", (request) => diagnostics.push(`requestfailed:${request.method()} ${new URL(request.url()).pathname}:${request.failure()?.errorText ?? "unknown"}`));
      page.on("response", (response) => {
        const pathname = new URL(response.url()).pathname;
        if (pathname.startsWith("/api/auth/")) diagnostics.push(`auth-response:${response.status()} ${pathname}`);
      });
      const startPath = profile.app
        ? "/login"
        : `/login?next=${encodeURIComponent(homePath)}`;
      await page.goto(startPath, { waitUntil: "domcontentloaded" });
      // Front hostnames are resolved locally by Chromium. Wait for Next's client
      // bundle to hydrate before submitting the client-handled login form.
      await page.waitForLoadState("networkidle");
      await page.locator('input[type="email"]').fill(profile.email);
      await page.locator('input[type="password"]').fill(credentials.password);
      await Promise.all([
        page.waitForFunction(
          () => location.pathname !== "/login" || Boolean(document.querySelector('[role="alert"]')),
          null,
          { timeout: 12_000 },
        ),
        page.locator('button[type="submit"]').click(),
      ]);
      if (new URL(page.url()).pathname === "/login") throw new Error("sign-in form rejected the simulated account");
      if (!profile.app) await waitForSignedInLanding(page);
      await configureProfile(page, profile, homePath);
      if (profile.app) {
        const companyId = String((state as { fixture?: { hospitality?: { companyId?: string } } }).fixture?.hospitality?.companyId ?? "");
        if (!companyId) throw new Error("UAT_HOSPITALITY_COMPANY_REQUIRED");
        await page.goto(`/${profile.holding}/${companyId}/holding/`, { waitUntil: "domcontentloaded" });
      }
      if (!profile.app && new URL(page.url()).pathname !== homePath) {
        throw new Error("simulated account did not reach its assigned workspace role");
      }
      await page.getByRole("main").first().waitFor({ state: "visible", timeout: 30_000 });
      await context.storageState({ path: resolve(authDirectory, `${profile.id}.json`) });
      await chmod(resolve(authDirectory, `${profile.id}.json`), 0o600);
    } catch (error) {
      const path = (() => { try { return new URL(diagnosticPage?.url() ?? "").pathname; } catch { return "<unknown>"; } })();
      const alerts = await diagnosticPage?.locator('[role="alert"]').allTextContents().catch(() => [] as string[]) ?? [];
      const formState = await diagnosticPage?.locator("form").first().evaluate((form) => ({
        action: (form as HTMLFormElement).action.replace(/\?.*$/, ""),
        method: (form as HTMLFormElement).method,
        submitText: form.querySelector('button[type="submit"]')?.textContent?.trim(),
        reactProps: Object.keys(form).some((key) => key.startsWith("__reactProps")),
      })).catch(() => null);
      const pageText = (await diagnosticPage?.locator("body").innerText().catch(() => "") ?? "").slice(0, 600);
      console.error(JSON.stringify({ profile: profile.id, path, alerts, formState, pageText, diagnostics: diagnostics.slice(-8) }));
      const message = error instanceof Error ? error.message.replace(/https?:\/\/[^\s)]+/g, "<url>") : "unknown";
      throw new Error(`UAT_AUTH_PROFILE_FAILED:${profile.id}:${message}`);
    } finally {
      await context.close();
    }
  }
} finally {
  await browser.close();
}
console.log(JSON.stringify({ authDirectory, profiles: selectedProfiles.map(({ id, role, scope, app }) => ({ id, role, scope, app })) }));
