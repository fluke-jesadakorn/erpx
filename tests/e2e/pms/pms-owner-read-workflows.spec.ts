import { expect, test, type BrowserContext, type Page, type TestInfo } from "@playwright/test";
import { readFile } from "node:fs/promises";
import { resolve } from "node:path";
import { createEngineeringBrowserContext } from "../../support/browser/engineering-browser-evidence";
import { selectFixtureAssignment } from "../../support/browser/select-fixture-assignment";
import {
  frontOrigin,
  frontWorkspacePath,
  signInFrontRole,
  type SimulatedFrontCredentials,
} from "../../support/helpers/front-simulation-session";
import { records, type Snapshot } from "../../../src/server/modules/pms/public/rules/pms-model";

type HospitalityRunState = {
  run: string;
  origin: string;
  port: number;
  ready: boolean;
  fixture: {
    companyId: string;
    hospitality: { companyId: string; propertyId: string };
  };
};

type OwnerWorkspace = {
  properties: Array<{ id: string; name: string }>;
  snapshot: (Snapshot & { listMeta?: { section: string; ids: string[] } }) | null;
  defaultDate: string;
  ledger: unknown;
  controlBalances: unknown;
};

type PreparedOwner = {
  context: BrowserContext;
  page: Page;
  state: HospitalityRunState;
  credentials: SimulatedFrontCredentials;
  origin: string;
  companyId: string;
  propertyId: string;
};

const runId = process.env.ERPX_DEMO_RUN ?? "";
const configuredOrigin = process.env.ERPX_E2E_BASE_URL ?? "http://127.0.0.1:3100";
const isolatedHospitality = /^[a-f0-9]{12}$/.test(runId) &&
  [3102, 3104].includes(Number(new URL(configuredOrigin).port));

async function prepareHospitalityOwner(browser: Parameters<typeof createEngineeringBrowserContext>[0], info: TestInfo): Promise<PreparedOwner> {
  expect(runId, "An exact disposable hospitality browser simulation is required").toMatch(/^[a-f0-9]{12}$/);
  const origin = configuredOrigin;
  const directory = resolve(".runtime/browser", runId);
  const [stateBytes, credentialsBytes] = await Promise.all([
    readFile(resolve(directory, "state.json"), "utf8"),
    readFile(resolve(directory, "credentials.json"), "utf8"),
  ]);
  const state = JSON.parse(stateBytes) as HospitalityRunState;
  const credentials = JSON.parse(credentialsBytes) as SimulatedFrontCredentials;
  const companyId = state.fixture.hospitality.companyId;
  const propertyId = state.fixture.hospitality.propertyId;
  const port = Number(new URL(origin).port);

  expect(state).toMatchObject({ run: runId, origin, port, ready: true });
  expect(companyId).not.toBe(state.fixture.companyId);
  expect(propertyId).toBeTruthy();

  const manager = credentials.users.find((user) =>
    user.role === "pms_manager" && user.email === "hotel-manager@riverside-lantern.example",
  );
  expect(manager, "The hospitality simulation's native PMS manager is required").toBeTruthy();

  const context = await createEngineeringBrowserContext(browser, info, { baseURL: origin });
  try {
    const login = await context.request.post(new URL("/api/auth/sign-in/email", origin).toString(), {
      headers: { Origin: origin },
      data: { email: manager!.email, password: credentials.password },
    });
    expect(login.status()).toBe(200);

    const companySelection = await context.request.post(
      `${origin}/api/v1/holdings/riverside-lantern/companies/active`,
      { headers: { Origin: origin }, data: { companyId } },
    );
    expect(companySelection.status()).toBe(200);

    const assignmentsResponse = await context.request.get(
      `${origin}/api/v1/holdings/riverside-lantern/assignments/active`,
    );
    expect(assignmentsResponse.status()).toBe(200);
    const assignments = (await assignmentsResponse.json()).data as {
      revision: number;
      options: Array<{ id: string; role: string; companyId: string; modules: string[] }>;
    };
    const assignment = assignments.options.find((option) =>
      option.role === "pms_manager" && option.companyId === companyId,
    );
    expect(assignment, "The fixture must provide an unprivileged PMS manager assignment").toBeTruthy();
    expect(assignment!.modules).toContain("pms");
    expect(assignment!.modules).not.toContain("commercial");

    await selectFixtureAssignment(context, companyId, assignment!.id, "riverside-lantern");
    const selectedResponse = await context.request.get(
      `${origin}/api/v1/holdings/riverside-lantern/assignments/active`,
    );
    expect(selectedResponse.status()).toBe(200);
    expect((await selectedResponse.json()).data).toMatchObject({
      valid: true,
      companyId,
      assignmentId: assignment!.id,
    });

    return {
      context,
      page: await context.newPage(),
      state,
      credentials,
      origin,
      companyId,
      propertyId,
    };
  } catch (error) {
    await context.close();
    throw error;
  }
}

