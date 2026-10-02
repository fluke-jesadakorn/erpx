import { createHmac } from "node:crypto";
import { readFile } from "node:fs/promises";
import { resolve } from "node:path";
import { expect, test, type BrowserContext, type Page } from "@playwright/test";

type SimUser = { email: string; role: string; userId: string; label: string };
type Credentials = { password: string; users: SimUser[] };
type DemoState = { fixture: { hospitality: { companyId: string } } };
type Organization = {
  nodes: { id: string; kind: string; display_name: string }[];
  positions: { id: string; title: string; active: number; level_id: string; level_number: number; branch_id: string | null; department_id: string | null }[];
  people: { person_id: string; legal_name: string; status: string; level_number: number | null }[];
};

const rootOrigin = process.env.ERPX_E2E_BASE_URL ?? "http://127.0.0.1:3100";
const holding = "/holdings/riverside-lantern";
const assignmentsPath = "/api/v1/holdings/riverside-lantern/assignments/active";

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

async function signInHolding(context: BrowserContext, page: Page, credentials: Credentials, user: SimUser, destination: string) {
  const login = await context.request.post(new URL("/api/auth/sign-in/email", rootOrigin).toString(), {
    headers: { Origin: rootOrigin },
    data: { email: user.email, password: credentials.password },
  });
  expect(login.ok(), "H002 account sign-in").toBe(true);
  const activeResponse = await context.request.get(new URL(assignmentsPath, rootOrigin).toString());
  expect(activeResponse.ok(), "H002 active assignments").toBe(true);
  const active = (await activeResponse.json()).data;
  const assignment = active.options.find((item: { role: string }) => item.role === "holding_owner");
  expect(assignment, `${user.label} holding-owner assignment`).toBeTruthy();
  const selected = await context.request.post(new URL(assignmentsPath, rootOrigin).toString(), {
    headers: { Origin: rootOrigin },
    data: { assignmentId: assignment.id, revision: active.revision },
  });
  expect(selected.ok(), await selected.text()).toBe(true);
  await page.goto(new URL(destination, rootOrigin).toString(), { waitUntil: "domcontentloaded" });
  await expect(page.locator("[data-work-stale]")).toHaveAttribute("data-work-stale", "false");
}

async function ensureMfa(page: Page) {
  await page.goto(new URL("/security", rootOrigin).toString(), { waitUntil: "domcontentloaded" });
  const current = await page.request.get(new URL("/api/v1/account/iam", rootOrigin).toString());
  if (current.ok() && Boolean((await current.json()).data?.mfaEnabled)) return;
  const enroll = page.getByRole("button", { name: "Set up authenticator", exact: true });
  if (await enroll.count()) {
    await enroll.click();
    const secret = await page.locator("code").first().innerText();
    await page.getByLabel("Six-digit code", { exact: true }).fill(authenticatorCode(secret));
    await page.getByRole("button", { name: "Verify code", exact: true }).click();
    await expect(page.getByRole("status")).toContainText("Authenticator verified");
  }
}

