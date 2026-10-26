import { afterEach, beforeAll, beforeEach, expect, it, vi } from "vitest";
import {mkdtemp,mkdir,readdir,cp,rm} from "node:fs/promises";
import {resolve,join} from "node:path";
import {migrateDatabase} from "@erpx/server-runtime/public/runtime/migrations";
import { env, applyMigrations } from "../../support/runtime-env";
import { seedExpenseCompany } from "../../../fixtures/expenses/company";
import {
  calendarData,
  calendarHierarchy,
  calendarCommand,
} from "@erpx/server-work_projection/public/calendar/index";
import { publishedAnnouncements } from "@erpx/server-work_projection/public/calendar/announcements";
import { rosterCommand, rosterData } from "@erpx/server-people_payroll/public/people-roster";
import { localInterval } from "@erpx/server-work_projection/public/calendar/commands";
import {drainCalendarReminders} from "@erpx/server-work_projection/public/calendar/reminders";
import {calendarAvailability} from "@erpx/server-work_projection/public/calendar/availability";
import { rosterMonday, addDays } from "@erpx/feature-people_payroll/lib/people-roster";
import { packageGrantStore } from "@erpx/server-commercial/public/package-grants";
import { calendarRosterPeoplePayrollFacts } from "@erpx/server-people_payroll/public/calendar-roster-facts";
import { calendarQuerySchema, itemOnDate } from "@erpx/feature-work_projection/lib/calendar";
import type { WorkContext } from "@erpx/server-identity_access/public/authorization";
vi.mock("@erpx/server-runtime/adapters/runtime", () => ({
  holdingResources: async () => ({ write: env.H001_DB, files: env.H001_FILES }),
  runtimeEnv: async () => env,
}));
const db = env.H001_DB,
  key = () => crypto.randomUUID(),
  person = (a: WorkContext) => `person-${a.userId}`;
afterEach(()=>vi.unstubAllEnvs());
let f: Awaited<ReturnType<typeof seedExpenseCompany>>;
const privateGuestPhone = "+66-555-010-1234";
const date = addDays(rosterMonday(), 14),
  read = (actor = f.manager, query: Record<string, unknown> = {}) =>
    calendarData(db, actor, { date, mode: "team", ...query });
const command = (input: unknown, actor = f.manager, id = key()) =>
  calendarCommand(db, actor, input, id);
