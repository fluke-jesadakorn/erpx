import { test, expect, type BrowserContext, type Page } from "@playwright/test";
import { randomUUID } from "node:crypto";
import type { AttachmentSetDto, AttachmentStatusDto } from "../../src/lib/attachment-client";
import { prepareLegacyLeaveReviewer } from "../helpers/legacy-leave-reviewer-browser";
import { selectFixtureAssignment } from "./helpers/select-fixture-assignment";
test.use({ actionTimeout: 10000 });
const png = Buffer.from(
  "iVBORw0KGgoAAAANSUhEUgAAAAIAAAACCAYAAABytg0kAAAACXBIWXMAAAPoAAAD6AG1e1JrAAAAEUlEQVQImWOQaEj4D8IMMAYAQaQH3RNNL/AAAAAASUVORK5CYII=",
  "base64",
);
type PersistedLeave = { id: string; state: string; version: number; evidence_version: number; handover_note: string; approval: { currentApprover: { approver_user_id: string } | null } };
async function readData<T>(context: BrowserContext, api: string): Promise<T> {
  const response = await context.request.get(api);
  expect(response.ok(), await response.text()).toBe(true);
  return (await response.json() as { data: T }).data;
}
async function readLeave(context: BrowserContext, api: string): Promise<PersistedLeave> {
  return readData<PersistedLeave>(context, api);
}
async function signIn(page: Page, email: string, password: string, assignmentId: string) {
  await page.goto("/login");
  await page.getByLabel("Work email").fill(email);
  await page.getByLabel(/^Password(?:\s*\*)?$/).fill(password);
  await page.getByRole("button", { name: "Sign in", exact: true }).click();
  await expect(page).toHaveURL(/\/holdings\/acme(?:\/branches\/[^/]+)?\/(?:home|work-role)(?:\?.*)?$/);
  await selectFixtureAssignment(page.context(), "acme-co", assignmentId);
  await page.goto("/holdings/acme/home");
  await expect(page).toHaveURL(/\/holdings\/acme\/home$/);
}
test("local draft review, failed request retry, evidence and independent approval", async ({
  browser,
}, testInfo) => {
  test.skip(
    !["localhost", "127.0.0.1"].includes(
      new URL(testInfo.project.use.baseURL as string).hostname,
    ),
    "Fixture creation is local only.",
  );
  if (!/^[a-f0-9]{12}$/.test(process.env.ERPX_BROWSER_SIMULATION_RUN ?? "")) throw new Error("DISPOSABLE_BROWSER_COHORT_REQUIRED");
  test.setTimeout(240000);
  await prepareLegacyLeaveReviewer(await browser.newContext(testInfo.project.use));
  const employee = await browser.newContext(testInfo.project.use);
  const manager = await browser.newContext(testInfo.project.use);
  const humanResources = await browser.newContext(testInfo.project.use);
  const fixtureIds: string[] = [];
  let id = "";
  try {
    const page = await employee.newPage();
    await signIn(page, "mali@erpx.dev", "Mali-ERPX-2026!", "asg_mali_owner");
    const selected = await employee.request.get(
      "/api/v1/holdings/acme/branches/bangkok/assignments/active",
    );
    expect(selected.ok(), await selected.text()).toBe(true);
    const { data: active } = await selected.json() as {
      data: { valid: boolean; companyId: string; userId: string; assignmentId: string | null; revision: number };
    };
    expect(active).toMatchObject({ valid: true, companyId: "acme-co", userId: "usr_mali" });
    expect(active.assignmentId).toBeTruthy();
    for (let index = 0; index < 6; index++) {
      const response = await employee.request.post(
        "/api/v1/holdings/acme/branches/bangkok/leave-requests",
        {
          headers: {
            "Idempotency-Key": randomUUID(),
            Origin: new URL(testInfo.project.use.baseURL as string).origin,
            "x-erpx-context-company": active.companyId,
            "x-erpx-context-assignment": String(active.assignmentId),
            "x-erpx-context-revision": String(active.revision),
          },
          data: {
            startDate: "2026-09-14",
            days: 2,
            handoverNote: "Isolated UI verification request",
          },
        },
      );
      expect(response.ok(), await response.text()).toBe(true);
      const created = (await response.json()).data;
      expect(created).toMatchObject({ state: "draft", version: 1 });
      fixtureIds.push(created.id);
    }
    id = fixtureIds[0];
    const number = `LR-${id.slice("leave-".length)}`;
    await page.goto(
      "/holdings/acme/branches/bangkok/work?queue=todo&type=leave",
    );
    const count = page
      .getByRole("navigation", { name: "Work pagination", exact: true })
      .getByText(/^\d+ matching items$/);
    await expect(count).toBeVisible();
    expect(Number((await count.innerText()).split(" ")[0])).toBeGreaterThanOrEqual(
      fixtureIds.length,
    );
    const results = page.getByRole("region", { name: "Work results", exact: true });
    for (const fixtureId of fixtureIds) {
      const fixtureNumber = `LR-${fixtureId.slice("leave-".length)}`;
      const row = results.getByRole("link", { name: new RegExp(fixtureNumber) });
      await expect(row).toBeVisible();
      await expect(row).toHaveAttribute("href", new RegExp(`/tasks/people_payroll/${fixtureId}\\?`));
    }
    await page.goto(
      `/holdings/acme/branches/bangkok/work?queue=todo&q=${number}`,
    );
    await results.getByRole("link", { name: new RegExp(number) }).click();
    const drawer = page.getByRole("dialog", { name: "Annual leave", exact: true });
    await expect(page).toHaveURL(url => url.pathname === "/holdings/acme/branches/bangkok/work"
      && url.searchParams.get("queue") === "todo" && url.searchParams.get("q") === number
      && url.searchParams.get("detailKind") === "leave" && url.searchParams.get("detailId") === id);
    await expect(drawer).toBeVisible();
    await expect(drawer.getByText(`Personal leave request · ${number}`, { exact: true })).toBeVisible();
    await page.goBack();
    await expect(page).toHaveURL(url => url.pathname === "/holdings/acme/branches/bangkok/work"
      && url.searchParams.get("queue") === "todo" && url.searchParams.get("q") === number
      && !url.searchParams.has("detailKind") && !url.searchParams.has("detailId"));
    await expect(drawer).toBeHidden();
    await expect(page.getByLabel("Search work", { exact: true })).toHaveValue(
      number,
    );
    await results.getByRole("link", { name: new RegExp(number) }).click();
    await expect(drawer).toBeVisible();
    const fullPage = drawer.getByRole("link", { name: "Open full page ↗", exact: true });
    await expect(fullPage).toHaveAttribute("href", new RegExp(`/tasks/people_payroll/${id}\\?return=`));
    await fullPage.click();
    await expect(page).toHaveURL(new RegExp(`/tasks/people_payroll/${id}\\?return=`));
    const api = `/api/v1/holdings/acme/branches/bangkok/leave-requests/${id}`;
    const draft = await readLeave(employee, api);
    expect(draft).toMatchObject({ id, state: "draft", version: 1, evidence_version: 0 });
    const handover = "The receiving team will cover my morning deliveries.";
    await page.getByRole("tab", { name: "Evidence", exact: true }).click();
    const evidence = page.getByRole("heading", { name: "Supporting photos and documents", exact: true }).locator("xpath=ancestor::section[1]");
    await expect(evidence).toBeVisible();
    await evidence.locator('input[type="file"]:not([capture])').setInputFiles({ name: "handover.png", mimeType: "image/png", buffer: png });
    await expect(evidence.getByText("handover.png", { exact: true })).toBeVisible();
    await expect(evidence.getByText("Ready to attach", { exact: true })).toBeVisible({ timeout: 120000 });
    const download = evidence.getByRole("link", { name: "Download handover.png", exact: true });
    await expect(download).toBeVisible();
    const originalUrl = (await download.getAttribute("href"))!;
    const originalPath = new URL(originalUrl, testInfo.project.use.baseURL as string).pathname;
    expect(originalPath).toMatch(/^\/api\/v1\/holdings\/acme(?:\/branches\/bangkok)?\/attachment-links\/[^/]+\/content$/);
    const linkId = originalPath.split("/").at(-2)!;
    const filesApi = originalPath.slice(0, originalPath.indexOf("/attachment-links/"));
    const statusApi = `${filesApi}/attachment-links/${linkId}/status`;
    const setsApi = `${filesApi}/attachment-sets?${new URLSearchParams({ targetType: "leave_request", targetId: id, purposeKey: "leave_supporting_photo", componentKind: "record" })}`;
    const staged = await readData<AttachmentStatusDto>(employee, statusApi);
    expect(staged).toMatchObject({ linkId, sourceState: "available", linkState: "draft", bindable: true, boundAt: null });
    expect(staged.sourceId).toBeTruthy();
    const stagedSets = await readData<AttachmentSetDto[]>(employee, setsApi);
    expect(stagedSets).toHaveLength(1);
    expect(stagedSets[0]).toMatchObject({ id: staged.setId, targetType: "leave_request", targetId: id, purposeKey: "leave_supporting_photo", componentKind: "record", componentRef: "leave-evidence:leave_supporting_photo", expectedTargetVersion: draft.version, state: "draft" });
    expect(stagedSets[0].links).toHaveLength(1);
    const stagedLink = stagedSets[0].links[0];
    expect(stagedLink).toMatchObject({ id: linkId, sourceId: staged.sourceId, filename: "handover.png", mediaType: "image/png", bytes: png.byteLength, sourceState: "available", linkState: "draft", boundAt: null });
    const original = await employee.request.get(originalUrl);
    expect(original.ok()).toBe(true);
    expect(original.headers()["cache-control"]).toBe("private, no-store");
    expect(original.headers()["content-disposition"]).toContain("attachment");
    const originalBytes = await original.body();
    expect(originalBytes).toEqual(png);
    const withEvidence = await readLeave(employee, api);
    // Uploading clean evidence stages it without changing the owner. The first
    // independent decision will freeze this exact set inside its transaction.
    expect(withEvidence).toMatchObject({ state: "draft", version: draft.version, evidence_version: draft.evidence_version });
    const review = await manager.newPage();
    await signIn(review, "nisa@erpx.dev", "Nisa-ERPX-2026!", "workflow_controller");
    for (const privateApi of [originalUrl, statusApi, setsApi]) {
      const forbidden = await manager.request.get(privateApi);
      expect(forbidden.status()).toBe(403);
      expect(await forbidden.json()).toMatchObject({ error: { code: "LEAVE_EVIDENCE_FORBIDDEN" } });
    }
    await page.getByRole("tab", { name: "Overview", exact: true }).click();
    await page.getByRole("textbox", { name: /^Handover note/ }).fill(handover);
    await expect(page.getByRole("textbox", { name: /^Handover note/ })).toHaveValue(handover);
    await page
      .getByRole("button", { name: "Review request", exact: true })
      .click();
    await expect(page.getByRole("heading", { name: "Check the calculation", exact: true })).toBeVisible();
    const attempts: { key: string; body: string | null }[] = [];
    let loseResponse = true;
    let committed: { id: string; state: string; version: number } | null = null;
    await page.route(
      `**${api}`,
      async (route) => {
        if (route.request().method() !== "PATCH") return route.continue();
        attempts.push({ key: route.request().headers()["idempotency-key"], body: route.request().postData() });
        if (loseResponse) {
          loseResponse = false;
          // Commit the real command, then lose its response. Retry must replay
          // the same receipt without another transition or balance reservation.
          const first = await route.fetch();
          expect(first.ok(), await first.text()).toBe(true);
          committed = (await first.json()).data;
          await route.abort("failed");
        } else await route.continue();
      },
    );
    await page
      .getByRole("button", { name: "Submit leave request", exact: true })
      .click();
    await expect(page.getByRole("alert").filter({ hasText: /Failed to fetch|Load failed|NetworkError|Connection interrupted/i })).toBeVisible();
    expect(committed).toMatchObject({ id, state: "pending_approval", version: withEvidence.version + 1 });
    expect(await readLeave(employee, api)).toMatchObject({ state: "pending_approval", version: withEvidence.version + 1, handover_note: handover, evidence_version: draft.evidence_version });
    expect(await readData<AttachmentStatusDto>(employee, statusApi)).toMatchObject({ linkId, setId: staged.setId, sourceId: staged.sourceId, sourceState: "available", linkState: "draft", boundAt: null });
    const retriedResponse = page.waitForResponse(response => new URL(response.url()).pathname === api && response.request().method() === "PATCH");
    await page
      .getByRole("button", { name: "Submit leave request", exact: true })
      .click();
    const retried = await retriedResponse;
    expect(retried.ok(), await retried.text()).toBe(true);
    expect(retried.request().postDataJSON()).toMatchObject({ expectedVersion: withEvidence.version, saveOnly: false, handoverNote: handover });
    expect((await retried.json()).data).toEqual(committed);
    expect(attempts).toHaveLength(2);
    expect(attempts[0].key).toBeTruthy();
    expect(attempts[0]).toEqual(attempts[1]);
    await page.unroute(`**${api}`);
    const pending = await readLeave(employee, api);
    expect(pending).toMatchObject({ state: "pending_approval", version: withEvidence.version + 1, handover_note: handover, evidence_version: draft.evidence_version, approval: { currentApprover: { approver_user_id: "usr_nisa" } } });
    expect(await readData<AttachmentSetDto[]>(employee, setsApi)).toEqual(stagedSets);
    await page.reload();
    await expect(
      page.getByRole("button", { name: "Approve", exact: true }),
    ).not.toBeVisible();
    await review.goto(`/holdings/acme/tasks/people_payroll/${id}`);
    await expect(review.getByText(handover, { exact: true })).toBeVisible();
    await review.getByRole("tab", { name: "Evidence", exact: true }).click();
    await expect(
      review.getByRole("link", { name: "Download handover.png", exact: true }),
    ).toBeVisible();
    const reviewerOriginal = await manager.request.get(originalUrl);
    expect(reviewerOriginal.ok(), await reviewerOriginal.text()).toBe(true);
    expect(reviewerOriginal.headers()["cache-control"]).toBe("private, no-store");
    expect(await reviewerOriginal.body()).toEqual(originalBytes);
    await review.getByRole("tab", { name: "Overview", exact: true }).click();
    await review.getByRole("textbox", { name: "Decision or cancellation reason", exact: true }).fill("Handover reviewed.");
    await review.getByRole("button", { name: "Approve", exact: true }).click();
    const managerApi = `/api/v1/holdings/acme/leave-requests/${id}`;
    expect(await readLeave(manager, managerApi)).toMatchObject({ state: "pending_approval", version: pending.version });
    const managerResponse = review.waitForResponse(response => new URL(response.url()).pathname === managerApi + "/approve" && response.request().method() === "POST");
    await review.getByRole("button", { name: "Confirm Approve", exact: true }).click();
    const reviewed = await managerResponse;
    expect(reviewed.ok(), await reviewed.text()).toBe(true);
    expect(reviewed.request().postDataJSON()).toMatchObject({ expectedVersion: pending.version, note: "Handover reviewed." });
    expect((await reviewed.json()).data).toMatchObject({ state: "pending_approval", version: pending.version + 1 });
    const hrPending = await readLeave(manager, managerApi);
    expect(hrPending).toMatchObject({ state: "pending_approval", version: pending.version + 1, evidence_version: draft.evidence_version + 1, approval: { currentApprover: { approver_user_id: "usr_ava" } } });
    const bound = await readData<AttachmentStatusDto>(employee, statusApi);
    expect(bound).toMatchObject({ linkId, setId: staged.setId, sourceId: staged.sourceId, sourceState: "available", linkState: "bound", bindable: false });
    expect(bound.boundAt).toBeGreaterThan(0);
    const boundSets = await readData<AttachmentSetDto[]>(employee, setsApi);
    expect(boundSets).toHaveLength(1);
    expect(boundSets[0]).toMatchObject({ id: staged.setId, targetType: "leave_request", targetId: id, purposeKey: "leave_supporting_photo", componentKind: "record", componentRef: "leave-evidence:leave_supporting_photo", expectedTargetVersion: draft.version, state: "bound", links: [{ ...stagedLink, linkState: "bound", boundAt: bound.boundAt }] });
    const boundOriginal = await employee.request.get(originalUrl);
    expect(boundOriginal.ok(), await boundOriginal.text()).toBe(true);
    expect(boundOriginal.headers()["cache-control"]).toBe("private, no-store");
    expect(await boundOriginal.body()).toEqual(originalBytes);
    // The same historical set remains visible after the decision advances the
    // owner version; it must not be replaced by a second staged copy.
    await page.reload();
    await page.getByRole("tab", { name: "Evidence", exact: true }).click();
    await expect(evidence.getByRole("heading", { name: "Attached history", exact: true })).toBeVisible();
    await expect(evidence.getByText("Attached and locked", { exact: true })).toBeVisible();
    await expect(download).toHaveAttribute("href", originalUrl);
    await page.reload();
    await page.getByRole("tab", { name: "History", exact: true }).click();
    await expect(
      page.getByText("Current approver: Ava Williams"),
    ).toBeVisible();
    const hr = await humanResources.newPage();
    await hr.goto("/login");
    await hr.getByLabel("Work email").fill("ava@erpx.dev");
    await hr.getByLabel(/^Password(?:\s*\*)?$/).fill("Ava-ERPX-2026!");
    await hr.getByRole("button", { name: "Sign in", exact: true }).click();
    await expect(hr).toHaveURL(/\/holdings\/acme/);
    await hr.goto(`/holdings/acme/branches/bangkok/tasks/people_payroll/${id}`);
    expect(await readLeave(humanResources, api)).toMatchObject({ state: "pending_approval", version: hrPending.version });
    await hr.getByRole("button", { name: "Approve", exact: true }).click();
    const hrResponse = hr.waitForResponse(response => new URL(response.url()).pathname === api + "/approve" && response.request().method() === "POST");
    await hr.getByRole("button", { name: "Confirm Approve", exact: true }).click();
    const approved = await hrResponse;
    expect(approved.ok(), await approved.text()).toBe(true);
    expect(approved.request().postDataJSON()).toMatchObject({ expectedVersion: hrPending.version });
    expect((await approved.json()).data).toMatchObject({ state: "approved", version: hrPending.version + 1 });
    await expect(hr.getByText("Your leave is approved.", { exact: true })).toBeVisible();
    await page.reload();
    await page.getByRole("tab", { name: "Overview", exact: true }).click();
    await expect(page.getByRole("tabpanel", { name: "Overview", exact: true }).getByText("Your approved leave remains in place. You can request a cancellation review if plans change.", { exact: true })).toBeVisible();
    for (const [context, endpoint] of [[employee, api], [manager, managerApi], [humanResources, api]] as const)
      expect(await readLeave(context, endpoint)).toMatchObject({ state: "approved", version: hrPending.version + 1, handover_note: handover, evidence_version: hrPending.evidence_version, approval: { currentApprover: null } });
    expect(await readData<AttachmentSetDto[]>(employee, setsApi)).toEqual(boundSets);
    expect(await readData<AttachmentStatusDto>(employee, statusApi)).toMatchObject({ linkId, setId: staged.setId, sourceId: staged.sourceId, sourceState: "available", linkState: "bound", boundAt: bound.boundAt });
    const finalOriginal = await manager.request.get(originalUrl);
    expect(finalOriginal.ok(), await finalOriginal.text()).toBe(true);
    expect(finalOriginal.headers()["cache-control"]).toBe("private, no-store");
    expect(await finalOriginal.body()).toEqual(originalBytes);
    await page.getByRole("tab", { name: "History", exact: true }).click();
    const history = page.getByRole("tabpanel", { name: "History", exact: true });
    await history.getByText("Approval route and history", { exact: true }).click();
    await expect(
      history.getByText("Handover reviewed.", { exact: true }),
    ).toBeVisible();
  } finally {
    await employee.close().catch(() => {});
    await manager.close().catch(() => {});
    await humanResources.close().catch(() => {});
    // The runner destroys this case’s disposable PostgreSQL and files in finally.
  }
});
