import {createEngineeringBrowserContext} from "../../support/browser/engineering-browser-evidence";
import { expect, test, type BrowserContext, type Page } from "@playwright/test";
import { readFile } from "node:fs/promises";
import { resolve } from "node:path";

type DemoUser = { email: string; role: string; roles?: string[] };
type Credentials = { password: string; users: DemoUser[] };
type DemoState = { fixture?: { customerId?: string; quotationId?: string } };

async function openFullProfileView(page: Page, view: "History" | "Summary") {
  const drawer = page.getByTestId("record-detail-drawer").filter({ visible: true });
  const link = drawer.getByRole("navigation", { name: "Customer profile views", exact: true }).getByRole("link", { name: view, exact: true });
  await link.click();
  await expect(drawer).toHaveCount(1);
  await expect(link).toHaveAttribute("aria-current", "page");
}

async function signInRole(context: BrowserContext, page: Page, credentials: Credentials, role: string, path: string) {
  const user = credentials.users.find((item) => item.roles?.includes(role) || item.role === role);
  expect(user, `${role} demo account`).toBeTruthy();
  const headers = { Origin: (process.env.ERPX_E2E_BASE_URL ?? "http://127.0.0.1:3102") };
  const login = await context.request.post("/api/auth/sign-in/email", { headers, data: { email: user!.email, password: credentials.password } });
  expect(login.ok(), await login.text()).toBe(true);
  const endpoint = "/api/v1/holdings/simulation/assignments/active";
  const active = (await (await context.request.get(endpoint)).json()).data;
  const assignment = active.options.find((item: { role: string }) => item.role === role);
  expect(assignment, `${role} active assignment`).toBeTruthy();
  const selected = await context.request.post(endpoint, { headers, data: { assignmentId: assignment.id, revision: active.revision } });
  expect(selected.ok(), await selected.text()).toBe(true);
  await page.goto(path, { waitUntil: "commit" });
  await expect(page.locator("[data-work-stale]")).toHaveAttribute("data-work-stale", "false");
}