function appointment(extra: Record<string, unknown> = {}) {
  return {
    action: "appointment.save",
    organizerPersonId: person(f.claimant),
    participantIds: [person(f.tie)],
    title: "Private interview",
    description: "Private notes",
    startLocal: `${date}T09:00`,
    endLocal: `${date}T10:00`,
    guests: [{ name: "Visitor", email: "visitor@example.test", phone: privateGuestPhone }],
    ...extra,
  };
}
async function shift(dateValue = date) {
  const w = await rosterCommand(
    db,
    f.manager,
    {
      action: "week.create",
      departmentId: f.department,
      weekStart: rosterMonday(dateValue),
    },
    key(),
  );
  const s = await rosterCommand(
    db,
    f.manager,
    {
      action: "shift.save",
      weekId: w.id,
      expectedVersion: 1,
      date: dateValue,
      name: "Night shift",
      startTime: "22:00",
      endTime: "06:00",
      capacity: 2,
      personIds: [person(f.claimant)],
    },
    key(),
  );
  return { w, s };
}
beforeAll(async () => {
  await applyMigrations(db, env.STAFF_TEST_MIGRATIONS);
});
beforeEach(async () => {
  f = await seedExpenseCompany(db, env.H001_FILES);
  f.manager = await f.member("calendar-manager", "people_manager");
  f.tie = await f.member("calendar-participant", "employee");
  const position=`${f.company}-calendar-shift-position`;
  await db.prepare("INSERT INTO organization_positions(id,company_id,branch_id,department_id,title,level_id,roster_required,created_by,created_at,updated_at) VALUES(?,?,?,?,?,?,1,'fixture',1,1)")
    .bind(position,f.company,f.branch,f.department,"Calendar shift worker",`${f.company}-level-1`).run();
  await db.prepare("UPDATE hr_engagements SET position_id=? WHERE person_id=?").bind(position,person(f.claimant)).run();
});
it("keeps occurrence responses separate from the series and hides a declined occurrence",async()=>{
  const saved=await command(appointment({recurrence:{frequency:"daily",intervalCount:1,weekdays:[],endKind:"count",occurrenceCount:3}}),f.claimant);
  const mine=(await read(f.tie,{mode:"mine",view:"week",date})).items.filter(item=>item.sourceId===saved.id);
  expect(mine.length).toBeGreaterThan(0);
  const occurrence=mine[0].occurrenceId!;
  await command({action:"appointment.respond",id:saved.id,expectedVersion:saved.version,occurrenceId:occurrence,scope:"single",response:"declined"},f.tie);
  const remaining=(await read(f.tie,{mode:"mine",view:"week",date})).items.filter(item=>item.sourceId===saved.id);
  expect(remaining.some(item=>item.occurrenceId===occurrence)).toBe(false);
  expect(remaining.some(item=>item.occurrenceId!==occurrence)).toBe(true);
});
it("shows a moved occurrence in its destination range with its original identity",async()=>{
  const saved=await command(appointment({recurrence:{frequency:"daily",intervalCount:1,weekdays:[],endKind:"count",occurrenceCount:3}}),f.claimant);
  const occurrenceId=`${date}T09:00`,target=addDays(date,8);
  await command({action:"appointment.reschedule",id:saved.id,expectedVersion:saved.version,occurrenceId,scope:"single",
    startLocal:`${target}T09:00`,endLocal:`${target}T10:00`,timezone:"Asia/Bangkok"},f.claimant);
  const moved=(await read(f.claimant,{mode:"mine",view:"week",date:target})).items.find(item=>item.sourceId===saved.id);
  expect(moved?.occurrenceId).toBe(occurrenceId);
  expect(moved?.startDate).toBeNull();
  expect(moved?.startsAt).toBeGreaterThan(Date.parse(`${target}T00:00Z`));
});
it("splits this and future occurrences while retaining earlier responses and inviting the new series",async()=>{
  const saved=await command(appointment({recurrence:{frequency:"daily",intervalCount:1,weekdays:[],endKind:"count",occurrenceCount:4}}),f.claimant);
  await command({action:"appointment.respond",id:saved.id,expectedVersion:saved.version,occurrenceId:`${date}T09:00`,scope:"single",response:"accepted"},f.tie);
  const current=await db.prepare("SELECT version FROM calendar_appointments WHERE id=?").bind(saved.id).first<{version:number}>();
  const keyValue=key(),future=addDays(date,1);
  await command({action:"appointment.reschedule",id:saved.id,expectedVersion:current!.version,occurrenceId:`${future}T09:00`,scope:"future",
    startLocal:`${future}T11:00`,endLocal:`${future}T12:00`,timezone:"Asia/Bangkok"},f.claimant,keyValue);
  const nextId=`calendar-${keyValue}`;
  const previous=await db.prepare("SELECT response FROM calendar_occurrence_responses WHERE appointment_id=? AND occurrence_key=? AND user_id=?")
    .bind(saved.id,`${date}T09:00`,f.tie.userId).first<{response:string}>();
  expect(previous?.response).toBe("accepted");
  expect((await read(f.tie,{mode:"mine",view:"week",date:future})).items.some(item=>item.sourceId===nextId)).toBe(true);
  const invitation=await db.prepare("SELECT 1 FROM inbox_events WHERE source_type='calendar_appointment' AND source_id=? AND company_id=?")
    .bind(nextId,f.claimant.companyId).first();
  expect(invitation).toBeTruthy();
});
it("stores an all-day appointment with an exclusive end date",async()=>{
  const receiptKey=key(),input=appointment({allDay:true,startDate:date,endDate:addDays(date,2),startLocal:undefined,endLocal:undefined});
  const tables=["calendar_appointments","calendar_attendees","calendar_appointment_participants","calendar_series","calendar_occurrence_exceptions","calendar_occurrence_responses","calendar_reminder_deliveries","calendar_history","inbox_events","inbox_deliveries","command_receipts","command_assertions","audit_events","outbox_events"];
  const snapshot=async()=>Object.fromEntries(await Promise.all(tables.map(async table=>[table,(await db.prepare(`SELECT coalesce(jsonb_agg(row_value ORDER BY row_value::text),'[]'::jsonb) rows FROM (SELECT to_jsonb(t) row_value FROM ${table} t) snapshot`).first<{rows:unknown}>())!.rows])));
  const before=await snapshot();
  await db.prepare(`CREATE FUNCTION fail_calendar_final_outbox() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN IF NEW.command_id='${receiptKey}' THEN RAISE EXCEPTION 'calendar final outbox rollback'; END IF; RETURN NEW; END $$`).run();
  await db.prepare("CREATE TRIGGER fail_calendar_final_outbox BEFORE INSERT ON outbox_events FOR EACH ROW EXECUTE FUNCTION fail_calendar_final_outbox()").run();
  try{await expect(command(input,f.claimant,receiptKey)).rejects.toThrow("calendar final outbox rollback");expect(await snapshot()).toEqual(before);}finally{await db.prepare("DROP TRIGGER fail_calendar_final_outbox ON outbox_events").run();await db.prepare("DROP FUNCTION fail_calendar_final_outbox()").run();}
  const saved=await command(input,f.claimant,receiptKey);
  expect(await command(input,f.claimant,receiptKey)).toEqual(saved);
  for(const table of ["command_receipts","audit_events","outbox_events"])expect(await db.prepare(`SELECT count(*)::int count FROM ${table} WHERE command_id=?`).bind(receiptKey).first()).toEqual({count:1});
  const item=(await read(f.claimant,{mode:"mine",view:"week",date})).items.find(item=>item.sourceId===saved.id)!;
  expect(item.allDay).toBe(true);
  expect(item.startDate).toBe(date);
  expect(item.endDate).toBe(addDays(date,2));
  await command({action:"appointment.reschedule",id:saved.id,expectedVersion:saved.version,
    startLocal:`${addDays(date,1)}T00:00`,endLocal:`${addDays(date,3)}T00:00`,timezone:"Asia/Bangkok"},f.claimant);
  const moved=(await read(f.claimant,{mode:"mine",view:"week",date})).items.find(row=>row.sourceId===saved.id)!;
  expect(moved.startDate).toBe(addDays(date,1));
  expect(moved.endDate).toBe(addDays(date,3));
});
it("lets an active account organize without an HR Person link",async()=>{
  const saved=await command(appointment({organizerPersonId:undefined,participantIds:[]}),f.claimant);
  const row=await db.prepare("SELECT organizer_person_id,created_by FROM calendar_appointments WHERE id=?").bind(saved.id)
    .first<{organizer_person_id:string|null;created_by:string}>();
  expect(row?.organizer_person_id).toBeNull();
  expect(row?.created_by).toBe(f.claimant.userId);
  expect((await read(f.claimant,{mode:"mine"})).items.some(item=>item.sourceId===saved.id)).toBe(true);
});
it("deduplicates reminders and suppresses them after cancellation",async()=>{
  const saved=await command(appointment({participantIds:[]}),f.claimant);
  const appointmentRow=await db.prepare("SELECT starts_at FROM calendar_appointments WHERE id=?").bind(saved.id).first<{starts_at:number}>();
  const due=appointmentRow!.starts_at-15*60000;
  expect(await drainCalendarReminders(db,due)).toBeGreaterThanOrEqual(1);
  expect(await drainCalendarReminders(db,due)).toBe(0);
  await command({action:"appointment.status",id:saved.id,expectedVersion:saved.version,status:"cancelled"},f.claimant);
  expect(await drainCalendarReminders(db,due+60000)).toBe(0);
});
it("returns busy intervals without appointment details",async()=>{
  const saved=await command(appointment(),f.claimant);
  const row=await db.prepare("SELECT starts_at,ends_at FROM calendar_appointments WHERE id=?").bind(saved.id).first<{starts_at:number;ends_at:number}>();
  const availability=await calendarAvailability(db,f.claimant,[f.tie.userId],row!.starts_at-3600000,row!.ends_at+3600000);
  expect(availability.intervals.some(interval=>interval.startsAt===row!.starts_at&&interval.endsAt===row!.ends_at)).toBe(true);
  expect(JSON.stringify(availability)).not.toContain("Private interview");
  expect(JSON.stringify(availability)).not.toContain("Visitor");
});
it("accepts independent attendee responses with a stale appointment version",async()=>{
  const saved=await command(appointment({participantIds:[person(f.tie),person(f.manager)]}),f.claimant);
  await command({action:"appointment.respond",id:saved.id,expectedVersion:saved.version,response:"accepted"},f.tie);
  await command({action:"appointment.respond",id:saved.id,expectedVersion:saved.version,response:"tentative"},f.manager);
  const row=await db.prepare("SELECT version FROM calendar_appointments WHERE id=?").bind(saved.id).first<{version:number}>();
  expect(row?.version).toBe(saved.version+2);
  const attendees=(await read(f.claimant)).items.find(item=>item.sourceId===saved.id)?.details?.attendees;
  expect(attendees?.find(person=>person.userId===f.tie.userId)?.response).toBe("accepted");
  expect(attendees?.find(person=>person.userId===f.manager.userId)?.response).toBe("tentative");
});
it("reads all sources without causing workflow transitions and hides draft rosters from staff", async () => {
  await shift();
  const before = await db
    .prepare("SELECT count(*)::int n FROM command_receipts")
    .first<{ n: number }>();
  const manager = await read(),
    staff = await read(f.claimant);
  expect(manager.sources.every((s) => s.status === "ready")).toBe(true);
  expect(manager.items.filter((i) => i.kind === "shift")).toHaveLength(1);
  expect(staff.items.filter((i) => i.kind === "shift")).toHaveLength(0);
  expect(
    await db.prepare("SELECT count(*)::int n FROM command_receipts").first(),
  ).toEqual(before);
});
it("preserves overnight shifts crossing adjacent weeks and stored timezone", async () => {
  const { w } = await shift(addDays(date, -1));
  const roster = await rosterData(db, f.manager, {
    department: f.department,
    week: addDays(date, -7),
  });
  await rosterCommand(
    db,
    f.manager,
    { action: "week.publish", id: w.id, expectedVersion: roster.week!.version },
    key(),
  );
  const d = await read(f.claimant);
  const s = d.items.find((i) => i.kind === "shift")!;
  expect(s.timezone).toBe("Asia/Bangkok");
  expect(s.endsAt! - s.startsAt!).toBe(8 * 3600000);
  expect(itemOnDate(s, date, "Asia/Bangkok")).toBe(true);
  expect(itemOnDate(s, date, "UTC")).toBe(false);
});
it("lets active staff organize, scopes delegates, redacts availability before serialization and checks direct commands", async () => {
  const saved = await command(appointment(), f.claimant);
  expect(saved.version).toBe(1);
  const participant = await read(f.tie);
  expect(
    participant.items.find((i) => i.sourceId === saved.id)?.details?.guests?.[0]
      .email,
  ).toBe("visitor@example.test");
  expect(participant.items[0].permittedActions).toEqual(["appointment.respond", "appointment.reminder.set"]);
  const outsider = await f.member("availability-only", "employee");
  const viewer = {
    ...outsider,
    iam: { permissions: ["calendar.availability.read"] },
  } as WorkContext;
  const busy = (await read(viewer)).items.find((i) => i.sourceId === saved.id)!;
  expect(busy.title).toBe("Busy");
  expect(busy.details).toBeUndefined();
  expect(busy.version).toBe(0);
  expect(JSON.stringify(busy)).not.toMatch(/Private|Visitor|example\.test/);
  expect(JSON.stringify(busy)).not.toContain(privateGuestPhone);
  expect((await read(outsider)).items).toEqual([]);
  await expect(
    command(
      {
        action: "appointment.status",
        id: saved.id,
        expectedVersion: 1,
        status: "cancelled",
      },
      f.tie,
    ),
  ).rejects.toMatchObject({ status: 403 });
  const other = await seedExpenseCompany(db, env.H001_FILES);
  await expect(
    command(
      {
        action: "appointment.status",
        id: saved.id,
        expectedVersion: 1,
        status: "cancelled",
      },
      other.owner,
    ),
  ).rejects.toMatchObject({ status: 404 });
  const scoped = {
    ...f.manager,
    branchId: f.branch,
    departmentId: f.accountsDepartment,
  };
  expect((await read(scoped)).items).toEqual([]);
});
it("uses stable receipts and versions for reschedule/cancel and rejects concurrent stale edits", async () => {
  const input = appointment(),
    id = key(),
    a = await command(input, f.claimant, id);
  expect(await command(input, f.claimant, id)).toEqual(a);
  await expect(
    command({ ...input, title: "Different" }, f.claimant, id),
  ).rejects.toMatchObject({ code: "IDEMPOTENCY_CONFLICT" });
  const changes = await Promise.allSettled(
    [1, 2].map((hour) =>
      command(
        appointment({
          id: a.id,
          expectedVersion: 1,
          startLocal: `${date}T1${hour}:00`,
          endLocal: `${date}T1${hour + 1}:00`,
        }),
        f.claimant,
      ),
    ),
  );
  expect(changes.filter((x) => x.status === "fulfilled")).toHaveLength(1);
  await expect(
    command(
      {
        action: "appointment.status",
        id: a.id,
        expectedVersion: 1,
        status: "cancelled",
      },
      f.claimant,
    ),
  ).rejects.toMatchObject({ code: "VERSION_CONFLICT" });
  await command(
    {
      action: "appointment.status",
      id: a.id,
      expectedVersion: 2,
      status: "cancelled",
    },
    f.claimant,
  );
  expect((await read(f.claimant)).items[0].status).toBe("cancelled");
  expect(
    (
      await db
        .prepare(
          "SELECT count(*)::int n FROM calendar_history WHERE record_id=?",
        )
        .bind(a.id)
        .first<{ n: number }>()
    )?.n,
  ).toBe(3);
});
it("reschedules through narrow commands while preserving appointment data and roster assignments", async () => {
  const appointmentResult = await command(appointment(), f.claimant);
  await command({
    action: "appointment.reschedule",
    id: appointmentResult.id,
    expectedVersion: 1,
    startLocal: `${date}T11:15`,
    endLocal: `${date}T12:15`,
    timezone: "Asia/Bangkok",
  }, f.claimant);
  const appointmentRow = await db.prepare("SELECT title,description,starts_at,ends_at,version FROM calendar_appointments WHERE id=?").bind(appointmentResult.id).first<{ title: string; description: string; starts_at: number; ends_at: number; version: number }>();
  expect(appointmentRow).toMatchObject({ title: "Private interview", description: "Private notes", version: 2 });
  expect(appointmentRow!.ends_at - appointmentRow!.starts_at).toBe(3_600_000);

  const notice = await command({
    action: "announcement.save",
    audienceId: f.department,
    title: "Range",
    content: "Preserved",
    startDate: date,
    endDate: addDays(date, 2),
  });
  await command({ action: "announcement.reschedule", id: notice.id, expectedVersion: 1, startDate: addDays(date, 1), endDate: addDays(date, 3) });
  expect(await db.prepare("SELECT title,start_date,end_date,version FROM calendar_announcements WHERE id=?").bind(notice.id).first()).toMatchObject({
    title: "Range", start_date: addDays(date, 1), end_date: addDays(date, 3), version: 2,
  });

  const roster = await shift();
  await command({ action: "shift.reschedule", id: roster.s.id, expectedVersion: 1, date: addDays(date, 1), startTime: "21:00", endTime: "05:00", reason: "" });
  const beforePublish = await rosterData(db, f.manager, { department: f.department, week: rosterMonday(date) });
  expect(beforePublish.assignments.filter((assignment) => assignment.shift_id === roster.s.id && assignment.state === "assigned")).toHaveLength(1);
  await rosterCommand(db, f.manager, { action: "week.publish", id: roster.w.id, expectedVersion: beforePublish.week!.version }, key());
  await expect(command({ action: "shift.reschedule", id: roster.s.id, expectedVersion: 2, date: addDays(date, 2), startTime: "21:00", endTime: "05:00", reason: "" })).rejects.toMatchObject({ code: "ROSTER_REASON" });
  await command({ action: "shift.reschedule", id: roster.s.id, expectedVersion: 2, date: addDays(date, 2), startTime: "21:00", endTime: "05:00", reason: "Coverage changed" });
  expect(await db.prepare("SELECT local_date,start_time,end_time,version FROM people_roster_shifts WHERE id=?").bind(roster.s.id).first()).toMatchObject({
    local_date: addDays(date, 2), start_time: "21:00", end_time: "05:00", version: 3,
  });
});
it("warns about overlaps and approved all-day leave without exposing reasons; meetings during shifts are allowed", async () => {
  await command(appointment(), f.claimant);
  const b = await command(
    appointment({ title: "Another appointment" }),
    f.claimant,
  );
  expect(b.warnings).toEqual(["An attendee is unavailable during this time."]);
  await db
    .prepare(
      "INSERT INTO employees(id,user_id,company_id,branch_id,department_id,person_id,display_name,status,created_at) VALUES(?,?,?,?,?,?,'Test','active',1)",
    )
    .bind(
      `emp-${f.claimant.userId}`,
      f.claimant.userId,
      f.company,
      f.branch,
      f.department,
      person(f.claimant),
    )
    .run();
  await db
    .prepare(
      "INSERT INTO leave_requests(id,number,employee_id,person_id,company_id,branch_id,department_id,start_date,days,state,created_by,handover_note,created_at,updated_at) VALUES(?,?,?,?,?,?,?,?,1,'approved',?,'Medical secret',1,1)",
    )
    .bind(
      key(),
      key(),
      `emp-${f.claimant.userId}`,
      person(f.claimant),
      f.company,
      f.branch,
      f.department,
      date,
      f.claimant.userId,
    )
    .run();
  const d = await read();
  expect(
    d.items
      .filter((i) => i.kind === "appointment")
      .every((i) => i.warnings.some((w) => w.includes("absence"))),
  ).toBe(true);
  expect(JSON.stringify(d)).not.toContain("Medical secret");
  const leave = d.items.find((i) => i.kind === "leave")!;
  expect(leave.allDay).toBe(true);
  expect(leave.startDate).toBe(date);
  expect(leave.endDate).toBe(addDays(date, 1));
  expect(leave.href).toBeUndefined();
});
it("shares announcement audience/state with the feed and enforces scoped publishing", async () => {
  const a = await command({
    action: "announcement.save",
    audienceId: f.department,
    title: "Team notice",
    content: "Planning day",
    startDate: date,
    endDate: addDays(date, 2),
  });
  expect((await read(f.claimant)).items).toEqual([]);
  await command({
    action: "announcement.status",
    id: a.id,
    expectedVersion: 1,
    status: "published",
  });
  const feed = await publishedAnnouncements(db, f.claimant, date),
    calendar = (await read(f.claimant)).items;
  expect(feed.map((x) => x.sourceId)).toEqual(calendar.map((x) => x.sourceId));
  const limited = { ...f.manager, departmentId: f.accountsDepartment };
  expect((await read(limited)).items).toEqual([]);
  expect(await publishedAnnouncements(db, limited, date)).toEqual([]);
  await expect(
    command(
      {
        action: "announcement.status",
        id: a.id,
        expectedVersion: 2,
        status: "withdrawn",
      },
      limited,
    ),
  ).rejects.toMatchObject({ status: 403 });
  await command({
    action: "announcement.status",
    id: a.id,
    expectedVersion: 2,
    status: "withdrawn",
  });
  expect(await publishedAnnouncements(db, f.claimant, date)).toEqual([]);
});
it("preserves actual organizational nesting and reporting ids without duplicates or Company leakage", async () => {
  await db
    .prepare("UPDATE hr_engagements SET manager_person_id=? WHERE person_id=?")
    .bind(person(f.manager), person(f.claimant))
    .run();
  const h = await calendarHierarchy(db, f.manager);
  expect(h.people.filter((p) => p.id === person(f.claimant))).toHaveLength(1);
  expect(h.nodes.find((n) => n.id === f.department)?.parentId).toBe(f.branch);
  expect(
    h.people.find((p) => p.id === person(f.claimant))?.managerId,
  ).toBeTruthy();
  const other = await seedExpenseCompany(db, env.H001_FILES);
  expect(
    (await calendarHierarchy(db, other.owner)).people.some(
      (p) => p.id === person(f.claimant),
    ),
  ).toBe(false);
  const org = await read(f.manager, { hierarchy: "organization" }),
    reports = await read(f.manager, { hierarchy: "reporting" });
  expect(org.items).toEqual(reports.items);
});
async function preparation(privileged = false) {
  const id = key(),
    c = key(),
    eng = await db
      .prepare(
        "SELECT id FROM hr_engagements WHERE company_id=? AND person_id=? LIMIT 1",
      )
      .bind(f.company, person(f.claimant))
      .first<{ id: string }>();
  await db
    .prepare(
      "INSERT INTO people_lifecycle_cases(id,company_id,person_id,engagement_id,kind,status,version,effective_at,definition,created_by,updated_at) VALUES(?,?,?,?,'offboarding','draft',1,?,'{}',?,1)",
    )
    .bind(
      c,
      f.company,
      person(f.claimant),
      eng!.id,
      Date.parse(`${date}T12:00:00Z`),
      f.owner.userId,
    )
    .run();
  await db
    .prepare(
      "INSERT INTO people_preparation_tasks(id,company_id,case_id,kind,title,updated_at,privileged) VALUES(?,?,?,'lifecycle','Return equipment',1,?)",
    )
    .bind(id, f.company, c, privileged)
    .run();
  return { id, c };
}
it("keeps legacy tasks privileged/undated, supports explicit ordinary assignees, and leaves cutoffs untouched", async () => {
  const hr = await f.member("hr-planner", "hr_admin"),
    t = await preparation();
  let d = await read(hr);
  expect(d.items.find((i) => i.sourceId === t.id)?.startDate).toBeNull();
  await command(
    {
      action: "task.plan",
      id: t.id,
      expectedVersion: 1,
      dueDate: addDays(date, -50),
      assigneeId: f.claimant.userId,
      responsibility: "Return laptop to reception",
    },
    hr,
  );
  d = await read(f.claimant, { mode: "mine" });
  const task = d.items.find((i) => i.sourceId === t.id)!;
  expect(task.warnings).toContain("Overdue preparation task");
  expect(task.href).toBeUndefined();
  expect(task.details?.caseId).toBeUndefined();
  expect(task.permittedActions).toContain("task.complete");
  await command(
    { action: "task.complete", id: t.id, expectedVersion: 2 },
    f.claimant,
  );
  expect(
    await db
      .prepare(
        "SELECT status,completed_by FROM people_preparation_tasks WHERE id=?",
      )
      .bind(t.id)
      .first(),
  ).toMatchObject({ status: "completed", completed_by: f.claimant.userId });
  expect(
    await db
      .prepare("SELECT 1 FROM people_access_cutoffs WHERE case_id=?")
      .bind(t.c)
      .first(),
  ).toBeNull();
  const restricted = await preparation(true);
  await expect(
    command(
      {
        action: "task.plan",
        id: restricted.id,
        expectedVersion: 1,
        dueDate: null,
        assigneeId: f.claimant.userId,
        responsibility: "IT work",
      },
      hr,
    ),
  ).rejects.toMatchObject({ status: 403 });
  await expect(
    command(
      { action: "task.complete", id: restricted.id, expectedVersion: 1 },
      f.claimant,
    ),
  ).rejects.toMatchObject({ status: 403 });
});
it("paginates deterministically and validates dates and nonexistent DST local times", async () => {
  for (let i = 0; i < 3; i++)
    await command(appointment({ title: `Meeting ${i}` }));
  const a = await read(f.manager, { limit: 1 }),
    b = await read(f.manager, { limit: 1, cursor: a.nextCursor });
  expect(a.nextCursor).toBe(1);
  expect(b.items[0].id).not.toBe(a.items[0].id);
  expect(calendarQuerySchema.safeParse({ date: "2026-02-30" }).success).toBe(
    false,
  );
  await expect(
    localInterval(
      db,
      "2027-03-14T02:30",
      "2027-03-14T04:00",
      "America/New_York",
    ),
  ).rejects.toMatchObject({ code: "CALENDAR_TIME" });
});
it("reports failed source loading explicitly", async () => {
  const faulty = {
    prepare: (sql: string) => {
      if (sql.includes("FROM calendar_appointments x"))
        throw new Error("Source temporarily unavailable");
      return db.prepare(sql);
    },
    batch: db.batch.bind(db),
    exec: db.exec.bind(db),
  };
  const d = await calendarData(faulty, f.manager, { date });
  expect(d.sources.find((s) => s.source === "appointments")?.status).toBe(
    "error",
  );
  expect(d.coverage.partial).toBe(true);
});
it("shows future joiners and ended engagement milestones while keeping each cutoff independent", async () => {
  const t = await preparation(),
    hr = await f.member("milestone-hr", "hr_admin");
  await db
    .prepare(
      "UPDATE people_lifecycle_cases SET definition=?::jsonb,kind='offboarding',status='scheduled' WHERE id=?",
    )
    .bind(JSON.stringify({ lastWorkingDate: date }), t.c)
    .run();
  const cutoff = Date.parse(`${addDays(date, 1)}T05:00:00Z`);
  await db
    .prepare(
      "INSERT INTO people_access_cutoffs(company_id,person_id,case_id,effective_at) VALUES(?,?,?,?)",
    )
    .bind(f.company, person(f.claimant), t.c, cutoff)
    .run();
  await db
    .prepare(
      "UPDATE hr_engagements SET status='ended',end_date=? WHERE person_id=?",
    )
    .bind(date, person(f.claimant))
    .run();
  let d = await read(hr);
  const last = d.items.find((i) => i.kind === "last_day")!,
    access = d.items.find((i) => i.kind === "cutoff")!;
  expect(last.startDate).toBe(date);
  expect(last.allDay).toBe(true);
  expect(access.startsAt).toBe(cutoff);
  expect(d.items.some((i) => i.kind === "directory")).toBe(false);
  expect(
    (await calendarHierarchy(db, hr)).people.some(
      (p) => p.id === person(f.claimant),
    ),
  ).toBe(true);
  await command(
    { action: "task.complete", id: t.id, expectedVersion: 1 },
    f.it,
  );
  expect(
    (
      await db
        .prepare(
          "SELECT effective_at FROM people_access_cutoffs WHERE case_id=?",
        )
        .bind(t.c)
        .first<{ effective_at: number }>()
    )?.effective_at,
  ).toBe(cutoff);
  await db
    .prepare(
      "UPDATE people_lifecycle_cases SET kind='onboarding',definition='{}',status='draft' WHERE id=?",
    )
    .bind(t.c)
    .run();
  await db
    .prepare(
      "UPDATE hr_engagements SET status='active',start_date=?,end_date=NULL WHERE person_id=?",
    )
    .bind(date, person(f.claimant))
    .run();
  d = await read(hr);
  expect(
    d.items.some(
      (i) =>
        i.kind === "start" &&
        i.participants.some((p) => p.id === person(f.claimant)),
    ),
  ).toBe(true);
});
it("scopes private detail even when an attendee is in another department and does not search redacted titles", async () => {
  const saved = await command(
    appointment({ participantIds: [person(f.reviewer)] }),
    f.claimant,
  );
  const a = { ...f.manager, departmentId: f.accountsDepartment };
  const d = await read(a);
  expect(d.items.find((i) => i.sourceId === saved.id)?.redacted).toBe(true);
  expect((await read(a, { search: "Private interview" })).items).toEqual([]);
  await expect(
    command(
      {
        action: "appointment.status",
        id: saved.id,
        expectedVersion: 1,
        status: "cancelled",
      },
      a,
    ),
  ).rejects.toMatchObject({ status: 403 });
  const other = await seedExpenseCompany(db, env.H001_FILES);
  await expect(
    read(a, { person: person(other.claimant) }),
  ).rejects.toMatchObject({ status: 403 });
});