test("H002 HR creates and edits an employee, approves access independently, then revokes it", async ({ browser }, testInfo) => {
  const run = process.env.ERPX_DEMO_RUN;
  expect(run, "The Thai Hotel mock runner must supply an exact prepared simulation run.").toMatch(/^[a-f0-9]{12}$/);
  test.setTimeout(240_000);
  const directory = resolve(`.runtime/browser/${run}`);
  const credentials = JSON.parse(await readFile(resolve(directory, "credentials.json"), "utf8")) as Credentials;
  const state = JSON.parse(await readFile(resolve(directory, "state.json"), "utf8")) as DemoState;
  const owner = credentials.users.find((user) => user.email === "admin@riverside-lantern.example");
  const reviewer = credentials.users.find((user) => user.email === "iam-reviewer@riverside-lantern.example");
  expect(owner && reviewer, "H002 provides separate HR and IAM review identities").toBeTruthy();
  const ownerContext = await browser.newContext(testInfo.project.use);
  const reviewerContext = await browser.newContext(testInfo.project.use);
  const employeeContext = await browser.newContext(testInfo.project.use);
  [ownerContext, reviewerContext, employeeContext].forEach((context) => context.setDefaultTimeout(15_000));
  const ownerPage = await ownerContext.newPage();
  const reviewerPage = await reviewerContext.newPage();
  const employeePage = await employeeContext.newPage();
  const stamp = `${run}-${testInfo.project.name}-${Date.now()}`;
  const employeeName = `H002 lifecycle ${stamp}`;
  const preferredName = `Riverside team ${stamp}`;
  const employeeEmail = `hr-${stamp}@simulation.test`;
  const employeePassword = `Hotel-HR-${Date.now()}-Pass!`;
  let personId = "";
  try {
    await signInHolding(ownerContext, ownerPage, credentials, owner!, `${holding}/people?new=person`);
    await ensureMfa(ownerPage);
    await ownerPage.goto(new URL(`${holding}/people?new=person`, rootOrigin).toString(), { waitUntil: "domcontentloaded" });
    const organizationResponse = await ownerPage.request.get(new URL(`/api/v1${holding}/people/organization`, rootOrigin).toString());
    expect(organizationResponse.ok(), await organizationResponse.text()).toBe(true);
    const organization = (await organizationResponse.json()).data as Organization;
    const branch = organization.nodes.find((node) => node.kind === "branch");
    const position = organization.positions.find((item) => item.active === 1 && item.title === "Riverside hospitality employee");
    const manager = organization.people.find((person) => person.status === "active" && position && person.level_number !== null && person.level_number < position.level_number);
    expect(branch && position && manager, "H002 has a branch, employee position, and eligible reporting manager").toBeTruthy();

    await ownerPage.getByLabel("Legal name", { exact: true }).fill(employeeName);
    await ownerPage.getByLabel("Preferred name", { exact: true }).fill(employeeName);
    await ownerPage.getByLabel("Work email", { exact: true }).fill(employeeEmail);
    await ownerPage.getByLabel("Country code", { exact: true }).fill("TH");
    await ownerPage.locator('select[name="branchId"]').selectOption(branch!.id);
    await ownerPage.locator("label").filter({ hasText: /^Position/ }).locator("select").selectOption(position!.id);
    await ownerPage.locator('select[name="managerPersonId"]').selectOption(manager!.person_id);
    await ownerPage.getByLabel("Job title", { exact: true }).fill("Hotel acceptance operator");
    await ownerPage.locator('select[name="employmentType"]').selectOption("shift");
    await ownerPage.locator('select[name="currency"]').selectOption("THB");
    const createResponse = ownerPage.waitForResponse((response) => response.request().method() === "POST" && response.url().endsWith("/people/directory"));
    await ownerPage.getByRole("button", { name: "Save employee", exact: true }).click();
    const created = await createResponse;
    expect(created.ok(), await created.text()).toBe(true);
    await expect(ownerPage).toHaveURL(new RegExp(`${holding}/people/[^/?]+$`));
    personId = new URL(ownerPage.url()).pathname.split("/").pop()!;
    const activateResponse = ownerPage.waitForResponse((response) => response.request().method() === "POST" && response.url().endsWith(`/people/directory/${personId}/status`));
    await ownerPage.getByRole("button", { name: "Activate employee", exact: true }).click();
    expect((await activateResponse).ok()).toBe(true);
    await ownerPage.reload({ waitUntil: "domcontentloaded" });
    await expect(ownerPage.getByRole("heading", { name: employeeName, exact: true })).toBeVisible();

    await ownerPage.getByRole("button", { name: "Edit employee", exact: true }).click();
    const employeeForm = ownerPage.locator("form").filter({ has: ownerPage.getByLabel("Preferred name", { exact: true }) });
    await employeeForm.getByLabel("Preferred name", { exact: true }).fill(preferredName);
    await employeeForm.getByLabel("Change reason", { exact: true }).fill("Update preferred name and confirm reporting placement for H002 mock acceptance.");
    const editResponse = ownerPage.waitForResponse((response) => response.request().method() === "POST" && response.url().endsWith(`/people/directory/${personId}`));
    await employeeForm.getByRole("button", { name: "Save employee", exact: true }).click();
    expect((await editResponse).ok()).toBe(true);
    const personResponse = await ownerPage.request.get(new URL(`/api/v1${holding}/people`, rootOrigin).toString());
    expect(personResponse.ok(), await personResponse.text()).toBe(true);
    const persistedPerson = (await personResponse.json()).data.find((person: { id: string }) => person.id === personId);
    expect(persistedPerson).toMatchObject({ preferred_name: preferredName, engagement_status: "active", manager_person_id: manager!.person_id, position_id: position!.id, employment_type: "shift" });

    await ownerPage.goto(new URL(`${holding}/people/${personId}?tab=access`, rootOrigin).toString(), { waitUntil: "domcontentloaded" });
    await ownerPage.getByText("Invite to ERPX", { exact: true }).click();
    await ownerPage.locator('select[name="role"]').selectOption("employee");
    await ownerPage.getByLabel("Access expires", { exact: true }).fill("2030-01-01");
    const inviteResponse = ownerPage.waitForResponse((response) => response.request().method() === "POST" && response.url().endsWith(`/people/directory/${personId}/invite`));
    await ownerPage.getByRole("button", { name: "Create activation link", exact: true }).click();
    expect((await inviteResponse).ok()).toBe(true);
    const activationLink = await ownerPage.getByLabel("Activation link (expires in 48 hours)", { exact: true }).inputValue();
    expect(activationLink).toMatch(/\/activate\?token=/);

    await signInHolding(reviewerContext, reviewerPage, credentials, reviewer!, `${holding}/people/settings?section=reviews`);
    await ensureMfa(reviewerPage);
    await reviewerPage.goto(new URL(`${holding}/people/settings?section=reviews`, rootOrigin).toString(), { waitUntil: "domcontentloaded" });
    const invitation = reviewerPage.locator("article").filter({ hasText: "Authorize an employee invitation and initial grant." }).first();
    await expect(invitation).toBeVisible();
    await invitation.locator('select[name="decision"]').selectOption("approve");
    await invitation.locator('textarea[name="reason"]').fill("Reviewed H002 employee identity, manager, and branch-scoped access independently.");
    const decisionResponse = reviewerPage.waitForResponse((response) => response.request().method() === "POST" && response.url().includes("/reviews/"));
    await invitation.getByRole("button", { name: "Record decision", exact: true }).click();
    expect((await decisionResponse).ok()).toBe(true);
    await expect(invitation).toContainText("approved");

    await employeePage.goto(activationLink, { waitUntil: "domcontentloaded" });
    await expect(employeePage.getByRole("heading", { name: "Activate your ERPX account", exact: true })).toBeVisible();
    await employeePage.getByLabel("Password", { exact: true }).fill(employeePassword);
    await employeePage.getByLabel("Confirm password", { exact: true }).fill(employeePassword);
    await employeePage.getByRole("button", { name: "Activate account", exact: true }).click();
    await expect(employeePage).toHaveURL(/\/login$/);
    await employeePage.getByLabel("Work email", { exact: true }).fill(employeeEmail);
    await employeePage.getByLabel("Password", { exact: true }).fill(employeePassword);
    await employeePage.getByRole("button", { name: "Sign in", exact: true }).click();
    await expect(employeePage).toHaveURL(new RegExp(`${holding}/(?:branches/[^/]+/)?home$|/work-role\\?`));
    if (employeePage.url().includes("/work-role")) {
      await employeePage.getByRole("combobox", { name: "Role for actions & approvals" }).fill("employee");
      await employeePage.getByRole("listbox").getByRole("option").click();
    }
    await expect(employeePage).toHaveURL(new RegExp(`${holding}/(?:branches/[^/]+/)?home$`));
    await expect(employeePage.getByRole("main")).toBeVisible();

    const crossHoldingDirectory = await employeePage.request.get(new URL("/api/v1/holdings/simulation/people", rootOrigin).toString());
    expect([403, 404, 409], "an H002 employee cannot enumerate another holding's people").toContain(crossHoldingDirectory.status());
    expect(crossHoldingDirectory.ok(), "cross-holding requests must not return directory data").toBe(false);
    const crossHoldingPerson = await employeePage.request.get(new URL(`/api/v1/holdings/simulation/people/people/${personId}`, rootOrigin).toString());
    expect([403, 404, 409], "an H002 employee cannot read their employee record through another holding").toContain(crossHoldingPerson.status());
    expect(crossHoldingPerson.ok(), "cross-holding requests must not return an employee record").toBe(false);

    await ownerPage.goto(new URL(`${holding}/people/${personId}?tab=access`, rootOrigin).toString(), { waitUntil: "domcontentloaded" });
    const revokeResponse = ownerPage.waitForResponse((response) => response.request().method() === "POST" && response.url().endsWith(`/people/directory/${personId}/revoke`));
    await ownerPage.getByRole("button", { name: "Revoke role", exact: true }).click();
    expect((await revokeResponse).ok()).toBe(true);
    const denied = await employeePage.request.get(new URL(`/api/v1${holding}/people/people/${personId}`, rootOrigin).toString());
    expect([403, 409], "revoked employee data reads are rejected before the route can return data").toContain(denied.status());
    await employeePage.goto(new URL(`${holding}/home`, rootOrigin).toString(), { waitUntil: "domcontentloaded" });
    await expect(employeePage).toHaveURL(/\/access\?reason=permission|\/login|\/work-role\?/);
  } finally {
    await Promise.all([ownerContext.close(), reviewerContext.close(), employeeContext.close()]);
  }
});