async function readOwnerWorkspace(
  context: BrowserContext,
  origin: string,
  companyId: string,
  propertyId: string,
  selection: string,
) {
  const response = await context.request.get(
    `${origin}/api/v1/holdings/riverside-lantern/pms?property=${encodeURIComponent(propertyId)}&${selection}`,
  );
  expect(response.status()).toBe(200);
  const { data } = await response.json() as { data: OwnerWorkspace };
  expect(data.snapshot).not.toBeNull();
  expect(data.snapshot!.property).toMatchObject({ id: propertyId, companyId });
  expect(data.properties.map((property) => property.id)).toContain(propertyId);
  return data;
}

function watchForWrites(page: Page) {
  const mutations: string[] = [];
  page.on("request", (request) => {
    if (["POST", "PUT", "PATCH", "DELETE"].includes(request.method())) {
      mutations.push(`${request.method()} ${new URL(request.url()).pathname}`);
    }
  });
  return () => expect(mutations, "Owner read workflow must not submit writes or provider actions").toEqual([]);
}

test("pms/dashboard renders the selected hotel's live room status through its owner-scoped read", async ({ browser }, info) => {
  test.skip(!isolatedHospitality, "Requires the exact disposable hospitality simulation on port 3102 or 3104.");
  const fixture = await prepareHospitalityOwner(browser, info);
  try {
    await signInFrontRole(
      fixture.context,
      fixture.page,
      fixture.credentials,
      "pms_manager",
      "pms",
      fixture.companyId,
      frontWorkspacePath(fixture.companyId),
    );
    const assertNoWrites = watchForWrites(fixture.page);
    await fixture.page.goto(
      `${fixture.origin}/holdings/riverside-lantern/pms/dashboard?property=${encodeURIComponent(fixture.propertyId)}`,
    );
    const dashboardUrl = new URL(fixture.page.url());
    expect(dashboardUrl.origin).toBe(frontOrigin("pms"));
    expect(dashboardUrl.pathname).toBe(frontWorkspacePath(fixture.companyId));
    expect(dashboardUrl.searchParams.get("property")).toBe(fixture.propertyId);

    const ownerData = await readOwnerWorkspace(
      fixture.context,
      fixture.origin,
      fixture.companyId,
      fixture.propertyId,
      "view=dashboard",
    );
    const snapshot = ownerData.snapshot!;
    const expectedRooms = records(snapshot, "room").map((room) => room.data.number).sort();
    expect(expectedRooms.length).toBeGreaterThan(0);
    const workspace = fixture.page.locator('[data-source="live"][data-front-daily="pms"]');
    await expect(workspace).toBeVisible();
    await expect(fixture.page.getByRole("heading", { name: "Room status", exact: true })).toBeVisible();
    const visibleRooms = await fixture.page.locator("#pms-room-status a").evaluateAll((links) =>
      links.map((link) => link.querySelector("strong")?.textContent?.trim() ?? "").filter(Boolean).sort(),
    );
    expect(visibleRooms).toEqual(expectedRooms);

    const occupiedStay = records(snapshot, "stay").find((stay) => stay.data.state === "in_house");
    expect(occupiedStay, "The fixture includes an in-house stay to validate room state").toBeTruthy();
    const occupiedRoom = records(snapshot, "room").find((room) => room.id === occupiedStay!.data.roomId);
    expect(occupiedRoom).toBeTruthy();
    await expect(
      fixture.page.locator("#pms-room-status a").filter({ hasText: occupiedRoom!.data.number }),
    ).toContainText("Occupied");
    assertNoWrites();
  } finally {
    await fixture.context.close();
  }
});