it("shows verified directory work in its own week even after the departure window", async () => {
  const t = await preparation(),
    hr = await f.member("directory-calendar-hr", "hr_admin"),
    connection = key();
  const verifiedAt = Date.parse(`${addDays(date, 14)}T05:00:00Z`);
  await db
    .prepare(
      "INSERT INTO iam_connections(id,company_id,name,provider,config,status,version,created_by,created_at) VALUES(?,?,'Calendar test','univention','{}','active',1,?,1)",
    )
    .bind(connection, f.company, f.owner.userId)
    .run();
  await db
    .prepare(
      "INSERT INTO people_directory_operations(id,company_id,connection_id,connection_version,state_version,status,definition,evidence,created_by,assignment_id,created_at,updated_at,result) VALUES(?,?,?,1,1,'completed',?::jsonb,'{}',?,?,1,?,?::jsonb)",
    )
    .bind(
      key(),
      f.company,
      connection,
      JSON.stringify({ action: { kind: "account.disable", caseId: t.c } }),
      f.it.userId,
      f.it.assignmentId,
      verifiedAt,
      JSON.stringify({ verified: true }),
    )
    .run();
  const d = await read(hr, { date: addDays(date, 14) });
  expect(d.sources.every((s) => s.status === "ready")).toBe(true);
  expect(d.items.find((i) => i.kind === "directory")?.startsAt).toBe(
    verifiedAt,
  );
  expect(d.items.some((i) => i.kind === "cutoff")).toBe(false);
});

