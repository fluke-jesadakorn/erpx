import { expect, test } from "@playwright/test";
import { selectFixtureAssignment } from "./helpers/select-fixture-assignment";

const base = "/holdings/acme";
const calendarApi = "**/api/v1/holdings/acme/calendar?*";

function calendarFixture(url: string, partial = false) {
  const params = new URL(url).searchParams;
  const date = params.get("date") ?? "2026-09-23";
  const view = params.get("view") ?? "month";
  const layers = params.get("layers") ?? "support,crm,customer_followups,stock_transit,supplier_promises,procurement_rfqs,supplier_bills,quality_holds,supplier_batches,refunds,legal,roster,leave,appointments,announcements,lifecycle";
  const selectedLayers = layers.split(",").filter(Boolean);
  const monday = (value: string) => {
    const day = new Date(value + "T12:00:00Z").getUTCDay();
    const shifted = new Date(Date.parse(value + "T12:00:00Z") - ((day + 6) % 7) * 86400000);
    return shifted.toISOString().slice(0, 10);
  };
  const addDays = (value: string, days: number) =>
    new Date(Date.parse(value + "T12:00:00Z") + days * 86400000).toISOString().slice(0, 10);
  const from = view === "month" ? date.slice(0, 7) + "-01" : view === "week" ? monday(date) : date;
  const to = view === "month"
    ? new Date(Date.UTC(Number(from.slice(0, 4)), Number(from.slice(5, 7)), 1)).toISOString().slice(0, 10)
    : addDays(from, view === "week" ? 7 : view === "day" ? 1 : 30);
  const sourceStatuses = selectedLayers.map((source, index) => ({
    source,
    status: partial && index < 2 ? "error" : "ready",
    ...(partial && index < 2 ? { message: "Source could not be loaded." } : {}),
  }));
  const item = (source: string, sourceId: string, title: string, startDate: string, permittedActions: string[] = []) => ({
    id: source + ":" + sourceId,
    source,
    sourceId,
    kind: source === "roster" ? "shift" : "appointment",
    title,
    description: "",
    allDay: true,
    startDate,
    endDate: addDays(startDate, 1),
    startsAt: null,
    endsAt: null,
    timezone: params.get("timezone") ?? "Asia/Bangkok",
    departmentId: source === "roster" ? "dept-1" : null,
    branchId: null,
    participants: [{ id: "person-1", name: "Mali Srisai" }],
    owner: null,
    status: "scheduled",
    version: 1,
    permittedActions,
    warnings: [],
    history: [],
  });
  const items = [];
  if (selectedLayers.includes("appointments")) {
    items.push(item("appointments", "calendar-selected", "Calendar focus event", date, ["appointment.save"]));
    for (let number = 1; number <= 3; number++) {
      items.push(item("appointments", "calendar-overflow-" + number, "Overflow preview " + number, "2026-09-10"));
    }
  }
  if (selectedLayers.includes("roster")) {
    items.push(item("roster", "calendar-roster", "Morning shift", "2026-09-18", ["roster.open"]));
  }
  const cursor = Number(params.get("cursor") ?? "0");
  const query = {
    date,
    view,
    hierarchy: params.get("hierarchy") ?? "organization",
    mode: params.get("mode") ?? "team",
    preset: params.get("preset") ?? undefined,
    person: params.get("person") ?? undefined,
    node: params.get("node") ?? undefined,
    search: params.get("search") ?? "",
    layers,
    timezone: params.get("timezone") ?? "Asia/Bangkok",
    cursor,
    limit: Number(params.get("limit") ?? "100"),
  };
  const nextCursor = partial ? 100 : null;
  return {
    data: {
      query,
      from,
      to,
      items: cursor > 0 ? [] : items,
      nextCursor,
      sources: sourceStatuses,
      coverage: { shifts: 1, assigned: 1, open: 1, warnings: 0, partial: partial || nextCursor !== null },
      selfPersonId: "person-1",
      canManageRoster: true,
      canSchedule: true,
      canAnnounce: true,
      teamDefault: true,
    },
  };
}

