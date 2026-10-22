import { test, expect, type BrowserContext } from "@playwright/test";
import { createHmac, randomUUID } from "node:crypto";
import { execFileSync } from "node:child_process";
import { resolve } from "node:path";
import type { OrganizationOverview } from "@erpx/feature-organization/lib/organization";
import { pinnedBunBinary } from "../../../scripts/testing/runtime-tooling";
import { browserChildEnvironment } from "../../../scripts/testing/runner-environment";
import { selectAcmeHoldingOwner } from "../../support/browser/select-acme-holding-owner";
import { selectFixtureAssignment } from "../../support/browser/select-fixture-assignment";

async function commandHeaders(context: BrowserContext) {
  const response = await context.request.get("/api/v1/holdings/acme/assignments/active");
  expect(response.ok(), await response.text()).toBe(true);
  const { data } = await response.json() as { data: { valid: boolean; companyId: string; assignmentId: string; revision: number } };
  expect(data).toMatchObject({ valid: true, companyId: "acme-co", assignmentId: "asg_mali_owner" });
  return {
    Origin: process.env.ERPX_E2E_BASE_URL ?? "http://127.0.0.1:3100",
    "x-erpx-context-company": data.companyId,
    "x-erpx-context-assignment": data.assignmentId,
    "x-erpx-context-revision": String(data.revision),
    "Idempotency-Key": randomUUID(),
  };
}

test.use({ actionTimeout: 15000 });

test("approval rerouting has one authorized editor and handles a changed version", async ({
  page,
}, info) => {
  test.skip(
    !["localhost", "127.0.0.1"].includes(
      new URL(info.project.use.baseURL as string).hostname,
    ),
    "Creates isolated local work.",
  );
  test.setTimeout(90000);
  const base = "/holdings/acme";
  const workspace = page.locator("#workspace-content:visible");
  const login = await page.request.post("/api/auth/sign-in/email", {
    data: { email: "mali@erpx.dev", password: "Mali-ERPX-2026!" },
  });
  expect(login.ok(), await login.text()).toBe(true);
  await selectFixtureAssignment(page.context(), "acme-co", "asg_mali_owner");
  async function command(path: string, data: unknown) {
    const r = await page.request.post(`/api/v1${base}/${path}`, {
      data,
      headers: await commandHeaders(page.context()),
    });
    expect(r.ok(), await r.text()).toBeTruthy();
    return (await r.json()).data;
  }
  const task = await command("staff-tasks", {
    title: `Routing test ${randomUUID().slice(0, 8)}`,
    description: "Verify the centralized approval editor.",
    assigneeUserId: "usr_nisa",
    dueAt: "2030-01-01T12:00:00Z",
    projectId: null,
    startApprovers: ["usr_ava"],
    completionApprovers: ["usr_ava"],
  });
  try {
    await command(`staff-tasks/${task.id}/transition`, {
      action: "submit",
      expectedVersion: 1,
    });
    await page.goto(`${base}/staff-tasks/${task.id}`);
    await expect(workspace).toHaveCount(1);
    await expect(
      workspace.getByText("Administratively reroute current step", { exact: true }).filter({ visible: true }),
    ).toHaveCount(0);
    await workspace.getByRole("link", { name: "Resolve approval issue" }).click();
    await expect(page).toHaveURL(
      new RegExp(`section=approvals&kind=task&record=${task.id}`),
    );
    await expect(workspace).toHaveCount(1);
    await workspace
      .getByText("Administratively reroute current step", { exact: true })
      .filter({ visible: true })
      .click();
    await workspace
      .getByRole("combobox", { name: "Replacement approver", exact: true })
      .selectOption("usr_noah");
    await workspace
      .getByLabel("Rerouting reason")
      .fill("Cover the unavailable reviewer.");
    const current = (
      await (
        await page.request.get(`/api/v1${base}/staff-tasks/${task.id}`)
      ).json()
    ).data;
    await command(`approvals/${current.approval.instance.id}/reroute`, {
      expectedVersion: current.task.version,
      instanceVersion: current.approval.instance.version,
      approverUserId: "usr_ava",
      reason: "Concurrent configuration review.",
    });
    await workspace
      .getByRole("button", { name: "Record rerouting", exact: true })
      .click();
    await expect(workspace.getByRole("alert")).toBeVisible();
    await expect(workspace.getByLabel("Rerouting reason")).toHaveValue(
      "Cover the unavailable reviewer.",
    );
    await page.reload();
    await expect(workspace).toHaveCount(1);
    await workspace
      .getByText("Administratively reroute current step", { exact: true })
      .filter({ visible: true })
      .click();
    await workspace
      .getByRole("combobox", { name: "Replacement approver", exact: true })
      .selectOption("usr_noah");
    await workspace
      .getByLabel("Rerouting reason")
      .fill("Cover the unavailable reviewer.");
    await workspace
      .getByRole("button", { name: "Record rerouting", exact: true })
      .click();
    await expect(
      workspace.getByText("Waiting for Noah Patel", { exact: true }).filter({ visible: true }),
    ).toBeVisible();
    await page.goto(`${base}/staff-tasks/${task.id}`);
    await expect(workspace).toHaveCount(1);
    await expect(
      workspace.getByText("Current approver: Noah Patel", { exact: true }).filter({ visible: true }),
    ).toBeVisible();
    await workspace.getByText("Approval route and history", { exact: true }).filter({ visible: true }).click();
    await expect(
      workspace.getByText("Cover the unavailable reviewer.", { exact: true }).filter({ visible: true }),
    ).toBeVisible();
  } finally {
    const current = (
      await (
        await page.request.get(`/api/v1${base}/staff-tasks/${task.id}`)
      ).json()
    ).data.task;
    if (current.state !== "cancelled")
      await command(`staff-tasks/${task.id}/transition`, {
        action: "cancel",
        expectedVersion: current.version,
        note: "UI verification completed.",
      });
  }
});