test("pms/inventory opens the mounted minibar route with inventory section identity and scoped records", async ({ browser }, info) => {
  test.skip(!isolatedHospitality, "Requires the exact disposable hospitality simulation on port 3102 or 3104.");
  const fixture = await prepareHospitalityOwner(browser, info);
  try {
    const assertNoWrites = watchForWrites(fixture.page);
    await fixture.page.goto(
      `${fixture.origin}/holdings/riverside-lantern/pms/settings?property=${encodeURIComponent(fixture.propertyId)}`,
    );
    await expect(fixture.page.locator('[data-source="live"]')).toBeVisible();
    const inventoryLink = fixture.page.getByRole("link", { name: "Minibar & linen", exact: true });
    await expect(inventoryLink).toHaveAttribute(
      "href",
      `/holdings/riverside-lantern/pms/minibar?property=${encodeURIComponent(fixture.propertyId)}`,
    );
    await inventoryLink.click();
    await expect(fixture.page).toHaveURL(
      `${fixture.origin}/holdings/riverside-lantern/pms/minibar?property=${encodeURIComponent(fixture.propertyId)}`,
    );
    await expect(fixture.page.getByRole("link", { name: "Minibar & linen", exact: true })).toHaveAttribute("aria-current", "page");
    await expect(fixture.page.getByRole("heading", { name: "Minibar, supplies & linen", exact: true })).toBeVisible();

    const ownerData = await readOwnerWorkspace(
      fixture.context,
      fixture.origin,
      fixture.companyId,
      fixture.propertyId,
      "view=list&section=inventory",
    );
    const snapshot = ownerData.snapshot!;
    expect(snapshot.listMeta?.section).toBe("inventory");
    const visibleItems = records(snapshot, "item").filter((item) =>
      !snapshot.listMeta || snapshot.listMeta.ids.includes(item.id),
    );
    const lostItems = records(snapshot, "lost");
    const expectedIds = [...visibleItems, ...lostItems].map((record) => record.id).sort();
    const displayedIds = await fixture.page.locator("[data-pms-record]").evaluateAll((nodes) =>
      nodes.map((node) => node.getAttribute("data-pms-record") ?? "").filter(Boolean).sort(),
    );
    expect(displayedIds).toEqual(expectedIds);
    for (const item of visibleItems) {
      await expect(fixture.page.getByRole("heading", { name: item.data.name, exact: true })).toBeVisible();
    }
    for (const item of lostItems) {
      await expect(fixture.page.getByText(item.data.description, { exact: true })).toBeVisible();
    }
    if (!visibleItems.length) await expect(fixture.page.getByText("No items available in this view.", { exact: true })).toBeVisible();
    if (!lostItems.length) await expect(fixture.page.getByText("No lost-property records available in this view.", { exact: true })).toBeVisible();
    assertNoWrites();
  } finally {
    await fixture.context.close();
  }
});