async function mockCalendarApi(page: import("@playwright/test").Page) {
  await page.route(calendarApi, (route) => route.fulfill({ json: calendarFixture(route.request().url()) }));
  await page.route("**/api/v1/holdings/acme/calendar/hierarchy?*", (route) => route.fulfill({
    json: { data: { nodes: [{ id: "dept-1", parentId: null, name: "Operations", kind: "department", branchId: null, inScope: true }], people: [{ id: "person-1", name: "Mali Srisai", userId: null, departmentId: "dept-1", branchId: null, managerId: null, status: "active" }], nextCursor: null, selfPersonId: "person-1" } },
  }));
  await page.route("**/api/v1/holdings/acme/people/roster?*", (route) => route.fulfill({
    json: { data: { asOf: 0, departments: [{ id: "dept-1", name: "Operations", branch_id: null, branch_name: null }], departmentId: "dept-1", weekStart: "2026-09-14", week: null, weeks: [], templates: [], shifts: [], assignments: [], requests: [], people: [], coverageGaps: [], history: [], conflicts: [], selfPersonId: null, selfRosterRequired: false, canManage: true, canApprove: true } },
  }));
}

test.beforeEach(async ({ context, baseURL }) => {
  test.skip(
    !["localhost", "127.0.0.1"].includes(new URL(baseURL!).hostname),
    "Uses the existing local test account.",
  );
  const response = await context.request.post("/api/auth/sign-in/email", {
    data: { email: "mali@erpx.dev", password: "Mali-ERPX-2026!" },
  });
  expect(response.status()).toBe(200);
  await selectFixtureAssignment(context, "acme-co", "seed_usr_mali");
});

test.beforeEach(async ({ page }) => {
  await mockCalendarApi(page);
});

test("calendar opens in the month view with a selected day at every viewport", async ({
  page,
}, testInfo) => {
  await page.goto(`${base}/calendar?layers=appointments`);
  const workspace = page.locator("[data-calendar-workspace]");
  await expect(workspace).toHaveCount(1);
  await expect(page.getByRole("button", { name: "Customize layout" })).toHaveCount(0);

  await expect(
    page.getByRole("group", { name: "Calendar view" }).getByRole("button", {
      name: "Month",
    }),
  ).toHaveAttribute("aria-pressed", "true");
  await expect(page.locator("[data-calendar-view]")).toHaveCount(1);
  await expect(page.locator('[data-calendar-view="month"]')).toBeVisible();
  await expect(page.locator("[data-calendar-attention]")).toHaveCount(0);
  await expect(page.locator('[data-calendar-date][data-selected="true"]')).toHaveCount(1);
  await expect(page.getByRole("group", { name: "Calendar view" }).first()).toBeVisible();
  await expect(page.getByRole("button", { name: "My", exact: true })).toBeVisible();
  await expect(page.getByRole("button", { name: "Team", exact: true })).toBeVisible();
  await expect(page.getByRole("button", { name: "Coverage", exact: true })).toBeVisible();

  const grid = page.locator("[data-month-grid]");
  for (const cell of await grid.locator("[data-calendar-date]").all()) {
    const count = Number(await cell.getAttribute("data-event-count"));
    const previews = cell.locator("[data-month-event-previews] [data-calendar-event]");
    expect(await previews.count()).toBeLessThanOrEqual(2);
    if (count > 2) {
      await expect(cell.getByRole("button", { name: new RegExp(`Show ${count - 2} more loaded events for`) })).toBeVisible();
    }
    if (testInfo.project.name === "mobile") {
      await expect(previews).toBeHidden();
      if (count > 0) await expect(cell.locator("[data-calendar-event-count]")).toBeVisible();
    }
  }
  await expect(page.locator("[data-selected-calendar-day]")).toBeVisible();
  await page.locator("html").evaluate((element) => element.setAttribute("data-theme", "dark"));
  await expect(page.locator('[data-calendar-view="month"]')).toBeVisible();
  expect(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth + 1)).toBe(true);
});