it("keeps unpurchased Roster rows and occupants out of Calendar while retaining other sources and draining rescheduling", async () => {
  const roster = await shift(), meeting = await command(appointment({organizerPersonId:person(f.manager),participantIds:[person(f.claimant)],title:"Independent Calendar source"}));
  const store=packageGrantStore(db,{authorize:async()=>({userId:"isolated-calendar-purchase-review"}),obligations:async()=>{throw Error("No invented People withdrawal witness");}});
  const grant=async(packageId:"people"|"people-workforce",state:"active"|"draining",expectedVersion:number)=>store.apply({expectedVersion,idempotencyKey:key(),grant:{companyId:f.company,packageId,packageVersion:1,state,startsAt:0,endsAt:null,version:expectedVersion+1,reason:"Declared isolated Roster Calendar purchase",evidence:"test:roster-calendar-purchase"}});
  try {
    vi.stubEnv("ERPX_PEOPLE_PACKAGE_ENFORCEMENT","enforced");
    for(const baseOnly of [false,true]){
      if(baseOnly)await grant("people","active",0);
      const hidden=await read(f.manager,{view:"week"});
      expect(hidden.sources.every(source=>source.status==="ready")).toBe(true);
      expect(hidden.items.some(item=>item.kind==="shift")).toBe(false);
      expect(hidden.items.some(item=>item.sourceId===meeting.id)).toBe(true);
      expect(await calendarRosterPeoplePayrollFacts(db).read2({value1:JSON.stringify([roster.s.id])})).toEqual([]);
    }
    await grant("people-workforce","active",0);
    const active=(await read(f.manager,{view:"week"})).items.find(item=>item.sourceId===roster.s.id);
    expect(active).toMatchObject({capacity:2,assigned:1,participants:[{id:person(f.claimant)}]});
    expect(active?.permittedActions).toEqual(expect.arrayContaining(["roster.open","roster.manage","shift.reschedule"]));
    await grant("people-workforce","draining",1);
    const before=await db.prepare("SELECT count(*) n FROM command_receipts").first("n"),retained=(await read(f.manager,{view:"week"})).items.find(item=>item.sourceId===roster.s.id);
    expect(retained).toMatchObject({capacity:2,assigned:1});
    expect(retained?.permittedActions).toContain("shift.reschedule");
    expect(retained?.permittedActions).not.toContain("roster.manage");
    expect(await db.prepare("SELECT count(*) n FROM command_receipts").first("n")).toBe(before);
    expect(await db.prepare("SELECT count(*) n FROM command_assertions").first("n")).toBe(0);
  } finally {vi.unstubAllEnvs();}
});