test("a profile owns level, activation and distinct access commands", async ({
  page,
  browser,
}, info) => {
  test.skip(
    !["localhost", "127.0.0.1"].includes(
      new URL(info.project.use.baseURL as string).hostname,
    ),
    "Uses an isolated local employee.",
  );
  test.setTimeout(180000);
  const base = "/holdings/acme",
    suffix = randomUUID().slice(0, 8);
  const workspace = page.locator("#workspace-content:visible");
  const login = await page.request.post("/api/auth/sign-in/email", {
    data: { email: "mali@erpx.dev", password: "Mali-ERPX-2026!" },
  });
  expect(login.ok(), await login.text()).toBe(true);
  await selectFixtureAssignment(page.context(), "acme-co", "asg_mali_owner");
  await page.goto(`${base}/home`);
  await expect(workspace).toHaveCount(1);
  await page.getByRole("button", { name: "Switch workspace" }).click();
  await expect(
    page
      .getByRole("dialog", { name: "Switch workspace", exact: true })
      .getByRole("button", { name: /Acme Corporation/ }),
  ).toBeVisible();
  await page.keyboard.press("Escape");
  await expect(
    page.getByRole("dialog", { name: "Switch workspace", exact: true }),
  ).not.toBeVisible();
  async function command(path: string, data: unknown) {
    const response = await page.request.post(`/api/v1${base}/${path}`, {
      data,
      headers: await commandHeaders(page.context()),
    });
    expect(response.ok(), await response.text()).toBeTruthy();
    return (await response.json()).data;
  }
  const levels = [];
  for (const rank of [1, 2])
    levels.push(
      await command("staff/levels", {
        code: `UX_${suffix}_${rank}`,
        name: `UX level ${suffix} ${rank}`,
        rank,
        expectedVersion: 0,
      }),
    );
  const organizationResponse = await page.request.get(`/api/v1${base}/people/organization`);
  expect(organizationResponse.ok(), await organizationResponse.text()).toBe(true);
  const organization = (await organizationResponse.json()).data as OrganizationOverview;
  const position = organization.positions.find(item => item.active === 1 && item.level_number === 6 && item.branch_id);
  const manager = organization.people.find(item => item.status === "active" && item.person_status === "active" && item.level_number !== null && item.level_number < 6 && item.branch_id === position?.branch_id);
  const staffLevel = organization.levels.find(item => item.level_number === 6);
  expect(position && manager && staffLevel, "An active Staff position and senior reporting manager are required for activation").toBeTruthy();
  const branchId = position!.branch_id;
  const person = await command("people/directory", {
    legalName: `UX employee ${suffix}`,
    email: `ux-${suffix}@example.test`,
    countryCode: "TH",
    branchId,
    departmentId: position!.department_id,
    positionId: position!.id,
    managerPersonId: manager!.person_id,
    jobTitle: "UX test operator",
    workerType: "employee",
    employmentType: "salaried",
    startDate: "2026-09-01",
    currency: "THB",
    levelId: levels[0].id,
  });
  const profile = `${base}/people/${person.id}`;
  const reviewerContext = await browser.newContext(info.project.use);
  const reviewerPage = await reviewerContext.newPage();
  const reviewerWorkspace = reviewerPage.locator("#workspace-content:visible");
  const workerReceipts: { action: string; run: string; jobsPaused: boolean; jobsPid?: number }[] = [];
  const accessExpires = new Date(Date.now() + 30 * 86400000).toISOString().slice(0, 10);
  const run = process.env.ERPX_BROWSER_SIMULATION_RUN;
  const origin = new URL(process.env.ERPX_E2E_BASE_URL ?? "http://127.0.0.1:3102");
  expect(run, "Only the current disposable browser cohort may schedule lifecycle work").toMatch(/^[a-f0-9]{12}$/);
  expect(["127.0.0.1", "localhost"]).toContain(origin.hostname);
  expect(["3102", "3104"]).toContain(origin.port);
  function workerControl(action: "pause-jobs" | "resume-jobs") {
    const result = JSON.parse(execFileSync(pinnedBunBinary(resolve(".")), ["--env-file=/dev/null", "scripts/local/run-server-script.ts", "scripts/local/browser-simulation.ts", action, run!], {
      cwd: resolve("."), timeout: 30_000,
      env: browserChildEnvironment(process.env, { ERPX_BROWSER_SIMULATION_PORT: origin.port, ERPX_BROWSER_SIMULATION_ATTACHMENTS: "local", ERPX_BROWSER_SIMULATION_JOBS: "1" }),
    }).toString()) as { run: string; jobsPaused: boolean; jobsPid?: number };
    expect(result.run).toBe(run);
    expect(result.jobsPaused).toBe(action === "pause-jobs");
    workerReceipts.push({ action, ...result });
  }
  async function currentPerson() {
    const response = await page.request.get(`/api/v1${base}/people/people/${person.id}`);
    expect(response.ok(), await response.text()).toBe(true);
    return (await response.json()).data.person as { version: number; status: string; engagement_status: string; user_id: string | null };
  }
  async function enrollMfa(target: typeof page) {
    await target.goto("/security");
    const security = target.locator("#security-content:visible");
    await expect(security).toHaveCount(1);
    await target.getByRole("button", { name: "Set up authenticator", exact: true }).click();
    const secretElement = security.locator("code:visible");
    await expect(secretElement).toHaveCount(1);
    const secret = await secretElement.innerText(), alphabet = "ABCDEFGHIJKLMNOPQRSTUVWXYZ234567", bytes: number[] = [];
    let value = 0, bits = 0;
    for (const character of secret) {
      value = (value << 5) | alphabet.indexOf(character);
      bits += 5;
      if (bits >= 8) { bytes.push((value >>> (bits - 8)) & 255); bits -= 8; }
    }
    const counter = Buffer.alloc(8);
    counter.writeBigUInt64BE(BigInt(Math.floor(Date.now() / 30_000)));
    const digest = createHmac("sha1", Buffer.from(bytes)).update(counter).digest(), offset = digest[19] & 15;
    await target.getByRole("textbox", { name: "Six-digit code", exact: true }).fill(String((digest.readUInt32BE(offset) & 0x7fffffff) % 1_000_000).padStart(6, "0"));
    await target.getByRole("button", { name: "Verify code", exact: true }).click();
    await expect(target.getByRole("status").filter({ hasText: "Authenticator verified" })).toBeVisible();
  }
  async function approveAccess(targetId: string, reviewId: string) {
    await reviewerPage.goto(`${base}/people/settings?section=reviews`);
    await expect(reviewerWorkspace).toHaveCount(1);
    const article = reviewerWorkspace.locator("article:visible").filter({ hasText: targetId }).filter({ has: reviewerPage.getByRole("button", { name: "Record decision", exact: true }) });
    await expect(article).toHaveCount(1);
    await article.getByRole("combobox", { name: "Decision", exact: true }).selectOption("approve");
    await article.getByLabel("Reason", { exact: true }).fill(`Independent review of employee access ${suffix}`);
    const response = reviewerPage.waitForResponse(result => result.request().method() === "POST" && result.url().endsWith(`/iam/reviews/${reviewId}`));
    await article.getByRole("button", { name: "Record decision", exact: true }).click();
    const reviewed = await response;
    expect(reviewed.ok(), await reviewed.text()).toBe(true);
    expect((await reviewed.json()).data).toMatchObject({ id: reviewId, status: "approved" });
  }
  async function lifecycleChange(kind: "suspend" | "transfer" | "offboarding", status: "suspended" | "active" | "ended") {
    await page.goto(profile);
    await expect(workspace).toHaveCount(1);
    const startChange = workspace.getByRole("button", { name: "Start a change", exact: true });
    await expect(startChange).toHaveAttribute("data-client-managed-ready", "true");
    await startChange.click();
    const dialog = page.getByRole("dialog", { name: `UX employee ${suffix} · employment change`, exact: true });
    await expect(dialog).toBeVisible();
    await dialog.getByRole("combobox", { name: "Change", exact: true }).selectOption(kind);
    if (kind === "transfer") {
      await dialog.getByRole("combobox", { name: "Position", exact: true }).selectOption(position!.id);
      await dialog.getByRole("combobox", { name: "Actual reporting manager", exact: true }).selectOption(manager!.person_id);
      await dialog.getByLabel("Assign the Level leave default", { exact: true }).uncheck();
    }
    if (kind === "offboarding") await dialog.locator('input[name="lastWorkingDate"]').fill(new Date().toISOString().slice(0, 10));
    await dialog.getByRole("combobox", { name: "Timezone", exact: true }).selectOption("UTC");
    await dialog.locator('input[name="effective"]').fill(new Date(Date.now() - 60_000).toISOString().slice(0, 16));
    await dialog.getByLabel("Reason", { exact: true }).fill(`Profile command verification: ${kind} ${suffix}`);
    await dialog.getByRole("button", { name: "Review placement", exact: true }).click();
    await expect(dialog.getByRole("button", { name: "Submit for scheduled processing", exact: true })).toBeEnabled();
    // Restart only this cohort's genuine worker to run its due schedule now.
    workerControl("pause-jobs");
    try {
      const response = page.waitForResponse(result => result.request().method() === "POST" && result.url().endsWith("/people/lifecycle/placements/submit"));
      await dialog.getByRole("button", { name: "Submit for scheduled processing", exact: true }).click();
      const submitted = await response;
      expect(submitted.ok(), await submitted.text()).toBe(true);
      expect((await submitted.json()).data).toMatchObject({ status: "scheduled" });
      await expect(dialog.getByRole("heading", { name: "Request saved", exact: true })).toBeVisible();
    } catch (error) {
      // Retain the primary UI/command failure; runner finally still removes the cohort.
      try { workerControl("resume-jobs"); } catch (resumeError) {
        await info.attach("lifecycle-worker-resume-error", { contentType: "text/plain", body: String(resumeError) });
      }
      throw error;
    }
    workerControl("resume-jobs");
    await expect.poll(async () => (await currentPerson()).engagement_status).toBe(status);
    await page.goto(profile);
  }
  try {
    const reviewerLogin = await reviewerContext.request.post("/api/auth/sign-in/email", { data: { email: "jesadakorn.kirtnu@gmail.com", password: "12345678" } });
    expect(reviewerLogin.ok(), await reviewerLogin.text()).toBe(true);
    await selectAcmeHoldingOwner(reviewerPage);
    for (const [context, assignmentId] of [[page.context(), "asg_mali_owner"], [reviewerContext, "dev_usr_jesadakorn_acme-co_holding_owner"]] as const) {
      const selection = await context.request.get(`/api/v1${base}/assignments/active`);
      expect(selection.ok(), await selection.text()).toBe(true);
      const { data } = await selection.json() as { data: { valid: boolean; assignmentId: string; options: { id: string; validUntil: number }[] } };
      expect(data).toMatchObject({ valid: true, assignmentId });
      const authority = data.options.find(option => option.id === assignmentId);
      expect(authority, "The proposed access expiry must fit the current authorizing and reviewing assignments").toBeDefined();
      expect(Date.parse(accessExpires) + 86400000).toBeLessThanOrEqual(authority!.validUntil);
    }
    await enrollMfa(page);
    await enrollMfa(reviewerPage);
    await page.goto(profile);
    await expect(workspace).toHaveCount(1);
    await workspace.getByText("Change staff level", { exact: true }).filter({ visible: true }).click();
    await workspace
      .getByRole("combobox", { name: "New level", exact: true })
      .selectOption(levels[1].id);
    const reason = `Development review ${suffix}`;
    await workspace.getByLabel("Change reason").fill(reason);
    const keys: string[] = [];
    await page.route("**/staff/engagements/*/level", (route) => {
      keys.push(route.request().headers()["idempotency-key"]);
      return keys.length === 1
        ? route.fulfill({
            status: 503,
            contentType: "application/json",
            body: JSON.stringify({ error: { message: "Retry level change" } }),
          })
        : route.continue();
    });
    await workspace.getByRole("button", { name: "Record level change" }).click();
    await expect(workspace.getByRole("alert")).toHaveText(
      "Retry level change",
    );
    await expect(workspace.getByLabel("Change reason")).toHaveValue(reason);
    await workspace.getByRole("button", { name: "Record level change" }).click();
    await expect(
      workspace.getByRole("definition").filter({ hasText: `UX level ${suffix} 2` }),
    ).toBeVisible();
    expect(keys).toHaveLength(2);
    expect(keys[0]).toBe(keys[1]);
    await workspace.getByText("Level history", { exact: true }).filter({ visible: true }).click();
    await expect(
      workspace.locator("details:visible").filter({ hasText: "Level history" }),
    ).toContainText(reason);
    // UX levels remain custom; activation uses one of the installed six levels.
    await workspace.getByRole("combobox", { name: "New level", exact: true }).selectOption(staffLevel!.id);
    await workspace.getByLabel("Change reason").fill(`Confirm canonical staff placement ${suffix}`);
    await workspace.getByRole("button", { name: "Record level change", exact: true }).click();
    await expect(workspace.getByRole("definition").filter({ hasText: staffLevel!.name })).toBeVisible();
    const activated = page.waitForResponse(response => response.request().method() === "POST" && response.url().endsWith(`/people/directory/${person.id}/status`));
    await workspace
      .getByRole("button", { name: "Activate employee", exact: true })
      .click();
    expect((await activated).ok()).toBe(true);
    expect(await currentPerson()).toMatchObject({ status: "active", engagement_status: "active" });
    await expect(workspace.getByRole("button", { name: "Deactivate employee", exact: true })).toHaveCount(0);
    const accessWidget = workspace.locator('[data-feature-widget="person-access"]:visible');
    await expect(accessWidget).toHaveCount(1);
    await expect(accessWidget).toBeVisible();
    await expect(accessWidget.getByText("No login account is linked to this employee.", { exact: true })).toBeVisible();
    await accessWidget.getByText("Invite to ERPX", { exact: true }).filter({ visible: true }).click();
    const inviteDialog = page.getByRole("dialog", { name: "Invite to ERPX", exact: true });
    await inviteDialog.getByRole("combobox", { name: "System role", exact: true }).selectOption("employee");
    await inviteDialog.getByLabel("Access expires").fill(accessExpires);
    const invitationResponse = page.waitForResponse(response => response.request().method() === "POST" && response.url().endsWith(`/people/directory/${person.id}/invite`));
    await inviteDialog.getByRole("button", { name: "Create activation link" }).click();
    const invited = await invitationResponse;
    expect(invited.ok(), await invited.text()).toBe(true);
    const invitation = (await invited.json()).data as { id: string; reviewId: string };
    await inviteDialog.getByRole("button", { name: "Close dialog", exact: true }).click();
    await expect(inviteDialog).not.toBeVisible();
    const link = accessWidget.getByLabel("Activation link (expires in 48 hours)");
    await expect(link).toHaveValue(/\/activate\?token=/);
    const token = new URL(await link.inputValue()).searchParams.get("token");
    await approveAccess(invitation.id, invitation.reviewId);
    const activation = await page.request.post("/api/v1/account-invitations", {
      data: { token, password: `UX-activation-${suffix}!` },
    });
    expect(activation.ok(), await activation.text()).toBeTruthy();
    await page.reload();
    const linkedUserId = (await currentPerson()).user_id;
    expect(linkedUserId).toBeTruthy();
    await expect(
      accessWidget.getByRole("button", { name: "Revoke role", exact: true }),
    ).toHaveCount(1);
    await accessWidget
      .getByRole("button", { name: "Revoke role", exact: true })
      .click();
    await expect(
      accessWidget.getByRole("button", { name: "Revoke role", exact: true }),
    ).toHaveCount(0);
    async function grant() {
      await accessWidget.locator("details:visible").getByText("Grant role", { exact: true }).filter({ visible: true }).click();
      const dialog = page.getByRole("dialog", { name: "Grant role", exact: true });
      await dialog.getByRole("combobox", { name: "System role", exact: true }).selectOption("employee");
      await dialog.getByLabel("Access expires").fill(accessExpires);
      const response = page.waitForResponse(result => result.request().method() === "POST" && result.url().endsWith(`/people/directory/${person.id}/grant`));
      await dialog.getByRole("button", { name: "Grant role", exact: true }).click();
      const granted = await response;
      expect(granted.ok(), await granted.text()).toBe(true);
      const review = (await granted.json()).data as { id: string; status: string };
      expect(review.status).toBe("pending");
      await dialog.getByRole("button", { name: "Close dialog", exact: true }).click();
      await expect(dialog).not.toBeVisible();
      await approveAccess(linkedUserId!, review.id);
      await page.reload();
      await expect(
        accessWidget.getByRole("button", { name: "Revoke role", exact: true }),
      ).toHaveCount(1);
    }
    await grant();
    await accessWidget.getByText("Company access", { exact: true }).filter({ visible: true }).click();
    await accessWidget
      .getByRole("button", { name: "Revoke company access", exact: true })
      .click();
    await expect(
      accessWidget.getByRole("button", { name: "Revoke role", exact: true }),
    ).toHaveCount(0);
    await page.reload();
    await grant();
    await lifecycleChange("suspend", "suspended");
    await lifecycleChange("transfer", "active");
    await expect(accessWidget).toHaveCount(1);
    await expect(accessWidget).toBeVisible();
    await expect(accessWidget.getByText("Login account linked. Roles below control access to this workspace.", { exact: true })).toBeVisible();
    await expect(
      accessWidget.getByRole("button", { name: "Revoke role", exact: true }),
    ).toHaveCount(0);
    await expect(
      accessWidget.getByText("Login account linked.", { exact: false }).filter({ visible: true }),
    ).toBeVisible();
    expect((await currentPerson()).user_id).toBe(linkedUserId);
    await lifecycleChange("offboarding", "ended");
    const archived = page.waitForResponse(response => response.request().method() === "POST" && response.url().endsWith(`/people/directory/${person.id}/status`));
    await workspace.getByRole("button", { name: "Archive employee", exact: true }).click();
    expect((await archived).ok()).toBe(true);
    expect((await currentPerson()).status).toBe("archived");
    await info.attach("actual-lifecycle-worker-receipts", { contentType: "application/json", body: JSON.stringify(workerReceipts, null, 2) });
  } finally {
    await reviewerContext.close();
  }
});
