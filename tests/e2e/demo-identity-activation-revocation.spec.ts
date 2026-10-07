import {createEngineeringBrowserContext} from './helpers/engineering-browser-evidence';
import { createHmac } from "node:crypto";
import { readFile } from "node:fs/promises";
import { resolve } from "node:path";
import { expect, test, type BrowserContext, type Page } from "@playwright/test";

type DemoUser = {
  email: string;
  role: string;
  roles?: string[];
  label: string;
  userId: string;
};
type Credentials = { password: string; users: DemoUser[] };

const base = "/holdings/simulation";
const origin = (process.env.ERPX_E2E_BASE_URL ?? "http://127.0.0.1:3102");

function authenticatorCode(secret: string, now = Date.now()) {
  const alphabet = "ABCDEFGHIJKLMNOPQRSTUVWXYZ234567";
  const bytes: number[] = [];
  let value = 0;
  let bits = 0;
  for (const character of secret) {
    value = (value << 5) | alphabet.indexOf(character);
    bits += 5;
    if (bits >= 8) {
      bytes.push((value >>> (bits - 8)) & 255);
      bits -= 8;
    }
  }
  const counter = Buffer.alloc(8);
  counter.writeBigUInt64BE(BigInt(Math.floor(now / 30_000)));
  const digest = createHmac("sha1", Buffer.from(bytes)).update(counter).digest();
  const offset = digest[19] & 15;
  return String((digest.readUInt32BE(offset) & 0x7fffffff) % 1_000_000).padStart(6, "0");
}

async function selectRole(context: BrowserContext, credentials: Credentials, role: string, preferredEmail?: string) {
  const user = credentials.users.find((item) => item.email === preferredEmail) ?? credentials.users.find((item) => item.roles?.includes(role) || item.role === role);
  expect(user, `${role} demo account`).toBeTruthy();
  const login = await context.request.post("/api/auth/sign-in/email", {
    headers: { Origin: origin },
    data: { email: user!.email, password: credentials.password },
  });
  expect(login.ok(), await login.text()).toBe(true);
  const endpoint = `${base}/assignments/active`;
  const active = (await (await context.request.get(`/api/v1${endpoint}`)).json()).data;
  const assignment = active.options.find((item: { role: string }) => item.role === role);
  expect(assignment, `${role} active assignment`).toBeTruthy();
  const selected = await context.request.post(`/api/v1${endpoint}`, {
    headers: { Origin: origin },
    data: { assignmentId: assignment.id, revision: active.revision },
  });
  expect(selected.ok(), await selected.text()).toBe(true);
  return user!;
}

async function signInRole(context: BrowserContext, page: Page, credentials: Credentials, role: string, path: string, preferredEmail?: string) {
  const user = await selectRole(context, credentials, role, preferredEmail);
  await page.goto(`${path}${path.includes("?") ? "&" : "?"}refresh=${Date.now()}`, { waitUntil: "commit" });
  await expect(page.locator("[data-work-stale]")).toHaveAttribute("data-work-stale", "false");
  return user;
}

async function verifyMfa(page: Page) {
  await page.goto("/security");
  const current = await page.request.get("/api/v1/account/iam");
  if (current.ok() && Boolean((await current.json()).data?.mfaEnabled)) return;
  const enroll = page.getByRole("button", { name: "Set up authenticator", exact: true });
  if (await enroll.count()) {
    await enroll.click();
    const security = page.locator("#security-content:visible");
    await expect(security).toHaveCount(1);
    const setup = security.locator("code");
    await expect(setup).toHaveCount(1);
    await expect(setup).toBeVisible();
    const secret = await setup.innerText();
    await page.getByLabel("Six-digit code", { exact: true }).fill(authenticatorCode(secret));
    await page.getByRole("button", { name: "Verify code", exact: true }).click();
    await expect(page.getByRole("status")).toContainText("Authenticator verified");
  }
}