async function workCalendarGrant(state:'active'|'draining'='active'){
 const {readCompanyPackageGrants}=await import('@erpx/server-commercial/public/package-grants'),old=(await readCompanyPackageGrants(db,f.company)).find(row=>row.packageId==='work');
 return packageGrantStore(db,{authorize:async()=>({userId:'isolated-calendar-provider'}),obligations:async()=>{throw Error('CALENDAR_TEST_CANNOT_FABRICATE_ALL_WORK_CLOSURE');}}).apply({expectedVersion:old?.version??0,idempotencyKey:key(),grant:{companyId:f.company,packageId:'work',packageVersion:1,state,startsAt:0,endsAt:null,version:(old?.version??0)+1,reason:'Actual original Calendar owner fixture',evidence:'test:work-calendar-owner'}});
}
it('purchasedCalendar keeps manual records and commands on Work while actual existing settlement and exact replay remain available during draining',async()=>{
 const saved=await command(appointment(),f.claimant),receipt=key(),newInput=appointment({title:'Actual purchased Calendar meeting'});vi.stubEnv('ERPX_APPS_PACKAGE_ENFORCEMENT','enforced');
 const hidden=await read(f.claimant,{layers:'appointments,announcements'});expect(hidden.sources.every(source=>source.status==='ready')).toBe(true);expect(hidden.items).toEqual([]);expect(hidden).toMatchObject({canSchedule:false,canAnnounce:false,purchase:{newWork:false,history:false}});await expect(command(newInput,f.claimant,receipt)).rejects.toMatchObject({code:'PACKAGE_REQUIRED'});
 await workCalendarGrant();const created=await command(newInput,f.claimant,receipt);expect(await command(newInput,f.claimant,receipt)).toEqual(created);const active=await read(f.claimant,{layers:'appointments'});expect(active.items.map(item=>item.sourceId)).toEqual(expect.arrayContaining([saved.id,created.id]));expect(active.purchase).toMatchObject({newWork:true,settleExisting:true,history:true});
 await workCalendarGrant('draining');await expect(command(appointment({title:'Unlicensed new draining intake'}),f.claimant)).rejects.toMatchObject({code:'PACKAGE_DRAINING'});const retained=await read(f.claimant,{layers:'appointments'}),item=retained.items.find(row=>row.sourceId===created.id)!;expect(item.permittedActions).not.toContain('appointment.save');expect(item.permittedActions).toContain('appointment.reschedule');expect(retained.canSchedule).toBe(false);
 const moved=await command({action:'appointment.reschedule',id:created.id,expectedVersion:created.version,startLocal:`${date}T11:00`,endLocal:`${date}T12:00`,timezone:'Asia/Bangkok'},f.claimant);await command({action:'appointment.respond',id:created.id,expectedVersion:moved.version,response:'accepted'},f.tie);await command({action:'appointment.reminder.set',id:created.id,minutes:null},f.tie);await expect(command({action:'appointment.reminder.set',id:created.id,minutes:30},f.tie)).rejects.toMatchObject({code:'PACKAGE_DRAINING'});expect(await command(newInput,f.claimant,receipt)).toEqual(created);
});