test("calendar view and filters follow URL history and event detail retains the view", async ({
  page,
}) => {
  await page.goto(`${base}/calendar?view=week&date=2026-09-18&layers=appointments,roster`);
  const views = page.getByRole("group", { name: "Calendar view" });
  const appointments = page.getByLabel("Appointments", { exact: true });
  const filterTrigger = page.locator("[data-calendar-filters] > summary");
  const filters = page.getByRole("dialog", { name: /^Filters\b/ });
  const verifyAppointments = async () => {
    await filterTrigger.click();
    await expect(filters).toBeVisible();
    await expect(filters.getByLabel("Appointments", { exact: true })).not.toBeChecked();
    await filters.getByRole("button", { name: "Close dialog", exact: true }).click();
    await expect(filters).toHaveCount(0);
  };

  await filterTrigger.click();
  await expect(filters).toBeVisible();
  await appointments.click();
  await expect(appointments).not.toBeChecked();
  await expect(page).toHaveURL(/(?:\?|&)layers=/);
  await expect(page).not.toHaveURL(/layers=[^&]*appointments/);
  await filters.getByRole("button", { name: "Close dialog", exact: true }).click();
  await expect(filters).toHaveCount(0);

  await views.getByRole("button", { name: "Month" }).click();
  await expect(page).toHaveURL(/(?:\?|&)view=month(?:&|$)/);
  await expect(page.locator('[data-calendar-view="month"]')).toBeVisible();
  await verifyAppointments();

  await views.getByRole("button", { name: "Agenda" }).click();
  await expect(page).toHaveURL(/(?:\?|&)view=agenda(?:&|$)/);
  await page.goBack();
  await expect(views.getByRole("button", { name: "Month" })).toHaveAttribute(
    "aria-pressed",
    "true",
  );
  await verifyAppointments();

  await views.getByRole("button", { name: "Week" }).click();
  const event = page.locator("[data-calendar-event]").first();
  await expect(event).toBeVisible();
  await event.click();
  await expect(page).toHaveURL(/(?:\?|&)view=week(?:&|$)/);
  await expect(page).toHaveURL(/(?:\?|&)event=[^&]+/);
  await expect(page.getByRole("dialog")).toHaveCount(1);
  await page.goBack();
  await expect(page.getByRole("dialog")).toHaveCount(0);
  await expect(views.getByRole("button", { name: "Week" })).toHaveAttribute(
    "aria-pressed",
    "true",
  );
  await page.goForward();
  await expect(page.getByRole("dialog")).toHaveCount(1);
  await expect(page).toHaveURL(/(?:\?|&)event=[^&]+/);
});

test("month selection updates the date URL and outside-month dates open their month", async ({ page }) => {
  await page.goto(`${base}/calendar?view=month&date=2026-09-01&layers=appointments`);
  const grid = page.locator('[data-calendar-view="month"]');
  await expect(grid.locator('[data-calendar-date="2026-09-01"][data-selected="true"]')).toBeVisible();
  await page.getByRole("button", { name: "Show Monday, August 31, 2026" }).click();
  await expect(page).toHaveURL(/(?:\?|&)date=2026-08-31(?:&|$)/);
  await expect(page.locator('[data-calendar-view="month"] [data-calendar-date="2026-08-31"][data-selected="true"]')).toBeVisible();
  await expect(page.locator("#selected-calendar-day")).toContainText(/(?:August 31,|31 August) 2026/);
});

test("month previews cap at two and the selected date keeps its full loaded list", async ({ page }, testInfo) => {
  await page.goto(`${base}/calendar?view=month&date=2026-09-01&layers=appointments`);
  await page.getByRole("button", { name: "Refresh calendar" }).click();

  const cell = page.locator('[data-calendar-view="month"] [data-calendar-date="2026-09-10"]');
  await expect(cell).toHaveAttribute("data-event-count", /[3-9]|[1-9][0-9]+/);
  const count = Number(await cell.getAttribute("data-event-count"));
  await expect(cell.locator("[data-month-event-previews] [data-calendar-event]")).toHaveCount(2);
  if (testInfo.project.name === "mobile") {
    await expect(cell.locator("[data-calendar-event-count]")).toHaveText(String(count));
    const dayButton = cell.getByRole("button", { name: "Show Thursday, September 10, 2026" });
    await dayButton.focus();
    await page.keyboard.press("Enter");
  } else {
    await cell.getByRole("button", { name: /Show \d+ more loaded events for (?:September 10|10 September)/ }).click();
  }
  await expect(page).toHaveURL(/(?:\?|&)date=2026-09-10(?:&|$)/);
  const loadedForSelectedDate = Number(await page.locator('[data-calendar-date="2026-09-10"]').getAttribute("data-event-count"));
  await expect(page.locator('[data-selected-calendar-day] [data-calendar-event]')).toHaveCount(loadedForSelectedDate);
  await expect(page.locator('[data-selected-calendar-day]')).toContainText("Overflow preview 1");
  await expect(page.locator('[data-selected-calendar-day]')).toContainText("Overflow preview 3");
});

