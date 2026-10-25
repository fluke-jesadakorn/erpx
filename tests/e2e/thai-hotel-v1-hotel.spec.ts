import { captureInvestorCheckpoint } from "../helpers/investor-capture";
import { createEngineeringBrowserContext } from './helpers/engineering-browser-evidence';
import { expect, test } from "@playwright/test";
import { execFileSync } from "node:child_process";
import { readFile } from "node:fs/promises";
import { resolve } from "node:path";
import { pinnedBunBinary } from "../../scripts/testing/runtime-tooling";
import { browserChildEnvironment } from '../../scripts/testing/runner-environment';
import { Pool } from "pg";
import { frontOrigin, frontWorkspacePath, signInFrontRole, type SimulatedFrontCredentials } from "../helpers/front-simulation-session";

type PmsRecord = { id: string; kind: string; version: number; data: Record<string, unknown> };
type DemoState = { sourceFingerprint?:string;release?:{id:string;fileManifestSha256:string};jobsPid?:number;fixture: { hospitality: { companyId: string; propertyId: string; roomIds: Record<string, string> } } };

test("H002 guest journey includes independent guest account correction, payment, checkout and Night Audit", async ({ browser }, testInfo) => {
  const run = process.env.ERPX_DEMO_RUN;
  expect(run, "The Thai Hotel mock runner must supply an exact prepared simulation run.").toMatch(/^[a-f0-9]{12}$/);
  if (!run) throw new Error("EXACT_BROWSER_SIMULATION_RUN_REQUIRED");
  test.setTimeout(360_000);
  const directory = resolve(`.runtime/browser/${run}`);
  const credentials = JSON.parse(await readFile(resolve(directory, "credentials.json"), "utf8")) as SimulatedFrontCredentials;
  const state = JSON.parse(await readFile(resolve(directory, "state.json"), "utf8")) as DemoState;
  const companyId = state.fixture.hospitality.companyId;
  const propertyId = state.fixture.hospitality.propertyId;
  const pmsPath = (section: string, query = "") => `${frontWorkspacePath(companyId, section)}${query}`;
  const pmsUrl = (section: string, query = "") => `${frontOrigin("pms")}${pmsPath(section, query)}`;
  const pmsCommandPath = `/api/v1/front/riverside-lantern/${companyId}/pms`;
  const pmsApi = `/api/v1/holdings/riverside-lantern/pms?property=${propertyId}`;
  const rootOrigin = process.env.ERPX_E2E_BASE_URL ?? "http://127.0.0.1:3100";
  const guestName = `Fictional E2E Guest ${run} ${testInfo.project.name}`;
  const frontDesk = await createEngineeringBrowserContext(browser,testInfo);
  const housekeeping = await createEngineeringBrowserContext(browser,testInfo);
  const inspector = await createEngineeringBrowserContext(browser,testInfo);
  const cashier = await createEngineeringBrowserContext(browser,testInfo);
  const manager = await createEngineeringBrowserContext(browser,testInfo);
  const auditor = await createEngineeringBrowserContext(browser,testInfo);
  const posCashier = await createEngineeringBrowserContext(browser,testInfo);
  [frontDesk, housekeeping, inspector, cashier, manager, auditor, posCashier].forEach((context) => context.setDefaultTimeout(15_000));
  const manifest = JSON.parse(await readFile(resolve(directory, "databases.json"), "utf8")) as { databases: { H002_DB: string } };
  const databaseUrl = new URL(manifest.databases.H002_DB);
  expect(databaseUrl.hostname).toBe("127.0.0.1");
  expect(databaseUrl.port).toBe("5432");
  expect(databaseUrl.pathname).toBe(`/erpx_browser_${run}_h002_db`);
  const db = new Pool({ connectionString: databaseUrl.href, max: 1 });
  let jobsPaused=false;
  const workerControl=(action:'pause-jobs'|'resume-jobs')=>{
    const result=JSON.parse(execFileSync(pinnedBunBinary(resolve('.')),['--env-file=/dev/null','scripts/local/run-server-script.ts','scripts/local/browser-simulation.ts',action,run],{cwd:resolve('.'),timeout:30_000,env:browserChildEnvironment(process.env,{ERPX_BROWSER_SIMULATION_PORT:new URL(rootOrigin).port,ERPX_BROWSER_SIMULATION_ATTACHMENTS:'local',ERPX_BROWSER_SIMULATION_JOBS:'1'})}).toString());
    expect(result.run).toBe(run);expect(result.jobsPaused).toBe(action==='pause-jobs');jobsPaused=action==='pause-jobs';return result;
  };
  try {
    const frontPage = await frontDesk.newPage();
    const reservations = pmsPath(`reservations?property=${propertyId}`);
    await signInFrontRole(frontDesk, frontPage, credentials, "pms_frontdesk", "pms", companyId, reservations);
    await frontPage.locator("summary").filter({ hasText: /^More\s*···$/ }).click();
    const currencyPicker = frontPage.getByRole("button", { name: "Choose display currency", exact: true });
    await currencyPicker.click();
    await frontPage.getByRole("textbox", { name: "Search currency", exact: true }).fill("THB");
    const currencySaved = frontPage.waitForResponse(response => response.request().method() === "POST" && response.url().endsWith("/money/preference"));
    await frontPage.getByRole("option", { name: /^THB\b/ }).click();
    const currencyResult = await currencySaved;
    expect(currencyResult.ok(), `${currencyResult.status()}: ${JSON.stringify((await currencyResult.json()).error ?? null)}`).toBe(true);
    await expect(currencyPicker).toContainText("THB");
    await expect(currencyPicker).toHaveAttribute("aria-expanded", "false");
    await frontPage.locator("summary").filter({ hasText: /^More\s*···$/ }).click();
    const frontMoney = `${frontOrigin("pms")}/api/v1/front/riverside-lantern/${companyId}/pms/money`;
    const moneyContext = await frontDesk.request.get(frontMoney, { headers: { "x-erpx-front-location": "holding" } });
    expect(moneyContext.status()).toBe(200);
    expect((await moneyContext.json()).data).toMatchObject({ companyId, displayCurrency: "THB", canConfirmRate: false });
    expect(moneyContext.headers()["cache-control"]).toBe("private, no-store");
    const wrongCompany = await frontDesk.request.get(`${frontOrigin("pms")}/api/v1/front/riverside-lantern/wrong-company/pms/money`, { headers: { "x-erpx-front-location": "holding" } });
    expect(wrongCompany.status()).toBe(404);
    expect((await wrongCompany.json()).error.code).toBe("FRONT_NOT_FOUND");
    const missingContext = await frontDesk.request.post(`${frontMoney}/rates`, { headers: { Origin: frontOrigin("pms"), "x-erpx-front-location": "holding", "Idempotency-Key": crypto.randomUUID() }, data: {} });
    expect(missingContext.status()).toBe(409);
    expect((await missingContext.json()).error.code).toBe("WORK_CONTEXT_CHANGED");
    const currencyHeaders = await currencyResult.request().allHeaders();
    const moneyContextHeaders = Object.fromEntries(Object.entries(currencyHeaders).filter(([name]) => name.startsWith("x-erpx-context-") || name === "x-erpx-front-location"));
    const forbiddenRate = await frontDesk.request.post(`${frontMoney}/rates`, { headers: { ...moneyContextHeaders, Origin: frontOrigin("pms"), "Idempotency-Key": crypto.randomUUID() }, data: {} });
    expect(forbiddenRate.status()).toBe(403);
    expect((await forbiddenRate.json()).error.code).toBe("FRONT_COMMAND_FORBIDDEN");
    const anonymousMoney = await createEngineeringBrowserContext(browser,testInfo);
    try {
      expect((await anonymousMoney.request.get(frontMoney)).status()).toBe(401);
    } finally {
      await anonymousMoney.close();
    }
    const cashierPage = await cashier.newPage();
    await signInFrontRole(cashier, cashierPage, credentials, "pms_cashier", "pms", companyId, pmsPath(`guest-accounts?property=${propertyId}`));
    const managerPage = await manager.newPage();
    await signInFrontRole(manager, managerPage, credentials, "pms_manager", "pms", companyId, pmsPath(`guest-accounts?property=${propertyId}&queue=corrections`));
    const auditorPage = await auditor.newPage();
    await signInFrontRole(auditor, auditorPage, credentials, "pms_night_auditor", "pms", companyId, pmsPath(`night-audit?property=${propertyId}`));
    const posPage = await posCashier.newPage();
    await signInFrontRole(posCashier, posPage, credentials, "pos_cashier", "pos", companyId, frontWorkspacePath(companyId, "?tab=shifts"));

    const sendPms = async (page: typeof frontPage, button: string, endpoint = pmsCommandPath) => {
      const response = page.waitForResponse((row) => row.request().method() === "POST" && row.url().endsWith(endpoint));
      await page.getByRole("button", { name: button, exact: true }).click();
      const result = await response;
      expect(result.ok(), `${button}: ${await result.text()}`).toBe(true);
      return result.json() as Promise<{ data: { id?: string; [key: string]: unknown } }>;
    };
    await frontPage.goto(pmsUrl(`reservations?property=${propertyId}`), { waitUntil: "domcontentloaded" });
    await expect(frontPage.getByRole("heading", { name: "Reservations", exact: true })).toBeVisible();
    if (testInfo.project.name === "mobile") {
      const headerWidth = await frontPage.locator('[data-front-daily="pms"] header > div').first().evaluate((node) => node.getBoundingClientRect().width);
      expect(headerWidth).toBeGreaterThan(300);
    }
    const initial = (await (await frontDesk.request.get(`/api/v1/holdings/riverside-lantern/pms?property=${propertyId}`)).json()).data;
    const today = String(initial.snapshot.property.businessDate);
    const tomorrow = new Date(`${today}T00:00:00Z`);
    tomorrow.setUTCDate(tomorrow.getUTCDate() + 1);
    const checkout = tomorrow.toISOString().slice(0, 10);
    await frontPage.getByLabel("Check-in", { exact: true }).fill(today);
    await frontPage.getByLabel("Check-out", { exact: true }).fill(checkout);
    await frontPage.getByRole("button", { name: "Search availability", exact: true }).click();
    const standardOffer = frontPage.getByRole("heading", { name: "Standard", exact: true });
    await expect(standardOffer).toBeVisible();
    await standardOffer.locator("..").locator("..").getByRole("button", { name: "Add rooms", exact: true }).click();
    await frontPage.getByRole("button", { name: "Continue to guest details", exact: true }).click();
    await frontPage.getByLabel("Full name", { exact: true }).fill(guestName);
    await frontPage.getByLabel("Email", { exact: true }).fill(`hotel-${run}@example.test`);
    await frontPage.getByLabel("Phone", { exact: true }).fill("+66 2000 0000");
    await frontPage.locator("label").filter({ hasText: "Booking source" }).locator("select").selectOption("phone");
    await frontPage.getByRole("button", { name: "Review price & policy", exact: true }).click();
    await expect(frontPage.getByRole("heading", { name: "Review your stay", exact: true })).toBeVisible();
    await expect(frontPage.getByText(/Stay total/)).toBeVisible();
    const confirmReservation = frontPage.getByRole("button", { name: "Confirm reservation", exact: true });
    await expect(confirmReservation).toHaveAttribute("data-client-managed-ready", "true");
    await frontPage.getByRole("checkbox", { name: "I have reviewed the dates, full price and cancellation terms.", exact: true }).check();
    const bookingResponse = frontPage.waitForResponse((response) => response.request().method() === "POST" && response.url().endsWith(pmsCommandPath));
    await confirmReservation.click();
    const bookingResult = await bookingResponse;
    expect(bookingResult.ok(), `${bookingResult.status()}: ${JSON.stringify((await bookingResult.json()).error ?? null)}`).toBe(true);
    await expect(frontPage.getByRole("status").filter({ hasText: "reservation is now available in the list below" })).toBeVisible();

    const created = (await (await frontDesk.request.get(`/api/v1/holdings/riverside-lantern/pms?property=${propertyId}`)).json()).data;
    const guest = created.snapshot.records.find((record: PmsRecord) => record.kind === "guest" && record.data.name === guestName) as PmsRecord | undefined;
    const reservation = created.snapshot.records.find((record: PmsRecord) => record.kind === "reservation" && record.data.guestId === guest?.id) as PmsRecord | undefined;
    const stay = created.snapshot.records.find((record: PmsRecord) => record.kind === "stay" && record.data.reservationId === reservation?.id) as PmsRecord | undefined;
    expect(guest).toBeTruthy();
    expect(reservation?.data).toMatchObject({ state: "confirmed", source: "phone" });
    expect(stay?.data).toMatchObject({ state: "reserved", arrival: today, departure: checkout });
    const prices = stay?.data.prices;
    expect(Array.isArray(prices)).toBe(true);
    if (!Array.isArray(prices)) throw new Error("STAY_PRICE_LINES_REQUIRED");
    expect(prices.reduce((total: number, price: { total: number }) => total + price.total, 0)).toBeGreaterThan(0);

    const housekeepingPage = await housekeeping.newPage();
    await signInFrontRole(housekeeping, housekeepingPage, credentials, "pms_housekeeping", "pms", companyId, pmsPath(`housekeeping?property=${propertyId}`));
    await housekeepingPage.goto(pmsUrl(`housekeeping?property=${propertyId}`), { waitUntil: "domcontentloaded" });
    await housekeepingPage.locator("summary").filter({ hasText: /^Create housekeeping task$/ }).click();
    const createTaskSummary = housekeepingPage.locator("summary").filter({ hasText: /^Create housekeeping task · Room 103$/ });
    await expect(createTaskSummary).toHaveCount(1);
    await createTaskSummary.click();
    const createTaskDetails = createTaskSummary.locator("..");
    await expect(createTaskDetails.getByRole("button", { name: "Create housekeeping task · Room 103", exact: true })).toHaveAttribute("data-client-managed-ready", "true");
    await createTaskDetails.locator("select").selectOption("arrival");
    await createTaskDetails.locator('input[type="date"]').fill(today);
    const taskResponse = housekeepingPage.waitForResponse((response) => response.request().method() === "POST" && response.url().endsWith(pmsCommandPath));
    await createTaskDetails.getByRole("button", { name: /Create housekeeping task/ }).click();
    const taskResult = await taskResponse;
    expect(taskResult.ok(), await taskResult.text()).toBe(true);
    const taskId = String((await taskResult.json()).data.id);

    const inspectorPage = await inspector.newPage();
    await signInFrontRole(inspector, inspectorPage, credentials, "pms_supervisor", "pms", companyId, pmsPath(`housekeeping?property=${propertyId}`));
    await inspectorPage.goto(pmsUrl(`housekeeping?property=${propertyId}`), { waitUntil: "domcontentloaded" });
    const taskHeading = inspectorPage.getByRole("heading", { name: "Room 103 · Before arrival", exact: true });
    const taskCard = taskHeading.locator("..").locator("..");
    const assignDetails = taskCard.locator("details").filter({ hasText: "Assign task" });
    await assignDetails.locator("summary").click();
    const housekeeper = credentials.users.find((item) => item.role === "pms_housekeeping" && item.email.endsWith("@riverside-lantern.example"));
    expect(housekeeper).toBeTruthy();
    await assignDetails.locator('select[name="assigneeId"]').selectOption(housekeeper!.userId);
    const assignResponse = inspectorPage.waitForResponse((response) => response.request().method() === "POST" && response.url().endsWith(pmsCommandPath));
    await assignDetails.getByRole("button", { name: "Assign task", exact: true }).click();
    expect((await assignResponse).ok()).toBe(true);

    await housekeepingPage.goto(pmsUrl(`housekeeping?property=${propertyId}`), { waitUntil: "domcontentloaded" });
    const workingHeading = housekeepingPage.getByRole("heading", { name: "Room 103 · Before arrival", exact: true });
    const workingCard = workingHeading.locator("..").locator("..");
    const startDetails = workingCard.locator("details").filter({ hasText: "Start task" });
    await startDetails.locator("summary").click();
    const startResponse = housekeepingPage.waitForResponse((response) => response.request().method() === "POST" && response.url().endsWith(pmsCommandPath));
    await startDetails.getByRole("button", { name: "Start task", exact: true }).click();
    expect((await startResponse).ok()).toBe(true);
    const completeDetails = workingCard.locator("details").filter({ hasText: "Mark task complete" });
    await completeDetails.locator("summary").click();
    const completeDrawer = housekeepingPage.getByRole("dialog", { name: "Mark task complete", exact: true });
    await expect(completeDrawer).toBeVisible();
    const completeForm = completeDrawer.locator("form");
    await expect(completeForm).toHaveCount(1);
    await expect(completeForm.getByRole("button", { name: "Mark task complete", exact: true })).toHaveAttribute("data-client-managed-ready", "true");
    const checklist = completeForm.locator('input[type="checkbox"][name^="check_"]');
    await expect(checklist).toHaveCount(5);
    for (const checkbox of await checklist.all()) await checkbox.check();
    await completeForm.getByLabel("Notes", { exact: true }).fill("Fictional room preparation completed and checked against the arrival list.");
    const completeResponse = housekeepingPage.waitForResponse((response) => response.request().method() === "POST" && response.url().endsWith(pmsCommandPath));
    await completeForm.getByRole("button", { name: "Mark task complete", exact: true }).click();
    expect((await completeResponse).ok()).toBe(true);

    await inspectorPage.reload({ waitUntil: "domcontentloaded" });
    const cleanHeading = inspectorPage.getByRole("heading", { name: "Room 103 · Before arrival", exact: true });
    const cleanCard = cleanHeading.locator("..").locator("..");
    const inspectDetails = cleanCard.locator("details").filter({ hasText: "Inspect task" });
    await inspectDetails.locator("summary").click();
    const inspectResponse = inspectorPage.waitForResponse((response) => response.request().method() === "POST" && response.url().endsWith(pmsCommandPath));
    await inspectDetails.getByRole("button", { name: "Inspect task", exact: true }).click();
    expect((await inspectResponse).ok()).toBe(true);
    await inspectorPage.reload({ waitUntil: "domcontentloaded" });
    await expect(inspectorPage.getByRole("heading", { name: "Room 103 · Before arrival", exact: true })).toHaveCount(0);
    const inspected = (await (await inspector.request.get(`/api/v1/holdings/riverside-lantern/pms?property=${propertyId}`)).json()).data;
    expect(inspected.snapshot.records.find((record: PmsRecord) => record.kind === "room" && record.id === state.fixture.hospitality.roomIds["103"])?.data.cleanliness).toBe("inspected");

    await frontPage.goto(pmsUrl(`front-desk?property=${propertyId}`), { waitUntil: "domcontentloaded" });
    await frontPage.reload({ waitUntil: "domcontentloaded" });
    await expect(frontPage.locator("[data-work-stale]")).toHaveAttribute("data-work-stale", "false");
    const frontOffice = frontPage.getByRole("main").locator("section:visible").filter({ has: frontPage.getByRole("heading", { name: "Front desk", exact: true }).filter({ visible: true }) }).first();
    const guestHeading = frontOffice.getByRole("heading", { name: guestName, exact: true }).filter({ visible: true }).first();
    await expect(guestHeading).toBeVisible();
    const stayCard = guestHeading.locator("xpath=../..");
    const stayCardText = await stayCard.innerText();
    expect(stayCardText, "Reserved stay should offer assignment and check-in actions").toContain("Assign room");
    const assignRoomDetails = stayCard.locator("details").filter({ hasText: "Assign room" });
    await assignRoomDetails.locator("summary").click();
    await assignRoomDetails.locator('select[name="roomId"]').selectOption(state.fixture.hospitality.roomIds["103"]);
    const roomResponse = frontPage.waitForResponse((response) => response.request().method() === "POST" && response.url().endsWith(pmsCommandPath));
    await assignRoomDetails.getByRole("button", { name: "Assign room", exact: true }).click();
    expect((await roomResponse).ok()).toBe(true);
    await frontPage.reload({ waitUntil: "domcontentloaded" });
    const assignedHeading = frontOffice.getByRole("heading", { name: guestName, exact: true }).filter({ visible: true }).first();
    const assignedCard = assignedHeading.locator("xpath=../..");
    const checkInDetails = assignedCard.locator("details").filter({ hasText: "Check in" });
    await checkInDetails.locator("summary").click();
    const checkInResponse = frontPage.waitForResponse((response) => response.request().method() === "POST" && response.url().endsWith(pmsCommandPath));
    await checkInDetails.getByRole("button", { name: "Check in", exact: true }).click();
    expect((await checkInResponse).ok()).toBe(true);
    await frontPage.reload({ waitUntil: "domcontentloaded" });
    await expect(frontPage.locator("article").filter({ hasText: guestName }).first()).toContainText("In house");
    if (process.env.ERPX_INVESTOR_CAPTURE === "1" && testInfo.project.name === "mobile") {
      await guestHeading.evaluate(element => {
        const headerBottom = document.querySelector("header")?.getBoundingClientRect().bottom ?? 0;
        window.scrollBy({ top: element.getBoundingClientRect().top - headerBottom - 24, behavior: "instant" });
      });
      await expect.poll(() => guestHeading.evaluate(element => element.getBoundingClientRect().top >= (document.querySelector("header")?.getBoundingClientRect().bottom ?? 0) + 8)).toBe(true);
    }
    const investorCheckin = await captureInvestorCheckpoint(frontPage, "hotel-checkin", guestHeading);
    if (testInfo.project.name === "mobile") {
      const viewportWidth = await frontPage.evaluate(() => window.innerWidth);
      const documentWidth = await frontPage.evaluate(() => document.documentElement.scrollWidth);
      expect(documentWidth).toBeLessThanOrEqual(viewportWidth);
      await testInfo.attach("H002 mobile front desk", { body: investorCheckin ?? await frontPage.screenshot({ fullPage: true }), contentType: "image/png" });
    }

    const persisted = (await (await frontDesk.request.get(pmsApi)).json()).data;
    const finalGuest = persisted.snapshot.records.find((record: PmsRecord) => record.kind === "guest" && record.data.name === guestName) as PmsRecord | undefined;
    const finalReservation = persisted.snapshot.records.find((record: PmsRecord) => record.kind === "reservation" && record.data.guestId === finalGuest?.id) as PmsRecord | undefined;
    const finalStay = persisted.snapshot.records.find((record: PmsRecord) => record.kind === "stay" && record.data.reservationId === finalReservation?.id) as PmsRecord | undefined;
    expect(finalStay?.data).toMatchObject({ state: "in_house", roomId: state.fixture.hospitality.roomIds["103"] });
    const guestAccount = persisted.snapshot.records.find((record: PmsRecord) => record.kind === "guest_account" && record.id === finalStay?.data.guestAccountId) as PmsRecord | undefined;
    expect(guestAccount?.data.state).toBe("open");
    const persistedTask = persisted.snapshot.records.find((record: PmsRecord) => record.kind === "task" && record.id === taskId) as PmsRecord | undefined;
    expect(persistedTask?.data.state).toBe("done");
    expect(persisted.snapshot.records.find((record: PmsRecord) => record.kind === "room" && record.id === state.fixture.hospitality.roomIds["103"])?.data.cleanliness).toBe("inspected");

    const guestAccountId = String(guestAccount!.id);
    await frontPage.goto(pmsUrl(`front-desk?property=${propertyId}`), { waitUntil: "domcontentloaded" });
    const getFrontDeskSection = () => frontPage.getByRole("main").locator("section:visible").filter({ has: frontPage.getByRole("heading", { name: "Front desk", exact: true }).filter({ visible: true }) }).first();
    const frontDeskStayCard = () => getFrontDeskSection().getByRole("heading", { name: guestName, exact: true }).filter({ visible: true }).first().locator("xpath=../..");
    const roomCharge = frontDeskStayCard().locator("details").filter({ hasText: "Post room charge before check-out" });
    await roomCharge.locator("summary").click();
    await sendPms(frontPage, "Post room charge before check-out");

    await cashierPage.goto(pmsUrl(`cashier?property=${propertyId}`), { waitUntil: "domcontentloaded" });
    let cashierSnapshot = (await (await cashier.request.get(pmsApi)).json()).data.snapshot;
    const cashierUserId = credentials.users.find((item) => item.role === "pms_cashier" && item.email.endsWith("@riverside-lantern.example"))!.userId;
    let cashierShift = cashierSnapshot.records.find((record: PmsRecord) => record.kind === "shift" && record.data.userId === cashierUserId && record.data.state === "open");
    if (!cashierShift) {
      const open = cashierPage.locator("details").filter({ hasText: "Open cash shift" }).first();
      await open.locator("summary").click();
      await open.locator('input[name="name"]').fill(`Hotel acceptance ${run}`);
      await open.locator('input[name="opening"]').fill("0");
      await sendPms(cashierPage, "Open cash shift");
      cashierSnapshot = (await (await cashier.request.get(pmsApi)).json()).data.snapshot;
      cashierShift = cashierSnapshot.records.find((record: PmsRecord) => record.kind === "shift" && record.data.userId === cashierUserId && record.data.state === "open");
    }
    expect(cashierShift).toBeTruthy();
    const guestAccountBeforeCheckout = cashierSnapshot.records.find((record: PmsRecord) => record.kind === "guest_account" && record.id === guestAccountId);
    expect(guestAccountBeforeCheckout.data.due).toBeGreaterThan(0);
    await cashierPage.goto(pmsUrl(`guest-accounts?property=${propertyId}&guestAccount=${guestAccountId}`), { waitUntil: "domcontentloaded" });
    const receive = cashierPage.locator("details").filter({ hasText: "Receive payment / advance" }).first();
    await receive.locator("summary").click();
    const receiveDrawer = cashierPage.getByRole("dialog", { name: "Receive payment / advance", exact: true });
    await expect(receiveDrawer).toBeVisible();
    const receiveForm = receiveDrawer.locator("form");
    await expect(receiveForm).toHaveCount(1);
    await expect(receiveForm.getByRole("button", { name: "Receive payment / advance", exact: true })).toHaveAttribute("data-client-managed-ready", "true");
    await receiveForm.locator('input[name="amount"]').fill((Number(guestAccountBeforeCheckout.data.due) / 100).toFixed(2));
    await receiveForm.locator('select[name="method"]').selectOption("cash");
    await receiveForm.locator('select[name="shiftId"]').selectOption(String(cashierShift!.id));
    await sendPms(cashierPage, "Receive payment / advance");
    const paidBeforeCheckout = (await (await cashier.request.get(pmsApi)).json()).data;
    expect(paidBeforeCheckout.snapshot.records.find((record: PmsRecord) => record.kind === "guest_account" && record.id === guestAccountId)?.data.due).toBe(0);

    await frontPage.goto(pmsUrl(`front-desk?property=${propertyId}`), { waitUntil: "domcontentloaded" });
    const checkoutForm = frontDeskStayCard().locator("details").filter({ hasText: "Check out" });
    await checkoutForm.locator("summary").click();
    await sendPms(frontPage, "Check out");
    const checkedOut = (await (await frontDesk.request.get(pmsApi)).json()).data;
    const stayAfterCheckout = checkedOut.snapshot.records.find((record: PmsRecord) => record.kind === "stay" && record.data.guestAccountId === guestAccountId);
    expect(stayAfterCheckout?.data.state).toBe("checked_out");

    const incorrectDescription = `Fictional minibar correction ${run}`;
    await cashierPage.goto(pmsUrl(`guest-accounts?property=${propertyId}&guestAccount=${guestAccountId}`), { waitUntil: "domcontentloaded" });
    const addCharge = cashierPage.locator("details").filter({ hasText: "Add service charge" }).first();
    await addCharge.locator("summary").click();
    await addCharge.locator('input[name="description"]').fill(incorrectDescription);
    await addCharge.locator('input[name="price"]').fill("100.00");
    await sendPms(cashierPage, "Add service charge");
    await cashierPage.reload({ waitUntil: "domcontentloaded" });
    const incorrectRow = cashierPage.locator("tr").filter({ hasText: incorrectDescription });
    await expect(incorrectRow).toBeVisible();
    const correctionReason = `Fictional minibar line was entered by mistake; request independent reversal ${run}`;
    const correctionForm = incorrectRow.locator("details").filter({ hasText: "Request reversal" });
    await correctionForm.locator("summary").click();
    await correctionForm.locator('input[name="reason"]').fill(correctionReason);
    const requested = await sendPms(cashierPage, "Request reversal");
    const correctionId = String(requested.data.id);
    expect(correctionId).toMatch(/^pms-correction-/);

    await auditorPage.goto(pmsUrl(`night-audit?property=${propertyId}`), { waitUntil: "domcontentloaded" });
    await expect(auditorPage.getByText(/hotel guest account correction\(s\) awaiting independent review/)).toBeVisible();
    await expect(auditorPage.getByRole("button", { name: "Start day close", exact: true })).toHaveCount(0);
    const blockedControls = (await (await auditor.request.get(`${pmsApi}&dayControls=1`)).json()).data.dayControls;
    expect(blockedControls.blockers.some((item: string) => item.includes("hotel guest account correction(s) awaiting independent review"))).toBe(true);

    await managerPage.goto(`${rootOrigin}/holdings/riverside-lantern/pms/guest-accounts?property=${propertyId}&queue=corrections`, { waitUntil: "domcontentloaded" });
    await expect(managerPage.getByRole("heading", { name: "Guest Account charge corrections", exact: true })).toBeVisible();
    const reviewCard = managerPage.locator(`article[data-pms-record="${correctionId}"]`).filter({ hasText: correctionReason, visible: true });
    await expect(reviewCard).toHaveCount(1);
    await expect(reviewCard).toBeVisible();
    const reviewForm = reviewCard.locator("details").filter({ hasText: "Review correction" });
    await reviewForm.locator("summary").click();
    await reviewForm.locator('select[name="action"]').selectOption("approve");
    await reviewForm.locator('input[name="reason"]').fill(`Independent manager review of correction ${run}`);
    await sendPms(managerPage, "Review correction", "/api/v1/holdings/riverside-lantern/pms");

    const correction = await db.query("SELECT status,requested_by,reviewed_by,review_reason,reversal_entry_id FROM pms_charge_corrections WHERE id=$1", [correctionId]);
    expect(correction.rows).toHaveLength(1);
    expect(correction.rows[0].status).toBe("approved");
    expect(correction.rows[0].requested_by).not.toBe(correction.rows[0].reviewed_by);
    expect(correction.rows[0].reversal_entry_id).toBeTruthy();
    const reversal = await db.query("SELECT data_json::jsonb->>'originalId' original_id,data_json::jsonb->>'total' total FROM pms_records WHERE id=$1 AND property_id=$2", [correction.rows[0].reversal_entry_id, propertyId]);
    expect(reversal.rows).toEqual([{ original_id: (await db.query("SELECT entry_id FROM pms_charge_corrections WHERE id=$1", [correctionId])).rows[0].entry_id, total: "-10700" }]);
    const events = await db.query("SELECT action,actor_id FROM pms_charge_correction_events WHERE correction_id=$1 ORDER BY correction_version", [correctionId]);
    expect(events.rows.map((row) => row.action)).toEqual(["requested", "approved"]);
    expect(events.rows[0].actor_id).not.toBe(events.rows[1].actor_id);
    const correctionPostings = await db.query("SELECT count(*)::int count FROM pms_postings WHERE property_id=$1 AND record_id IN ($2,$3)", [propertyId, correction.rows[0].reversal_entry_id, reversal.rows[0].original_id]);
    expect(correctionPostings.rows[0].count).toBe(2);

    const afterCorrection = (await (await cashier.request.get(pmsApi)).json()).data;
    const guestAccountAfterCorrection = afterCorrection.snapshot.records.find((record: PmsRecord) => record.kind === "guest_account" && record.id === guestAccountId);
    expect(guestAccountAfterCorrection.data.due).toBe(0);
    expect(guestAccountAfterCorrection.data.advance).toBe(0);

    await cashierPage.goto(pmsUrl(`guest-accounts?property=${propertyId}&guestAccount=${guestAccountId}`), { waitUntil: "domcontentloaded" });
    const closeGuestAccount = cashierPage.locator("details").filter({ hasText: "Close guest account" }).first();
    await expect(closeGuestAccount.locator("summary")).toBeVisible();
    await expect(cashierPage.getByText("Balance due", { exact: true }).filter({ visible: true }).first()).toBeVisible();
    await captureInvestorCheckpoint(cashierPage, "hotel-guestAccount", cashierPage.getByRole("heading", { name: guestName, exact: true }).filter({ visible: true }).first());
    await closeGuestAccount.locator("summary").click();
    await sendPms(cashierPage, "Close guest account");
    const closed = (await (await cashier.request.get(pmsApi)).json()).data;
    const closedGuestAccount = closed.snapshot.records.find((record: PmsRecord) => record.kind === "guest_account" && record.id === guestAccountId);
    expect(closedGuestAccount?.data).toMatchObject({ state: "closed", due: 0 });
    expect(closedGuestAccount?.data.invoiceId).toBeTruthy();

    await cashierPage.goto(pmsUrl(`cashier?property=${propertyId}`), { waitUntil: "domcontentloaded" });
    const openHotelShift = closed.snapshot.records.find((record: PmsRecord) => record.kind === "shift" && record.data.userId === cashierUserId && record.data.state === "open");
    expect(openHotelShift).toBeTruthy();
    const closeShift = cashierPage.locator("article").filter({ hasText: String(openHotelShift!.data.name) }).locator("details").filter({ hasText: "Close cash shift" });
    await closeShift.locator("summary").click();
    await closeShift.locator('input[name="counted"]').fill((Number(openHotelShift!.data.cash) / 100).toFixed(2));
    await closeShift.locator('textarea[name="handover"]').fill(`Fictional H002 cashier handover ${run}`);
    await sendPms(cashierPage, "Close cash shift");

    const posApi = "/api/v1/holdings/riverside-lantern/pos?tab=shifts";
    const posShifts = (await (await posCashier.request.get(posApi)).json()).data.shifts as Array<{ id: string; state: string; register_name: string; cash_expected_minor: number }>;
    if (posShifts.some((shift) => shift.state === "open")) {
      await posPage.goto(`${frontOrigin("pos")}${frontWorkspacePath(companyId, "?tab=shifts")}`, { waitUntil: "domcontentloaded" });
      for (const shift of posShifts.filter((item) => item.state === "open")) {
        const cash = await db.query("SELECT COALESCE(SUM(p.amount_minor),0)::bigint amount FROM pos_payments p JOIN pos_sales s ON s.id=p.sale_id WHERE s.shift_id=$1 AND p.method='cash' AND p.state='succeeded'", [shift.id]);
        const counted = Number(shift.cash_expected_minor);
        expect(Number(cash.rows[0].amount)).toBeLessThanOrEqual(counted);
        const shiftCard = posPage.locator("article").filter({ has: posPage.getByRole("heading", { name: shift.register_name, exact: true }) }).filter({ has: posPage.getByRole("button", { name: "Close cash shift", exact: true }) });
        await expect(shiftCard).toHaveCount(1);
        await shiftCard.getByLabel("Enter counted total", { exact: true }).check();
        await shiftCard.getByLabel("Counted cash", { exact: true }).fill((counted / 100).toFixed(2));
        const closePos = posPage.waitForResponse((response) => response.request().method() === "POST" && response.url().endsWith("/pos/shift/close"));
        await shiftCard.getByRole("button", { name: "Close cash shift", exact: true }).click();
        expect((await closePos).ok()).toBe(true);
      }
    }

    await auditorPage.goto(pmsUrl(`night-audit?property=${propertyId}`), { waitUntil: "domcontentloaded" });
    const startDayClose = auditorPage.locator("details").filter({ hasText: "Start day close" }).first();
    await expect(startDayClose.locator("summary")).toBeVisible();
    await captureInvestorCheckpoint(auditorPage, "hotel-audit", startDayClose.locator("summary"));
    if(state.release)workerControl('pause-jobs');
    await startDayClose.locator("summary").click();
    await sendPms(auditorPage, "Start day close");
    const closing = (await (await auditor.request.get(pmsApi)).json()).data;
    expect(closing.snapshot.property.state).toBe("closing");
    expect(closing.snapshot.records.some((record: PmsRecord) => record.kind === "night" && record.data.state === "pending")).toBe(true);
    const latestJob=async()=>(await db.query('SELECT state,error_code FROM pms_jobs WHERE property_id=$1 ORDER BY updated_at DESC LIMIT 1',[propertyId])).rows[0];
    if(state.release){workerControl('resume-jobs');await expect.poll(async()=>(await latestJob())?.state,{timeout:30_000,intervals:[250,500,1000]}).toBe('completed');}
    const completed = state.release?{job:await latestJob(),property:(await db.query('SELECT state,business_date FROM pms_properties WHERE id=$1',[propertyId])).rows[0]}:JSON.parse(execFileSync(pinnedBunBinary(resolve(".")), ["--env-file=/dev/null", "scripts/local/run-server-script.ts", "scripts/fixtures/hospitality-drain-demo-job.ts", run], { cwd: resolve("."), timeout: 30_000 }).toString()) as { job: { state: string; error_code: string | null }; property: { state: string; business_date: string } };
    expect(completed).toMatchObject({ job: { state: "completed", error_code: null }, property: { state: "open" } });
    expect(completed.property.business_date).not.toBe(closing.snapshot.property.businessDate);
    await auditorPage.reload({waitUntil:'domcontentloaded'});
    await expect(auditorPage.getByText(`Business date ${completed.property.business_date} · THB`,{exact:true})).toBeVisible();
    await expect(auditorPage.getByText('This hotel is closing its business day. Ask the hotel administrator to finish the day close.',{exact:true})).toHaveCount(0);
    await auditorPage.screenshot({path:testInfo.outputPath('completed-business-date.png'),fullPage:true});
    if(state.release){const restarted=JSON.parse(await readFile(resolve(directory,'state.json'),'utf8')) as DemoState;expect(restarted.release).toEqual(state.release);expect(restarted.sourceFingerprint).toBe(state.sourceFingerprint);expect(restarted.jobsPid).not.toBe(state.jobsPid);await testInfo.attach('compiled-guest-journey-night-recovery',{body:JSON.stringify({sourceFingerprint:state.sourceFingerprint,release:state.release,firstWorkerPid:state.jobsPid,restartedWorkerPid:restarted.jobsPid,completed}),contentType:'application/json'});}

    const invoice = await db.query("SELECT total_minor,outstanding_minor,status,source_type,source_id FROM finance_invoices WHERE id=$1 AND company_id=$2", [closedGuestAccount!.data.invoiceId, companyId]);
    expect(invoice.rows).toHaveLength(1);
    expect(invoice.rows[0]).toMatchObject({ outstanding_minor: "0", status: "paid", source_type: "pms", source_id: guestAccountId });
    const journals = await db.query("SELECT count(*)::int count FROM (SELECT j.id FROM finance_journals j JOIN pms_postings pp ON pp.journal_id=j.id JOIN finance_journal_lines l ON l.journal_id=j.id WHERE pp.property_id=$1 GROUP BY j.id HAVING SUM(l.debit_minor)<>SUM(l.credit_minor)) unbalanced", [propertyId]);
    expect(journals.rows[0].count).toBe(0);
    const correctionReplay = await db.query("SELECT count(*)::int count FROM pms_postings WHERE property_id=$1 AND record_id IN ($2,$3)", [propertyId, reversal.rows[0].original_id, correction.rows[0].reversal_entry_id]);
    expect(correctionReplay.rows[0].count).toBe(2);
    const closedShifts = await db.query("SELECT data_json::jsonb->>'state' state,data_json::jsonb->>'cash' cash,data_json::jsonb->>'counted' counted,data_json::jsonb->>'variance' variance FROM pms_records WHERE property_id=$1 AND kind='shift' AND data_json::jsonb->>'userId'=$2", [propertyId, cashierUserId]);
    expect(closedShifts.rows.every((row) => row.state === "closed" && row.cash === row.counted && row.variance === "0")).toBe(true);
  } finally {
    if(jobsPaused)workerControl('resume-jobs');
    await Promise.all([frontDesk.close(), housekeeping.close(), inspector.close(), cashier.close(), manager.close(), auditor.close(), posCashier.close(), db.end()]);
  }
});