it('purchasedCalendar repeats actual completed source and current actor after deferred effects and rolls the whole original source history notice and receipt back before unchanged retry',async()=>{
 await workCalendarGrant();vi.stubEnv('ERPX_APPS_PACKAGE_ENFORCEMENT','enforced');
 for(const kind of ['source','iam'] as const){const commandId=key(),id=`calendar-${commandId}`,input=appointment({title:`Original Calendar ${kind} deferred proof`}),fault=`calendar_final_${kind}_${key().replaceAll('-','')}`,effect=kind==='source'?`UPDATE calendar_appointments SET title='Unreviewed deferred private meeting' WHERE id='${id}'`:`UPDATE assignments SET revoked_at=1,version=version+1 WHERE id='${f.claimant.assignmentId}'`;
  await db.exec(`CREATE FUNCTION ${fault}() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN ${effect};RETURN NEW;END $$;CREATE CONSTRAINT TRIGGER ${fault} AFTER INSERT ON command_receipts DEFERRABLE INITIALLY DEFERRED FOR EACH ROW WHEN(NEW.command_id='${commandId}') EXECUTE FUNCTION ${fault}()`);
  try{await expect(command(input,f.claimant,commandId)).rejects.toMatchObject(kind==='source'?{code:'CALENDAR_SOURCE_CHANGED'}:{code:'CALENDAR_AUTHORITY_CHANGED'});}finally{await db.exec(`DROP TRIGGER ${fault} ON command_receipts;DROP FUNCTION ${fault}()`);}
  expect(await db.prepare('SELECT id FROM calendar_appointments WHERE id=?').bind(id).first()).toBeNull();expect(await db.prepare('SELECT record_id FROM calendar_history WHERE record_id=?').bind(id).first()).toBeNull();expect(await db.prepare('SELECT command_id FROM command_receipts WHERE command_id=?').bind(commandId).first()).toBeNull();expect(await db.prepare('SELECT revoked_at,version FROM assignments WHERE id=?').bind(f.claimant.assignmentId).first()).toEqual({revoked_at:null,version:1});
  const actual=await command(input,f.claimant,commandId);expect(await command(input,f.claimant,commandId)).toEqual(actual);expect(await db.prepare('SELECT title FROM calendar_appointments WHERE id=?').bind(id).first('title')).toBe(input.title);
 }
});