test("calendar attention stays compact and distinguishes loaded warning items", async ({ page }, testInfo) => {
  await page.unroute(calendarApi);
  await page.route(calendarApi, (route) => {
    const fixture = calendarFixture(route.request().url());
    const baseItem = fixture.data.items[0];
    const items = Array.from({ length: 8 }, (_, index) => ({
      ...baseItem,
      id: `lifecycle:attention-${index}`,
      source: "lifecycle",
      sourceId: `attention-${index}`,
      kind: "task",
      title: index === 2 ? "ตรวจสอบเอกสารและสิทธิ์การเข้าถึงของพนักงานก่อนวันสุดท้าย" : index < 2 ? "Review final payroll" : `Preparation task ${index + 1}`,
      startDate: index === 7 ? null : `2026-09-${String(10 + index).padStart(2, "0")}`,
      participants: [{ id: `person-${index}`, name: `Colleague ${index + 1}` }],
      owner: "People Operations",
      warnings: ["Overdue preparation task"],
    }));
    const inSeptember = fixture.data.query.date.startsWith("2026-09");
    void route.fulfill({ json: { data: { ...fixture.data, items: inSeptember ? items.reverse() : [], nextCursor: inSeptember ? 100 : null, coverage: { ...fixture.data.coverage, warnings: inSeptember ? 8 : 0, partial: inSeptember } } } });
  });
  await page.goto(`${base}/calendar?view=month&date=2026-09-12&layers=appointments`);
  await page.getByRole("button", { name: "Refresh calendar" }).click();

  const attention = page.locator("[data-calendar-attention]");
  await expect(attention).toBeVisible();
  await expect(attention).not.toHaveAttribute("open", "");
  await expect(attention.locator("summary")).toContainText("8 loaded");
  await expect(attention.locator("summary")).toContainText("Review final payroll");
  expect(await attention.locator("summary").evaluate((element) => element.getBoundingClientRect().height)).toBeLessThanOrEqual(80);

  await attention.locator("summary").focus();
  await page.keyboard.press("Enter");
  await expect(attention).toHaveAttribute("open", "");
  await expect(attention.locator("li")).toHaveCount(8);
  await expect(attention.locator("li").first()).toContainText("Colleague 1");
  await expect(attention.locator("li").nth(1)).toContainText("Colleague 2");
  await expect(attention.locator("li").last()).toContainText("Unscheduled");
  await expect(attention).toContainText("Only loaded events are counted");
  expect(await attention.locator("ul").evaluate((element) => element.getBoundingClientRect().height)).toBeLessThanOrEqual(testInfo.project.name === "mobile" ? 280 : 320);
  expect(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth + 1)).toBe(true);
  await page.locator("html").evaluate((element) => element.setAttribute("data-theme", "dark"));
  await expect(attention).toBeVisible();
  expect(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth + 1)).toBe(true);

  await attention.locator("li").first().getByRole("button").click();
  await expect(page.getByRole("dialog")).toBeVisible();
  await expect(page.getByRole("dialog")).toContainText("Review final payroll");
  await page.getByRole("button", { name: "Close dialog" }).click();
  await page.getByRole("button", { name: "Next month" }).click();
  await expect(attention).toHaveCount(0);
});

test("calendar presets preserve date and timezone, reset people search, and restore history", async ({ page }) => {
  const hierarchy = await page.request.get("/api/v1/holdings/acme/calendar/hierarchy?limit=10");
  expect(hierarchy.status()).toBe(200);
  const person = (await hierarchy.json()).data.people[0]?.id as string | undefined;
  expect(person).toBeTruthy();
  await page.goto(`${base}/calendar?view=agenda&date=2026-09-18&timezone=UTC&mode=team&preset=team&layers=appointments,roster&person=${encodeURIComponent(person!)}&search=weekend`);

  await page.getByRole("button", { name: "Coverage", exact: true }).click();
  await expect(page).toHaveURL(/(?:\?|&)view=week(?:&|$)/);
  let query = new URL(page.url()).searchParams;
  expect(query.get("date")).toBe("2026-09-18");
  expect(query.get("timezone")).toBe("UTC");
  expect(query.get("person")).toBeNull();
  expect(query.get("search")).toBeNull();
  expect(query.get("layers")).toBe("roster,leave");

  await page.getByRole("button", { name: "My", exact: true }).click();
  await expect(page).toHaveURL(/(?:\?|&)view=month(?:&|$)/);
  query = new URL(page.url()).searchParams;
  expect(query.get("date")).toBe("2026-09-18");
  expect(query.get("timezone")).toBe("UTC");
  expect(query.get("person")).toBeNull();
  expect(query.get("search")).toBeNull();
  await page.goBack();
  await expect(page.getByRole("button", { name: "Coverage", exact: true })).toHaveAttribute("aria-pressed", "true");
  await page.goBack();
  const restored = new URL(page.url()).searchParams;
  expect(restored.get("person")).toBe(person);
  expect(restored.get("search")).toBe("weekend");
  await expect(page.getByRole("button", { name: "Agenda", exact: true })).toHaveAttribute("aria-pressed", "true");
});

