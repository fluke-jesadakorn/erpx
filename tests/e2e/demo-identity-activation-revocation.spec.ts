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
const origin = "http://127.0.0.1:3102";

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
    const setup = page.locator("code").first();
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
  const ownerContext = await browser.newContext(testInfo.project.use);
  const reviewerContext = await browser.newContext(testInfo.project.use);
  const inviteeContext = await browser.newContext(testInfo.project.use);
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

    await ownerPage.goto(`${base}/people/${personId}?tab=access`);
    await ownerPage.getByText("Invite to ERPX", { exact: true }).click();
    await ownerPage.locator('select[name="role"]').selectOption("employee");
    await ownerPage.getByLabel("Access expires", { exact: true }).fill("2030-01-01");
    await ownerPage.getByRole("button", { name: "Create activation link", exact: true }).click();
    const link = ownerPage.getByLabel("Activation link (expires in 48 hours)", { exact: true });
    await expect(link).toHaveValue(/\/activate\?token=/);
    activationLink = await link.inputValue();

    const reviewer = credentials.users.find((user) => user.roles?.includes("holding_owner") && user.role !== "holding_owner");
    expect(reviewer, "an independent Demo holding owner reviewer").toBeTruthy();
    await signInRole(reviewerContext, reviewerPage, credentials, "holding_owner", `${base}/people/settings?section=reviews`, reviewer!.email);
    await expect(reviewerPage.getByRole("button", { name: /Account menu for Quality release manager/ })).toBeVisible();
    await verifyMfa(reviewerPage);
    await reviewerPage.goto(`${base}/people/settings?section=reviews&refresh=${Date.now()}`);
    await expect(reviewerPage.locator("[data-work-stale]")).toHaveAttribute("data-work-stale", "false");
    await reviewerPage.goto(`${base}/people/settings?section=reviews`);
    const invitation = reviewerPage.locator("article").filter({ hasText: "Authorize an employee invitation and initial grant." }).first();
    await expect(invitation).toBeVisible();
    await invitation.locator('select[name="decision"]').selectOption("approve");
    await invitation.locator('textarea[name="reason"]').fill("Reviewed the employee identity and least-privilege scope.");
    await invitation.getByRole("button", { name: "Record decision", exact: true }).click();
    await expect(invitation).toContainText("approved");

    await inviteePage.goto(activationLink);
    await expect(inviteePage.getByRole("heading", { name: "Activate your ERPX account", exact: true })).toBeVisible();
    await inviteePage.getByLabel("Password", { exact: true }).fill(employeePassword);
    await inviteePage.getByLabel("Confirm password", { exact: true }).fill(employeePassword);
    await inviteePage.getByRole("button", { name: "Activate account", exact: true }).click();
    await expect(inviteePage).toHaveURL(/\/login$/);
    await inviteePage.getByLabel("Work email", { exact: true }).fill(employeeEmail);
    await inviteePage.getByLabel("Password", { exact: true }).fill(employeePassword);
    await inviteePage.getByRole("button", { name: "Sign in", exact: true }).click();
    await expect(inviteePage).toHaveURL(new RegExp(`${base}/(?:branches/[^/]+/)?home$|/work-role\\?`));
    if (inviteePage.url().includes("/work-role")) {
      await inviteePage.getByRole("combobox", { name: "Role for actions & approvals" }).fill("employee");
      await inviteePage.getByRole("listbox").getByRole("option").click();
    }
    await expect(inviteePage).toHaveURL(new RegExp(`${base}/(?:branches/[^/]+/)?home$`));
    await expect(inviteePage.getByRole("main")).toBeVisible();

    await ownerPage.goto(`${base}/people/${personId}?tab=access`);
    await ownerPage.getByRole("button", { name: "Revoke role", exact: true }).click();
    await expect(ownerPage.getByRole("button", { name: "Revoke role", exact: true })).toHaveCount(0);
    await inviteePage.goto(`${base}/home`);
    await expect(inviteePage).toHaveURL(/\/access\?reason=permission|\/login|\/work-role\?/);
    if (inviteePage.url().includes("/work-role")) {
      await expect(inviteePage.getByText("No active roles are available.", { exact: false })).toBeVisible();
    }
    await testInfo.attach("identity-activation-revocation-result", {
      contentType: "application/json",
      body: JSON.stringify({ run, employeeEmail, personId, ownerRole: "holding_owner", reviewerRole: "holding_owner", reviewerEmail: reviewer!.email, activated: true, revokedImmediately: true }, null, 2),
    });
  } finally {
    await Promise.allSettled([ownerContext.close(), reviewerContext.close(), inviteeContext.close()]);
  }
});