it('purchasedCalendar preserves the original independent Roster projection and shift action without buying Work manual Calendar',async()=>{
 const roster=await shift();vi.stubEnv('ERPX_APPS_PACKAGE_ENFORCEMENT','enforced');const independent=await read(f.manager,{layers:'roster,appointments',view:'week'});expect(independent.sources.every(source=>source.status==='ready')).toBe(true);expect(independent.items.find(item=>item.sourceId===roster.s.id)).toMatchObject({kind:'shift',capacity:2,assigned:1});expect(independent.canSchedule).toBe(false);expect(independent.items.find(item=>item.sourceId===roster.s.id)?.permittedActions).toContain('shift.reschedule');
});

it('purchasedCalendarReminder captures a real account without HR and delivers original reminders once while draining without authorizing legacy queues',async()=>{
 const legacy=await command(appointment({organizerPersonId:undefined,participantIds:[]}),f.claimant),account=await f.member('calendar-account-only','employee',1,f.department,false);await workCalendarGrant();vi.stubEnv('ERPX_APPS_PACKAGE_ENFORCEMENT','enforced');
 expect(await db.prepare('SELECT 1 FROM hr_people WHERE user_id=?').bind(account.userId).first()).toBeNull();
 const created=await command(appointment({organizerPersonId:undefined,participantIds:[],title:'Account-only queued original meeting'}),account),source=await db.prepare('SELECT starts_at FROM calendar_appointments WHERE id=?').bind(created.id).first<{starts_at:number}>(),due=source!.starts_at-15*60000;
 const captured=await db.prepare('SELECT actor_id,assignment_id,recipient_id,source_version FROM calendar_reminder_execution_authorizations WHERE appointment_id=?').bind(created.id).first();expect(captured).toEqual({actor_id:account.userId,assignment_id:account.assignmentId,recipient_id:account.userId,source_version:created.version});await workCalendarGrant('draining');
 expect(await drainCalendarReminders(db,due)).toBeGreaterThanOrEqual(1);expect(await drainCalendarReminders(db,due)).toBe(0);expect(await db.prepare('SELECT count(*) n FROM calendar_reminder_deliveries WHERE appointment_id=? AND delivered_at IS NOT NULL').bind(created.id).first('n')).toBe(1);expect(await db.prepare('SELECT 1 FROM calendar_reminder_deliveries WHERE appointment_id=?').bind(legacy.id).first()).toBeNull();
 expect(await db.prepare("SELECT source_version,preview FROM inbox_events WHERE source_id=? AND event_key LIKE 'reminder:%'").bind(created.id).first()).toEqual({source_version:created.version,preview:'Meeting soon: Account-only queued original meeting'});
 await db.transaction(async tx=>{await expect(drainCalendarReminders(tx,due)).rejects.toMatchObject({code:'CALENDAR_REMINDER_UNIT_REQUIRED'});});
});

it('purchasedCalendarReminder rolls private notification and ledger back after real deferred source or original queued authority revocation then permits unchanged delivery',async()=>{
 await workCalendarGrant();vi.stubEnv('ERPX_APPS_PACKAGE_ENFORCEMENT','enforced');
 for(const fault of ['source','actor','recipient'] as const){const account=await f.member(`reminder-${fault}-account`,'employee',1,f.department,false),recipient=await f.member(`reminder-${fault}-recipient`,'employee',1,f.department,false),saved=await command(appointment({organizerPersonId:undefined,participantIds:[],attendees:[{userId:recipient.userId,attendance:'required'}],title:`Retained private ${fault} reminder`}),account),source=await db.prepare('SELECT starts_at FROM calendar_appointments WHERE id=?').bind(saved.id).first<{starts_at:number}>(),due=source!.starts_at-15*60000,name=`reminder_final_${key().replaceAll('-','')}`,effect=fault==='source'?`UPDATE calendar_appointments SET title='Private changed after delivery' WHERE id='${saved.id}'`:`UPDATE assignments SET revoked_at=1,version=version+1 WHERE id='${fault==='actor'?account.assignmentId:recipient.assignmentId}'`;
  await db.exec(`CREATE FUNCTION ${name}() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN ${effect};RETURN NEW;END $$;CREATE CONSTRAINT TRIGGER ${name} AFTER INSERT ON inbox_events DEFERRABLE INITIALLY DEFERRED FOR EACH ROW WHEN(NEW.source_id='${saved.id}' AND NEW.event_key LIKE 'reminder:%'${fault==='recipient'?` AND NEW.event_key LIKE '%:${recipient.userId}:%'`:''}) EXECUTE FUNCTION ${name}()`);
  try{await drainCalendarReminders(db,due);}finally{await db.exec(`DROP TRIGGER ${name} ON inbox_events;DROP FUNCTION ${name}()`);}
  expect(await db.prepare("SELECT 1 FROM inbox_events event JOIN inbox_deliveries delivery ON delivery.event_id=event.id WHERE event.source_id=? AND event.event_key LIKE 'reminder:%' AND delivery.user_id=?").bind(saved.id,recipient.userId).first()).toBeNull();expect(await db.prepare('SELECT 1 FROM calendar_reminder_deliveries WHERE appointment_id=? AND user_id=?').bind(saved.id,recipient.userId).first()).toBeNull();expect(await db.prepare('SELECT title FROM calendar_appointments WHERE id=?').bind(saved.id).first('title')).toBe(`Retained private ${fault} reminder`);
  expect(await db.prepare('SELECT revoked_at,version FROM assignments WHERE id=?').bind(fault==='actor'?account.assignmentId:recipient.assignmentId).first()).toEqual({revoked_at:null,version:1});expect(await drainCalendarReminders(db,due)).toBeGreaterThanOrEqual(1);expect(await drainCalendarReminders(db,due)).toBe(0);
 }
});