test("isolated demo exposes a scoped Customer 360 timeline to independent Sales roles", async ({ browser }, testInfo) => {
  const run = process.env.ERPX_DEMO_RUN;
  test.skip(!run, "Set ERPX_DEMO_RUN to an exact prepared browser-simulation run.");
  test.setTimeout(120_000);
  const directory = resolve(`.runtime/browser/${run}`);
  const credentials = JSON.parse(await readFile(resolve(directory, "credentials.json"), "utf8")) as Credentials;
  const state = JSON.parse(await readFile(resolve(directory, "state.json"), "utf8")) as DemoState;
  const customerId = state.fixture?.customerId;
  const quotationId = state.fixture?.quotationId;
  expect(customerId).toBeTruthy();
  expect(quotationId).toBeTruthy();
  const operatorContext = await createEngineeringBrowserContext(browser,testInfo);
  const managerContext = await createEngineeringBrowserContext(browser,testInfo);
  const operatorPage = await operatorContext.newPage();
  const managerPage = await managerContext.newPage();
  const path = `/holdings/simulation/customers-sales/customers/directory/${customerId}`;
  const originalBody = "Demo customer asked for a copy of the updated service terms.";
  const correctedBody = "Demo customer requested the updated service terms by email.";
  const correctionReason = "Confirmed the wording in the customer mailbox.";
  const archiveReason = "Replaced by the signed customer correspondence.";
  try {
    await signInRole(operatorContext, operatorPage, credentials, "sales_operator", path);
    await expect(operatorPage.getByRole("heading", { name: "Simulation customer only", exact: true })).toBeVisible();
    const operatorWorkspace = operatorPage.getByTestId("record-detail-drawer").filter({ visible: true });
    await expect(operatorWorkspace).toHaveCount(1);
    const pendingWork = operatorWorkspace.locator("section").filter({ has: operatorPage.getByRole("heading", { name: "Pending work", exact: true }) });
    await expect(pendingWork).toHaveCount(1);
    const source = pendingWork.locator(`[data-slot="record-list"] a[href*="${quotationId}"]`);
    await expect(source).toHaveCount(1);
    await expect(source).toContainText(/Quotation ·/);
    await expect(source).toHaveAttribute("href", new RegExp(quotationId));
    await expect(operatorWorkspace.getByText(/payment beneficiary|contact details|ต้นทุน/i)).toHaveCount(0);
    await operatorWorkspace.getByRole("button", { name: "Record contact", exact: true }).click();
    const contact = operatorWorkspace.locator("[data-record-editor]");
    await expect(contact).toBeVisible();
    await expect(contact.getByRole("heading", { name: "Record contact", exact: true })).toBeVisible();
    const contactTime = contact.getByLabel("Contact time", { exact: true });
    await expect(contactTime).toBeEnabled();
    const instant = Math.floor(Date.now() / 60_000) * 60_000 - 120_000;
    const timeField = contact.locator(".preferred-date-time-field").filter({ has: operatorPage.getByLabel("Contact time", { exact: true }) });
    const timeZone = (await timeField.locator("small").first().textContent())?.match(/^Timezone: ([^\s·]+)/)?.[1];
    expect(timeZone, "the contact editor displays its resolved timezone").toBeTruthy();
    const parts = Object.fromEntries(new Intl.DateTimeFormat("en-GB", { timeZone, year: "numeric", month: "2-digit", day: "2-digit", hour: "2-digit", minute: "2-digit", hourCycle: "h23" }).formatToParts(instant).map(part => [part.type, part.value]));
    await contactTime.fill(`${parts.year}-${parts.month}-${parts.day}T${parts.hour}:${parts.minute}`);
    await expect(timeField.locator('input[name="occurred"]')).toHaveValue(new Date(instant).toISOString());
    await contact.getByRole("textbox", { name: "Contact summary", exact: true }).fill(originalBody);
    const [saved] = await Promise.all([
      operatorPage.waitForResponse((response) => response.url().includes("/api/v1/holdings/simulation/customer-notes") && response.request().method() === "POST"),
      contact.getByRole("button", { name: "Save internal note", exact: true }).click(),
    ]);
    expect(saved.ok(), await saved.text()).toBe(true);
    expect(saved.request().postDataJSON().occurredAt).toBe(instant);
    const noteId = (await saved.json()).data.id;
    await expect(contact).not.toBeVisible();
    await openFullProfileView(operatorPage, "History");
    await expect(operatorWorkspace.getByRole("heading", { name: "Customer history", exact: true })).toBeVisible();
    await expect(operatorWorkspace.getByRole("paragraph").filter({ hasText: originalBody }).filter({ visible: true })).toBeVisible();
    await operatorPage.reload();
    await expect(operatorWorkspace.getByRole("paragraph").filter({ hasText: originalBody }).filter({ visible: true })).toBeVisible();

    let noteCard = operatorWorkspace.locator("section").filter({ hasText: originalBody });
    await expect(noteCard).toHaveCount(1);
    await noteCard.getByText("Correct or archive", { exact: true }).click();
    let changeForm = noteCard.locator("form");
    await expect(changeForm).toHaveCount(1);
    await changeForm.locator('select[name="action"]').selectOption("correct");
    await changeForm.locator('textarea[name="body"]').fill(correctedBody);
    await changeForm.locator('textarea[name="reason"]').fill(correctionReason);
    const [corrected] = await Promise.all([
      operatorPage.waitForResponse((response) => response.url().includes("/api/v1/holdings/simulation/customer-notes") && response.request().method() === "POST"),
      changeForm.getByRole("button", { name: "Save with history", exact: true }).click(),
    ]);
    expect(corrected.ok(), await corrected.text()).toBe(true);
    expect(corrected.request().postDataJSON()).toMatchObject({ id: noteId, expectedVersion: 1, action: "correct", body: correctedBody, reason: correctionReason });
    await expect(noteCard.getByRole("heading").first()).toContainText("active");
    await expect(noteCard.locator(":scope > p").filter({ hasText: correctedBody })).toBeVisible();
    let noteHistory = noteCard.locator("details").filter({ hasText: "Change history" });
    await noteHistory.locator("summary").click();
    await expect(noteHistory).toContainText("Version 1 · create");
    await expect(noteHistory).toContainText("Initial standalone customer contact record");
    await expect(noteHistory).toContainText(originalBody);
    await expect(noteHistory).toContainText("Version 2 · correct");
    await expect(noteHistory).toContainText(correctionReason);
    await expect(noteHistory).toContainText(correctedBody);

    await noteCard.getByText("Correct or archive", { exact: true }).click();
    changeForm = noteCard.locator("form");
    await expect(changeForm).toHaveCount(1);
    await changeForm.locator('select[name="action"]').selectOption("archive");
    await changeForm.locator('textarea[name="reason"]').fill(archiveReason);
    const [archived] = await Promise.all([
      operatorPage.waitForResponse((response) => response.url().includes("/api/v1/holdings/simulation/customer-notes") && response.request().method() === "POST"),
      changeForm.getByRole("button", { name: "Save with history", exact: true }).click(),
    ]);
    expect(archived.ok(), await archived.text()).toBe(true);
    expect(archived.request().postDataJSON()).toMatchObject({ id: noteId, expectedVersion: 2, action: "archive", reason: archiveReason });
    await expect(noteCard.getByRole("heading").first()).toContainText("archived");
    await expect(noteCard.getByText("Correct or archive", { exact: true })).toHaveCount(0);
    await operatorPage.reload();
    noteCard = operatorWorkspace.locator("section").filter({ hasText: originalBody });
    await expect(noteCard).toHaveCount(1);
    await expect(noteCard.getByRole("heading").first()).toContainText("archived");
    noteHistory = noteCard.locator("details").filter({ hasText: "Change history" });
    await noteHistory.locator("summary").click();
    await expect(noteHistory).toContainText("Version 1 · create");
    await expect(noteHistory).toContainText(originalBody);
    await expect(noteHistory).toContainText("Version 2 · correct");
    await expect(noteHistory).toContainText(correctionReason);
    await expect(noteHistory).toContainText("Version 3 · archive");
    await expect(noteHistory).toContainText(archiveReason);
    await expect(noteHistory).toContainText(correctedBody);

    await openFullProfileView(operatorPage, "Summary");
    await source.click();
    const sourceDrawer = operatorPage.getByTestId("record-detail-drawer").filter({ visible: true });
    await expect(sourceDrawer).toHaveCount(1);
    await expect(sourceDrawer.getByRole("link", { name: "Open in new tab ↗", exact: true })).toHaveAttribute("href", new RegExp(quotationId));
    await expect(operatorPage).toHaveURL(new RegExp(quotationId));
    await signInRole(managerContext, managerPage, credentials, "sales_manager", path);
    const managerWorkspace = managerPage.getByTestId("record-detail-drawer").filter({ visible: true });
    await expect(managerWorkspace).toHaveCount(1);
    await expect(managerWorkspace.getByRole("heading", { name: "Pending work", exact: true })).toBeVisible();
    await expect(managerWorkspace.locator(`[data-slot="record-list"] a[href*="${quotationId}"]`)).toContainText(/Quotation ·/);
    await openFullProfileView(managerPage, "History");
    await expect(managerWorkspace.getByRole("heading", { name: "Customer history", exact: true })).toBeVisible();
    await expect(managerWorkspace.getByRole("paragraph").filter({ hasText: correctedBody }).filter({ visible: true })).toBeVisible();
    const managerNote = managerWorkspace.locator("section").filter({ hasText: originalBody });
    await expect(managerNote).toHaveCount(1);
    await managerNote.locator("details").filter({ hasText: "Change history" }).locator("summary").click();
    await expect(managerNote).toContainText("Version 1 · create");
    await expect(managerNote).toContainText(originalBody);
    await expect(managerNote).toContainText("Version 2 · correct");
    await expect(managerNote).toContainText(correctionReason);
    await expect(managerNote).toContainText("Version 3 · archive");
    await expect(managerNote).toContainText(archiveReason);
    const notes = await managerContext.request.get(`/api/v1/holdings/simulation/customer-notes?customerId=${customerId}`);
    expect(notes.ok(), await notes.text()).toBe(true);
    const noteData = (await notes.json()).data;
    expect(noteData.notes).toContainEqual(expect.objectContaining({ id: noteId, occurred_at: instant, status: "archived", version: 3, body: correctedBody }));
    expect(noteData.history.filter((event: { note_id: string }) => event.note_id === noteId)).toEqual(expect.arrayContaining([
      expect.objectContaining({ version: 1, action: "create", body: originalBody }),
      expect.objectContaining({ version: 2, action: "correct", body: correctedBody, reason: correctionReason }),
      expect.objectContaining({ version: 3, action: "archive", body: correctedBody, reason: archiveReason }),
    ]));
    await expect(managerWorkspace.getByText(/payment beneficiary|contact details|ต้นทุน/i)).toHaveCount(0);
    await testInfo.attach("customer-timeline-result", { contentType: "application/json", body: JSON.stringify({ run, customerId, roles: ["sales_operator", "sales_manager"], sourceLinkScoped: true, standaloneNoteCreatedCorrectedArchivedAndReloaded: true, noteVersions: [1, 2, 3], originalEvidenceRetained: true, restrictedFieldsHidden: true }, null, 2) });
    await managerPage.screenshot({ path: testInfo.outputPath("customer-timeline.png"), fullPage: true });
  } finally {
    await Promise.allSettled([operatorContext.close(), managerContext.close()]);
  }
});