test("Demo invitation activates a separate account and revocation removes access immediately", async ({ browser }, testInfo) => {
  const run = process.env.ERPX_DEMO_RUN;
  test.skip(!run, "Use an exact isolated Demo run; this test creates one disposable employee.");
  test.setTimeout(180_000);
  const credentials = JSON.parse(await readFile(resolve(`.runtime/browser/${run}/credentials.json`), "utf8")) as Credentials;
  const ownerContext = await createEngineeringBrowserContext(browser,testInfo);
  const reviewerContext = await createEngineeringBrowserContext(browser,testInfo);
  const inviteeContext = await createEngineeringBrowserContext(browser,testInfo);
  [ownerContext, reviewerContext, inviteeContext].forEach((context) => context.setDefaultTimeout(15_000));
  const ownerPage = await ownerContext.newPage();
  const reviewerPage = await reviewerContext.newPage();
  const inviteePage = await inviteeContext.newPage();
  const suffix = Date.now();
  const employeeName = `Demo activation employee ${suffix}`;
  const employeeEmail = `activation-${suffix}@simulation.test`;
  const employeePassword = `Activation-${suffix}-Password!`;
  let personId = "";
  let activationLink = "";
  try {
    await signInRole(ownerContext, ownerPage, credentials, "holding_owner", `${base}/people`);
    await verifyMfa(ownerPage);
    await ownerPage.goto(`${base}/people?refresh=${Date.now()}`);
    await expect(ownerPage.locator("[data-work-stale]")).toHaveAttribute("data-work-stale", "false");
    const contextNode = ownerPage.locator("[data-work-revision]").first();
    const workHeaders = {
      "x-erpx-context-revision": (await contextNode.getAttribute("data-work-revision"))!,
      "x-erpx-context-company": (await contextNode.getAttribute("data-work-company"))!,
      "x-erpx-context-assignment": (await contextNode.getAttribute("data-work-assignment"))!,
    };
    const organizationResponse = await ownerPage.request.get(`/api/v1${base}/people/organization`);
    expect(organizationResponse.ok(), await organizationResponse.text()).toBe(true);
    const organization = (await organizationResponse.json()).data as {
      nodes: { id: string; kind: string }[];
      positions: { id: string; active: number; is_ceo: number; level_id: string; level_number: number; branch_id: string | null; department_id: string | null }[];
      people: { person_id: string; status: string; person_status: string; level_number: number | null }[];
    };
    const branch = organization.nodes.find((node) => node.kind === "branch");
    const position = organization.positions.find((item) => item.active === 1 && item.is_ceo === 0 && item.level_number > 1);
    const manager = organization.people.find((person) => person.status === "active" && person.person_status === "active" && person.level_number !== null && position && person.level_number < position.level_number);
    expect(branch && position && manager, "Demo organization must provide a valid onboarding placement").toBeTruthy();
    const createPerson = await ownerPage.request.post(`/api/v1${base}/people/directory`, {
      headers: { ...workHeaders, "Idempotency-Key": `demo-identity-person-${suffix}` },
      data: {
        legalName: employeeName,
        preferredName: employeeName,
        email: employeeEmail,
        phone: "",
        countryCode: "TH",
        branchId: branch!.id,
        departmentId: position!.department_id,
        jobTitle: "Demo activation operator",
        workerType: "employee",
        employmentType: "salaried",
        startDate: "2026-09-23",
        currency: "THB",
        managerPersonId: manager!.person_id,
        levelId: position!.level_id,
        positionId: position!.id,
        reason: "Identity activation acceptance scenario",
      },
    });
    expect(createPerson.ok(), await createPerson.text()).toBe(true);
    personId = String((await createPerson.json()).data.id);
    const activatePerson = await ownerPage.request.post(`/api/v1${base}/people/directory/${personId}/status`, {
      headers: { ...workHeaders, "Idempotency-Key": `demo-identity-activate-${suffix}` },
      data: { status: "active", expectedVersion: 1 },
    });
    expect(activatePerson.ok(), await activatePerson.text()).toBe(true);
    await ownerPage.goto(`${base}/people/${personId}?tab=access`);

    const workspace = ownerPage.locator("#workspace-content:visible");
    await expect(workspace).toHaveCount(1);
    const access = workspace.locator('[data-feature-widget="person-access"]');
    await expect(access).toHaveCount(1);
    await expect(access).toBeVisible();
    await access.locator("summary").filter({ hasText: /^Invite to ERPX$/ }).click();
    const inviteDialog = ownerPage.getByRole("dialog", { name: "Invite to ERPX", exact: true });
    await inviteDialog.locator('select[name="role"]').selectOption("employee");
    await inviteDialog.getByLabel("Access expires", { exact: true }).fill("2030-01-01");
    const invitationResponse = ownerPage.waitForResponse(response => response.request().method() === "POST" && response.url().endsWith(`/people/directory/${personId}/invite`));
    await inviteDialog.getByRole("button", { name: "Create activation link", exact: true }).click();
    const invited = await invitationResponse;
    expect(invited.status(), await invited.text()).toBe(200);
    const createdInvitation = (await invited.json()).data as { id: string; reviewId: string };
    expect(createdInvitation.id).toMatch(/^invite-/);
    expect(createdInvitation.reviewId).toBeTruthy();
    await inviteDialog.getByRole("button", { name: "Close dialog", exact: true }).click();
    await expect(inviteDialog).not.toBeVisible();
    const link = access.getByLabel("Activation link (expires in 48 hours)", { exact: true });
    await expect(link).toHaveValue(/\/activate\?token=/);
    activationLink = await link.inputValue();

    const reviewer = credentials.users.find((user) => user.roles?.includes("holding_owner") && user.role !== "holding_owner");
    expect(reviewer, "an independent Demo holding owner reviewer").toBeTruthy();
    await signInRole(reviewerContext, reviewerPage, credentials, "holding_owner", `${base}/people/settings?section=reviews`, reviewer!.email);
    await expect(reviewerPage.getByRole("button", { name: /Account menu for Quality release manager/ })).toBeVisible();
    await verifyMfa(reviewerPage);
    await reviewerPage.goto(`${base}/people/settings?section=reviews&refresh=${Date.now()}`);
    await expect(reviewerPage.locator("[data-work-stale]")).toHaveAttribute("data-work-stale", "false");
    const reviewerWorkspace = reviewerPage.locator("#workspace-content:visible");
    await expect(reviewerWorkspace).toHaveCount(1);
    const invitation = reviewerWorkspace.locator("article").filter({ hasText: "Authorize an employee invitation and initial grant." }).filter({ has: reviewerPage.locator("pre").filter({ hasText: createdInvitation.id }) });
    await expect(invitation).toHaveCount(1);
    await expect(invitation).toBeVisible();
    await invitation.locator("summary").filter({ hasText: /^Review exact change and impact$/ }).click();
    expect(JSON.parse(await invitation.locator("pre").innerText()).invitationId).toBe(createdInvitation.id);
    await invitation.locator('select[name="decision"]').selectOption("approve");
    await invitation.locator('textarea[name="reason"]').fill("Reviewed the employee identity and least-privilege scope.");
    await invitation.getByRole("button", { name: "Record decision", exact: true }).click();
    await expect(invitation).toContainText("approved");

    await inviteePage.goto(activationLink);
    await expect(inviteePage.getByRole("heading", { name: "Activate your ERPX account", exact: true })).toBeVisible();
    await inviteePage.getByLabel(/^Password(?:\s*\*)?$/).fill(employeePassword);
    await inviteePage.getByLabel("Confirm password", { exact: true }).fill(employeePassword);
    await inviteePage.getByRole("button", { name: "Activate account", exact: true }).click();
    await expect(inviteePage).toHaveURL(/\/login$/);
    await inviteePage.getByRole("textbox", { name: "Work email", exact: true }).fill(employeeEmail);
    await inviteePage.getByLabel(/^Password(?:\s*\*)?$/).fill(employeePassword);
    await inviteePage.getByRole("button", { name: "Sign in", exact: true }).click();
    await expect(inviteePage).toHaveURL(new RegExp(`${base}/(?:branches/[^/]+/)?home$|/work-role\\?`));
    if (inviteePage.url().includes("/work-role")) {
      const picker = inviteePage.locator("#work-role-content:visible #role-picker");
      await expect(picker).toHaveCount(1);
      const trigger = picker.getByRole("button", { name: /Role for actions & approvals/ });
      await expect(trigger).toHaveCount(1);
      await expect(trigger).toBeEnabled();
      await trigger.click();
      const search = picker.getByRole("combobox", { name: "Search roles", exact: true });
      await expect(search).toHaveCount(1);
      await search.fill("employee");
      const employee = picker.getByRole("listbox", { name: "Available roles", exact: true }).getByRole("option");
      await expect(employee).toHaveCount(1);
      await employee.click();
    }
    await expect(inviteePage).toHaveURL(new RegExp(`${base}/(?:branches/[^/]+/)?home$`));
    await expect(inviteePage.getByRole("main")).toBeVisible();
    const employeeHome = new URL(inviteePage.url());
    const employeeWorkspace = employeeHome.pathname.replace(/\/home$/, "");
    const selfReadUrl = new URL(`/api/v1${employeeWorkspace}/people/people/${personId}`, origin).toString();
    const authorizedSelf = await inviteePage.request.get(selfReadUrl);
    expect(authorizedSelf.status(), "the activated employee can read their own record in the selected branch").toBe(200);
    expect((await authorizedSelf.json()).data.person.id).toBe(personId);

    await ownerPage.goto(`${base}/people/${personId}?tab=access`);
    await access.getByRole("button", { name: "Revoke role", exact: true }).click();
    await expect(access.getByRole("button", { name: "Revoke role", exact: true })).toHaveCount(0);
    const revokedSelf = await inviteePage.request.get(selfReadUrl);
    expect(revokedSelf.status(), "the same employee read is denied immediately after revocation").toBe(409);
    expect((await revokedSelf.json()).error.code).toBe("WORK_ROLE_UNAVAILABLE");
    const revokedNavigation = await inviteePage.goto(employeeHome.toString());
    expect(revokedNavigation?.status(), "revoked employee cannot open the protected home").toBe(403);
    await expect(inviteePage).toHaveURL(employeeHome.toString());
    const denied = inviteePage.getByRole("region", { name: "Your role cannot open this view", exact: true });
    await expect(denied).toHaveCount(1);
    await expect(denied.getByRole("heading", { name: "Your role cannot open this view", exact: true })).toBeVisible();
    await expect(inviteePage.locator("#workspace-content")).toHaveCount(0);
    await testInfo.attach("identity-activation-revocation-result", {
      contentType: "application/json",
      body: JSON.stringify({ run, employeeEmail, personId, ownerRole: "holding_owner", reviewerRole: "holding_owner", reviewerEmail: reviewer!.email, activated: true, revokedImmediately: true }, null, 2),
    });
  } finally {
    await Promise.allSettled([ownerContext.close(), reviewerContext.close(), inviteeContext.close()]);
  }
});