it('purchasedCalendarReminder accepts normal response and future-series lineage but rejects an unwitnessed source version before sending',async()=>{
 await workCalendarGrant();vi.stubEnv('ERPX_APPS_PACKAGE_ENFORCEMENT','enforced');const organizer=await f.member('calendar-lineage-account','employee',1,f.department,false),saved=await command(appointment({organizerPersonId:undefined,participantIds:[],attendees:[{userId:f.tie.userId,attendance:'required'}],title:'Version-witness meeting'}),organizer),responded=await command({action:'appointment.respond',id:saved.id,expectedVersion:saved.version,response:'accepted'},f.tie),source=await db.prepare('SELECT starts_at FROM calendar_appointments WHERE id=?').bind(saved.id).first<{starts_at:number}>(),due=source!.starts_at-15*60000;
 await db.prepare('UPDATE calendar_appointments SET version=version+1 WHERE id=?').bind(saved.id).run();await drainCalendarReminders(db,due);expect(await db.prepare("SELECT 1 FROM inbox_events WHERE source_id=? AND event_key LIKE 'reminder:%'").bind(saved.id).first()).toBeNull();expect(await db.prepare('SELECT 1 FROM calendar_reminder_deliveries WHERE appointment_id=?').bind(saved.id).first()).toBeNull();await db.prepare('UPDATE calendar_appointments SET version=? WHERE id=?').bind(responded.version,saved.id).run();expect(await drainCalendarReminders(db,due)).toBeGreaterThanOrEqual(1);expect((await db.prepare("SELECT DISTINCT source_version FROM inbox_events WHERE source_id=? AND event_key LIKE 'reminder:%'").bind(saved.id).all<{source_version:number}>()).results).toEqual([{source_version:responded.version}]);
 const series=await command(appointment({organizerPersonId:undefined,participantIds:[],title:'Original future split',recurrence:{frequency:'daily',intervalCount:1,weekdays:[],endKind:'count',occurrenceCount:3}}),organizer),future=addDays(date,1),splitKey=key();await command({action:'appointment.reschedule',id:series.id,expectedVersion:series.version,occurrenceId:`${future}T09:00`,scope:'future',startLocal:`${future}T11:00`,endLocal:`${future}T12:00`,timezone:'Asia/Bangkok'},organizer,splitKey);const splitId=`calendar-${splitKey}`,split=await db.prepare('SELECT starts_at,version FROM calendar_appointments WHERE id=?').bind(splitId).first<{starts_at:number;version:number}>();expect(await drainCalendarReminders(db,split!.starts_at-15*60000)).toBeGreaterThanOrEqual(1);expect(await db.prepare("SELECT source_version FROM inbox_events WHERE source_id=? AND event_key LIKE 'reminder:%'").bind(splitId).first('source_version')).toBe(split!.version);
});


it('purchasedCalendarReminder upgrades populated1053 history without fabricating queued authority and preserves disabled schema-compatible owner execution',async()=>{
 const database=env.H002_DB,oldRoot=await mkdtemp(resolve('.runtime/tests/calendar-before-')),currentRoot=await mkdtemp(resolve('.runtime/tests/calendar-current-')),source=resolve('migrations/postgres/holding-write');
 try{for(const directory of [oldRoot,currentRoot])await mkdir(join(directory,'holding-write'),{recursive:true});for(const file of (await readdir(source)).filter(name=>name.endsWith('.sql')&&Number(name.slice(0,4))<=1054)){await cp(join(source,file),join(currentRoot,'holding-write',file));if(Number(file.slice(0,4))<=1053)await cp(join(source,file),join(oldRoot,'holding-write',file));}
  await migrateDatabase(database,'holding-write',oldRoot);const old=await seedExpenseCompany(database,env.H002_FILES),input=appointment({organizerPersonId:undefined,participantIds:[],title:'Retained pre1054 Calendar source'}),original=await calendarCommand(database,old.claimant,input,key());expect(await database.prepare("SELECT to_regclass('calendar_reminder_execution_authorizations') relation").first('relation')).toBeNull();
  const tables=(await database.prepare("SELECT tablename FROM pg_tables WHERE schemaname='public' AND tablename<>'erpx_migrations' ORDER BY tablename").all<{tablename:string}>()).results.map(row=>row.tablename),snapshot=async()=>{const rows=[];for(const table of tables){if(!/^[a-z][a-z0-9_]*$/.test(table))throw Error('DECLARED_CALENDAR_SCHEMA_TABLE_REQUIRED');rows.push({table,...await database.prepare(`SELECT count(*) rows,md5(coalesce(jsonb_agg(to_jsonb(t) ORDER BY to_jsonb(t)::text),'[]'::jsonb)::text) digest FROM "${table}" t${table==='work_cache_revision_baselines'?" WHERE table_name<>'calendar_reminder_execution_authorizations'":''}`).first()});}return rows;},before=await snapshot(),receipts=(await database.prepare('SELECT name,checksum FROM erpx_migrations ORDER BY name').all()).results;
  await migrateDatabase(database,'holding-write',currentRoot);expect(await snapshot()).toEqual(before);expect((await database.prepare("SELECT name,checksum FROM erpx_migrations WHERE name NOT LIKE '1054_%' ORDER BY name").all()).results).toEqual(receipts);expect(await database.prepare('SELECT count(*) count FROM calendar_reminder_execution_authorizations').first('count')).toBe(0);
  const sourceRow=await database.prepare('SELECT starts_at FROM calendar_appointments WHERE id=?').bind(original.id).first<{starts_at:number}>(),due=sourceRow!.starts_at-15*60000;vi.stubEnv('ERPX_APPS_PACKAGE_ENFORCEMENT','enforced');expect(await drainCalendarReminders(database,due)).toBe(0);expect(await database.prepare('SELECT 1 FROM calendar_reminder_deliveries WHERE appointment_id=?').bind(original.id).first()).toBeNull();vi.stubEnv('ERPX_APPS_PACKAGE_ENFORCEMENT','disabled');await migrateDatabase(database,'holding-write',oldRoot);expect(await drainCalendarReminders(database,due)).toBe(1);expect(await database.prepare('SELECT count(*) count FROM calendar_reminder_execution_authorizations').first('count')).toBe(0);expect(await calendarCommand(database,old.claimant,{action:'appointment.status',id:original.id,expectedVersion:original.version,status:'cancelled'},key())).toMatchObject({id:original.id,status:'cancelled'});expect(await database.prepare('SELECT count(*) count FROM company_package_grants').first('count')).toBe(0);
 }finally{await rm(oldRoot,{recursive:true,force:true});await rm(currentRoot,{recursive:true,force:true});}
});
