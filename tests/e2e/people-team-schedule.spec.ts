import { expect, test } from "@playwright/test";
import { addDays, rosterMonday, type RosterCommand, type RosterData, type RosterWeek } from "../../src/lib/people-roster";
import { organizationSession, readOrganization } from "../helpers/organization-access-browser";
import { mockPeopleDraft } from "../helpers/people-draft-browser";

test("a team schedule adds shifts, publishes, copies a week and creates a Calendar appointment", async ({ page, context }) => {
  test.setTimeout(180_000);
  const base = await organizationSession(context, "hr_admin"), organization = await readOrganization(context, base);
  await mockPeopleDraft(page, organization);
  const position = organization.positions.find(position => position.active && position.roster_required && position.department_id)!;
  expect(position).toBeTruthy();
  const weekStart = addDays(rosterMonday(), 14), endpoint = `/api/v1${base}/people/roster`;
  const response = await context.request.get(`${endpoint}?department=${position.department_id}&week=${weekStart}`);
  expect(response.ok(), await response.text()).toBe(true);
  const data = (await response.json()).data as RosterData;
  expect(data.canManage).toBe(true); expect(data.people.length).toBeGreaterThan(0);
  const week: RosterWeek = { id: "browser-roster-week", company_id: position.company_id, branch_id: position.branch_id, department_id: position.department_id!, week_start: weekStart, timezone: "Asia/Bangkok", state: "draft", version: 1 };
  let current: RosterData = { ...data, weekStart, week, weeks: [week], shifts: [], assignments: [], requests: [], history: [], coverageGaps: [], conflicts: [] };
  const rosterWrites: RosterCommand[] = [], appointments: Record<string, unknown>[] = [];
  await page.route(/\/api\/v1\/.*\/people\/roster(?:\?.*)?$/, async route => {
    const respond = (value: unknown) => route.fulfill({ status: 200, contentType: "application/json", body: JSON.stringify({ data: value }) });
    if (route.request().method() === "GET") return respond(current);
    const input = route.request().postDataJSON() as RosterCommand; rosterWrites.push(input);
    if (input.action === "shift.save") {
      expect(input.expectedVersion).toBe(current.week!.version);
      const id = `browser-shift-${rosterWrites.length}`;
      current.shifts.push({ id, week_id: current.week!.id, name: input.name, local_date: input.date, start_time: input.startTime, end_time: input.endTime, starts_at: Date.parse(`${input.date}T${input.startTime}:00Z`), ends_at: Date.parse(`${input.date}T${input.endTime}:00Z`), capacity: input.capacity, state: "active", version: 1, template_id: input.templateId ?? null });
      current.assignments.push(...input.personIds.map(personId => { const person = current.people.find(person => person.person_id === personId)!; return { id: `browser-assignment-${personId}`, shift_id: id, person_id: personId, engagement_id: person.engagement_id, person_name: person.legal_name, state: "assigned" as const, version: 1 }; }));
      current.week = { ...current.week!, version: current.week!.version + 1 };
    } else if (input.action === "week.publish") {
      expect(input.expectedVersion).toBe(current.week!.version);
      current.week = { ...current.week!, state: "published", version: current.week!.version + 1 };
    } else if (input.action === "week.create") {
      expect(input.copyWeekId).toBe(current.week!.id);
      const copied = { ...current.week!, id: "browser-copied-week", week_start: input.weekStart, state: "draft" as const, version: 1 };
      const oldShifts = [...current.shifts];
      current = { ...current, week: copied, weekStart: input.weekStart, weeks: [...current.weeks, copied], shifts: [...oldShifts, ...oldShifts.map(shift => ({ ...shift, id: `copied-${shift.id}`, week_id: copied.id, local_date: addDays(shift.local_date, 7), starts_at: shift.starts_at + 7 * 86400000, ends_at: shift.ends_at + 7 * 86400000 }))], assignments: [...current.assignments, ...current.assignments.map(assignment => ({ ...assignment, id: `copied-${assignment.id}`, shift_id: `copied-${assignment.shift_id}` }))] };
    } else throw new Error(`Unexpected browser roster write: ${input.action}`);
    return respond({ id: current.week!.id });
  });
  await page.route(/\/api\/v1\/.*\/calendar\/appointments$/, async route => {
    appointments.push(route.request().postDataJSON());
    await route.fulfill({ status: 200, contentType: "application/json", body: JSON.stringify({ data: { id: "browser-team-appointment" } }) });
  });
  await page.goto(`${base}/people/roster?department=${position.department_id}&week=${weekStart}`);
  await expect(page).toHaveURL(/\/people\?.*panel=schedule/);
  await page.getByRole("button", { name: "Add shift", exact: true }).click();
  const shift = page.getByRole("dialog", { name: "Add shift", exact: true });
  await shift.getByLabel("Shift name", { exact: true }).fill("Browser team morning");
  await shift.getByLabel("Start time", { exact: true }).fill("08:00");
  await shift.getByLabel("End time", { exact: true }).fill("16:00");
  await shift.getByLabel(data.people[0].legal_name, { exact: true }).check();
  await shift.getByRole("button", { name: "Save shift", exact: true }).click();
  await expect(page.getByTestId("roster-shift")).toContainText("Browser team morning");
  await page.getByRole("button", { name: "Publish week", exact: true }).click();
  await page.getByRole("dialog", { name: "Publish roster", exact: true }).getByRole("button", { name: "Publish roster", exact: true }).click();
  await expect.poll(() => current.week!.state).toBe("published");
  await expect(page.getByRole("button", { name: "Publish week", exact: true })).toHaveCount(0);
  await page.getByRole("button", { name: "Copy week", exact: true }).click();
  await page.getByRole("dialog", { name: "Copy roster week", exact: true }).getByRole("button", { name: "Copy into draft", exact: true }).click();
  await expect(page.getByLabel("Week of", { exact: true })).toHaveValue(addDays(weekStart, 7));
  expect(rosterWrites.map(write => write.action)).toEqual(["shift.save", "week.publish", "week.create"]);
  await page.getByRole("button", { name: "Create team appointment", exact: true }).click();
  const appointment = page.getByRole("region", { name: "Create team appointment", exact: true });
  await appointment.getByLabel("Title", { exact: true }).fill("Browser team handoff");
  await appointment.getByRole("button", { name: "Save appointment", exact: true }).click();
  await expect(page.getByRole("status").filter({ hasText: "Team appointment saved to Calendar." })).toBeVisible();
  expect(appointments).toHaveLength(1);
  expect(appointments[0]).toMatchObject({ action: "appointment.save", title: "Browser team handoff", timezone: "Asia/Bangkok" });
  expect((appointments[0].participantIds as string[]).length).toBeGreaterThan(0);
  for (const width of [320, 390, 768, 1024, 1440]) {
    await page.setViewportSize({ width, height: 1000 });
    expect(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth + 1)).toBe(true);
  }
  await page.setViewportSize({ width: 390, height: 1000 });
  await page.getByRole("button", { name: "Back to structure", exact: true }).click();
  await expect(page.getByTestId("people-structure-workspace")).toBeVisible();
  await expect(page.getByTestId("people-detail")).toHaveCount(0);
  await expect.poll(() => new URL(page.url()).searchParams.get("unit")).toBe(`department:${position.department_id}`);
  expect(new URL(page.url()).searchParams.get("department")).toBeNull();
  await expect(page.getByTestId(`people-department-${position.department_id}`).locator('button[aria-pressed="true"]')).toHaveCount(1);
  await expect(page.getByRole("button", { name: /^Choose unit/ })).toBeVisible();
});