test("partial calendar sources share one retry notice and mark loaded results", async ({ page }) => {
  let failOnce = true;
  await page.unroute(calendarApi);
  await page.route(calendarApi, async (route) => {
    const partial = failOnce;
    failOnce = false;
    await route.fulfill({ json: calendarFixture(route.request().url(), partial) });
  });

  await page.goto(`${base}/calendar?view=month&date=2026-09-01&layers=appointments,roster`);
  await page.getByRole("button", { name: "Refresh calendar" }).click();
  const notice = page.locator("[data-calendar-workspace] [role=alert]").filter({ hasText: "Some calendar sources could not be loaded" });
  await expect(notice).toHaveCount(1);
  await expect(notice).toContainText("Results are partial");
  await expect(page.getByRole("status")).toContainText("More results are available below");
  await expect(page.getByRole("button", { name: "Load more events" })).toBeVisible();
  await page.getByRole("button", { name: "Retry calendar sources" }).click();
  await expect(notice).toHaveCount(0);
});

test("week and day navigation move by their calendar period", async ({ page }) => {
  await page.goto(`${base}/calendar?view=week&date=2026-09-18&layers=roster`);
  await page.getByRole("button", { name: "Next period" }).click();
  await expect(page).toHaveURL(/(?:\?|&)date=2026-09-25(?:&|$)/);
  await expect(page.locator('[data-calendar-view="week"]')).toBeVisible();

  await page.goto(`${base}/calendar?view=day&date=2026-09-18&layers=roster`);
  await page.getByRole("button", { name: "Next period" }).click();
  await expect(page).toHaveURL(/(?:\?|&)date=2026-09-19(?:&|$)/);
  await expect(page.locator('[data-calendar-view="day"]')).toBeVisible();
});

test("authorized roster events open the roster planner", async ({ page }) => {
  const query = "view=week&date=2026-09-18&mode=team&preset=team&layers=roster";
  await page.goto(`${base}/calendar?${query}`);
  await page.getByRole("button", { name: "Refresh calendar" }).click();
  await page.locator("[data-calendar-event]").filter({ hasText: "Morning shift" }).first().click();
  const detail = page.getByRole("dialog");
  await expect(detail.getByRole("button", { name: "Open roster actions" })).toBeVisible();
  await detail.getByRole("button", { name: "Open roster actions" }).click();
  await expect(page.getByRole("dialog")).toContainText("Roster planning");
});

test("day view renders a horizontally scrollable people timeline and opens event details", async ({ page }) => {
  await page.route(calendarApi, (route) => {
    const fixture = calendarFixture(route.request().url());
    if (!fixture.data.items.some(item => item.sourceId === "calendar-selected")) throw new Error("DAY_TIMELINE_APPOINTMENT_FIXTURE_REQUIRED");
    return route.fulfill({ json: { data: { ...fixture.data, items: fixture.data.items.map(item => item.sourceId === "calendar-selected" ? {
      ...item,
      allDay: false,
      startsAt: Date.parse("2026-09-18T09:00:00+07:00"),
      endsAt: Date.parse("2026-09-18T10:00:00+07:00"),
    } : item) } } });
  });
  await page.goto(`${base}/calendar?view=day&date=2026-09-18`);
  await page.getByRole("button", { name: "Refresh calendar", exact: true }).click();
  const timeline = page.locator('[data-calendar-view="day"]');
  await expect(timeline).toBeVisible();
  await expect(page.getByRole("button", { name: "Day", exact: true })).toHaveAttribute("aria-pressed", "true");
  await expect(timeline.locator("[data-calendar-event]").first()).toBeVisible();
  await timeline.locator("[data-calendar-event]").first().click();
  await expect(page).toHaveURL(/(?:\?|&)event=[^&]+/);
  await expect(page.getByRole("dialog")).toHaveCount(1);
  expect(await timeline.evaluate((element) => element.scrollWidth > element.clientWidth)).toBe(true);
});