test("pms/reports runs a date-range report for the selected hotel using read-only owner data", async ({ browser }, info) => {
  test.skip(!isolatedHospitality, "Requires the exact disposable hospitality simulation on port 3102 or 3104.");
  const fixture = await prepareHospitalityOwner(browser, info);
  try {
    const assertNoWrites = watchForWrites(fixture.page);
    await fixture.page.goto(
      `${fixture.origin}/holdings/riverside-lantern/pms/reports?property=${encodeURIComponent(fixture.propertyId)}`,
    );
    await expect(fixture.page.locator('[data-source="live"]')).toBeVisible();
    await expect(fixture.page.getByRole("heading", { name: "Performance by date range", exact: true })).toBeVisible();
    await expect(fixture.page.getByRole("heading", { name: "PMS vs GL", exact: true })).toBeVisible();

    const ownerData = await readOwnerWorkspace(
      fixture.context,
      fixture.origin,
      fixture.companyId,
      fixture.propertyId,
      "view=report",
    );
    expect(ownerData.ledger).not.toBeNull();
    const date = ownerData.snapshot!.property.businessDate;
    await fixture.page.getByLabel("From", { exact: true }).fill(date);
    await fixture.page.getByLabel("To", { exact: true }).fill(date);
    const reportResponse = fixture.page.waitForResponse((response) =>
      response.request().method() === "GET" &&
      new URL(response.url()).pathname === "/api/v1/holdings/riverside-lantern/pms/reports",
    );
    await fixture.page.getByRole("button", { name: "Run report", exact: true }).click();
    const response = await reportResponse;
    expect(response.status()).toBe(200);
    const reportUrl = new URL(response.url());
    expect(reportUrl.searchParams.get("property")).toBe(fixture.propertyId);
    expect(reportUrl.searchParams.get("from")).toBe(date);
    expect(reportUrl.searchParams.get("to")).toBe(date);
    const { data: report } = await response.json() as {
      data: { from: string; to: string; days: Array<{ date: string }> };
    };
    expect(report).toMatchObject({ from: date, to: date });
    expect(report.days.map((day) => day.date)).toEqual([date]);
    await expect(fixture.page.getByText(`${date} → ${date} · THB · Balances at range end`, { exact: true })).toBeVisible();
    assertNoWrites();
  } finally {
    await fixture.context.close();
  }
});

test("pms/settings shows configured rooms and rates from the selected hotel's scoped setup read", async ({ browser }, info) => {
  test.skip(!isolatedHospitality, "Requires the exact disposable hospitality simulation on port 3102 or 3104.");
  const fixture = await prepareHospitalityOwner(browser, info);
  try {
    const assertNoWrites = watchForWrites(fixture.page);
    await fixture.page.goto(
      `${fixture.origin}/holdings/riverside-lantern/pms/settings?property=${encodeURIComponent(fixture.propertyId)}`,
    );
    await expect(fixture.page.locator('[data-source="live"]')).toBeVisible();
    await expect(fixture.page.getByRole("heading", { name: "Hotel details", exact: true })).toBeVisible();
    await expect(fixture.page.getByRole("heading", { name: "Room types", exact: true })).toBeVisible();
    await expect(fixture.page.getByRole("heading", { name: "Rate plans", exact: true })).toBeVisible();

    const ownerData = await readOwnerWorkspace(
      fixture.context,
      fixture.origin,
      fixture.companyId,
      fixture.propertyId,
      "view=setup",
    );
    const snapshot = ownerData.snapshot!;
    await expect(fixture.page.getByRole("heading", { name: snapshot.property.name, exact: true })).toBeVisible();
    const expectedRecords = [
      ...records(snapshot, "room_type"),
      ...records(snapshot, "room"),
      ...records(snapshot, "rate"),
    ];
    expect(records(snapshot, "room_type").length).toBeGreaterThan(0);
    expect(records(snapshot, "room").length).toBeGreaterThan(0);
    expect(records(snapshot, "rate").length).toBeGreaterThan(0);
    const displayedIds = await fixture.page.locator("[data-pms-record]").evaluateAll((nodes) =>
      nodes.map((node) => node.getAttribute("data-pms-record") ?? "").filter(Boolean),
    );
    expect(displayedIds).toEqual(expect.arrayContaining(expectedRecords.map((record) => record.id)));
    for (const roomType of records(snapshot, "room_type")) {
      await expect(fixture.page.getByText(roomType.data.name, { exact: true })).toBeVisible();
    }
    for (const room of records(snapshot, "room")) {
      await expect(fixture.page.getByText(room.data.number, { exact: true })).toBeVisible();
    }
    for (const rate of records(snapshot, "rate")) {
      await expect(fixture.page.getByText(rate.data.name, { exact: true })).toBeVisible();
    }
    assertNoWrites();
  } finally {
    await fixture.context.close();
  }
});
