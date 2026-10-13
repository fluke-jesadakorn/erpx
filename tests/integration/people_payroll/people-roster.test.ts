import { beforeAll, beforeEach, expect, it, vi } from "vitest";
import { env, applyMigrations } from "../../support/runtime-env";
import { seedExpenseCompany } from "../../../fixtures/expenses/company";
import { escalateRosterCoverage, rosterCommand, rosterData, rosterNotificationLink } from "../../../src/server/modules/people_payroll/public/people-roster";
import { rosterMonday, addDays } from "../../../src/lib/people_payroll/people-roster";
import { rosterPositionExitStatements } from "../../../src/server/modules/people_payroll/public/roster-position-exit";
import type { WorkContext } from "../../../src/server/modules/identity_access/public/authorization";
import { prepareLocalBrowserRosterExample } from "../../../fixtures/people_payroll/builders/local-browser-roster-directory";
vi.mock("../../../src/server/modules/runtime/adapters/runtime", () => ({ holdingResources: async () => ({ write: env.H001_DB, files: env.H001_FILES }), runtimeEnv: async () => env }));
const db = env.H001_DB, key = () => crypto.randomUUID();
let f: Awaited<ReturnType<typeof seedExpenseCompany>>;
const monday = addDays(rosterMonday(), 14);
const person = (a: WorkContext) => `person-${a.userId}`;
const command = (input: unknown, actor = f.manager, id = key()) => rosterCommand(db, actor, input, id);
const read = (actor = f.manager, department = f.department, start = monday) => rosterData(db, actor, { department, week: start });
async function createWeek(start = monday, department = f.department, timezone = "Asia/Bangkok") {
  return command({ action: "week.create", departmentId: department, weekStart: start, timezone });
}
async function addShift(weekId: string, names: WorkContext[], date = monday, start = "06:00", end = "14:00", capacity = 2) {
  const w = await db.prepare("SELECT version FROM people_roster_weeks WHERE id=?").bind(weekId).first<{ version: number }>();
  return command({ action: "shift.save", weekId, expectedVersion: w!.version, date, name: `${start} shift`, startTime: start, endTime: end, capacity, personIds: names.map(person), reason: "Roster planning" });
}
async function publish(id: string) {
  const existing=await db.prepare(`SELECT DISTINCT a.person_id FROM people_roster_assignments a JOIN people_roster_shifts s ON s.id=a.shift_id WHERE s.week_id=? AND a.state='assigned'`).bind(id).all<{person_id:string}>();
  const missing=[f.claimant,f.tie].filter(actor=>!existing.results.some(row=>row.person_id===person(actor)));
  if(missing.length) await addShift(id,missing,addDays(monday,4),"07:00","15:00");
  const w = await db.prepare("SELECT version FROM people_roster_weeks WHERE id=?").bind(id).first<{ version: number }>();
  return command({ action: "week.publish", id, expectedVersion: w!.version });
}
async function setup() {
  const w = await createWeek();
  const a = await addShift(w.id, [f.claimant]);
  const b = await addShift(w.id, [f.tie], monday, "14:00", "22:00");
  const c = await addShift(w.id, [], addDays(monday, 1));
  await publish(w.id);
  const data = await read();
  const aa = data.assignments.find(x => x.shift_id === a.id)!;
  const bb = data.assignments.find(x => x.shift_id === b.id)!;
  return { w, a, b, c, aa, bb };
}
async function swap(s: Awaited<ReturnType<typeof setup>>) {
  return command({ action: "request.create", kind: "swap", sourceAssignmentId: s.aa.id, targetShiftId: s.b.id, targetAssignmentId: s.bb.id, reason: "Appointment in the morning" }, f.claimant);
}
beforeAll(async () => { await applyMigrations(db, env.STAFF_TEST_MIGRATIONS); });
beforeEach(async () => {
  f = await seedExpenseCompany(db, env.H001_FILES); f.manager = await f.member("roster-manager", "people_manager");
  const id=`${f.company}-shift-position`;
  await db.prepare("INSERT INTO organization_positions(id,company_id,branch_id,department_id,title,level_id,roster_required,created_by,created_at,updated_at) VALUES(?,?,?,?,?,?,1,'fixture',1,1)")
    .bind(id,f.company,f.branch,f.department,"Roster staff",`${f.company}-level-1`).run();
  await db.prepare("UPDATE hr_engagements SET position_id=? WHERE person_id=ANY(?::text[])").bind(id,`{${[person(f.claimant),person(f.tie)].join(',')}}`).run();
});

it("keeps roster reads side-effect free and deduplicates scheduled coverage alerts", async () => {
  await read();
  const count = () => db.prepare("SELECT count(*)::int n FROM staff_notifications WHERE record_type='roster_coverage'").first<{ n: number }>();
  expect((await count())?.n).toBe(0);
  expect((await escalateRosterCoverage(db, monday)).missing).toBeGreaterThan(0);
  const sent = (await count())?.n;
  expect(sent).toBeGreaterThan(0);
  await escalateRosterCoverage(db, monday);
  expect((await count())?.n).toBe(sent);
});

it("reports coverage gaps for shift staff whose account is not linked yet", async () => {
  await db.prepare("UPDATE hr_people SET user_id=NULL WHERE id=?").bind(person(f.tie)).run();
  const w = await createWeek();
  await addShift(w.id, [f.claimant]);
  const version = (await db.prepare("SELECT version FROM people_roster_weeks WHERE id=?").bind(w.id).first<{ version: number }>())!.version;
  await expect(command({ action: "week.publish", id: w.id, expectedVersion: version })).rejects.toMatchObject({ code: "ROSTER_COVERAGE" });
});

it("keeps drafts private and publishes copied templates without changing their snapshots", async () => {
  const t = await command({ action: "template.save", departmentId: f.department, name: "Morning", startTime: "06:00", endTime: "14:00", capacity: 2 });
  const w = await createWeek();
  await command({ action: "shift.save", weekId: w.id, expectedVersion: 1, templateId: t.id, date: monday, name: "Morning", startTime: "06:00", endTime: "14:00", capacity: 2, personIds: [person(f.claimant)] });
  expect((await read(f.claimant)).week).toBeNull();
  expect((await read(f.claimant)).shifts).toEqual([]);
  await publish(w.id);
  await command({ action: "template.save", id: t.id, expectedVersion: 1, departmentId: f.department, name: "Later", startTime: "07:00", endTime: "15:00", capacity: 3 });
  expect((await read(f.claimant)).shifts[0].start_time).toBe("06:00");
  const copy = await command({ action: "week.create", departmentId: f.department, weekStart: addDays(monday, 7), timezone: "Asia/Bangkok", copyWeekId: w.id });
  const d = await read(f.manager, f.department, addDays(monday, 7));
  expect(d.week?.id).toBe(copy.id); expect(d.week?.state).toBe("draft");
  const shifts = d.shifts.filter(s => s.week_id === copy.id);
  expect(shifts).toHaveLength(2); expect(shifts[0].start_time).toBe("06:00");
  expect(d.assignments.filter(a => a.shift_id === shifts[0].id)).toHaveLength(1);
  expect(d.requests).toEqual([]);
  expect(await rosterNotificationLink(db, f.claimant, "roster_week", w.id)).toContain("/people/roster?");
});
it("requires coworker acceptance and swaps both assignments only on independent approval", async () => {
  const s = await setup(), r = await swap(s);
  await expect(command({ action: "request.decide", id: r.id, expectedVersion: 1, decision: "approve" })).rejects.toMatchObject({ code: "ROSTER_COWORKER_REQUIRED" });
  expect((await read()).assignments.find(a => a.id === s.aa.id)?.state).toBe("assigned");
  await command({ action: "request.decide", id: r.id, expectedVersion: 1, decision: "accept" }, f.tie);
  const before = await read(); expect(before.requests[0].state).toBe("pending_manager");
  await command({ action: "request.decide", id: r.id, expectedVersion: 2, decision: "approve" });
  const after = await read();
  expect(after.requests[0].state).toBe("approved");
  expect(after.assignments.find(a => a.shift_id === s.b.id && a.person_id === person(f.claimant))?.state).toBe("assigned");
  expect(after.assignments.find(a => a.shift_id === s.a.id && a.person_id === person(f.tie))?.state).toBe("assigned");
  expect((await db.prepare("SELECT count(*)::int n FROM people_roster_history WHERE request_id=?").bind(r.id).first<{ n: number }>())?.n).toBe(3);
});
it("supports extra shifts, moving an assignment, rejection and withdrawal", async () => {
  const s = await setup();
  const extra = await command({ action: "request.create", kind: "extra", targetShiftId: s.c.id, reason: "Available to help" }, f.claimant);
  await command({ action: "request.decide", id: extra.id, expectedVersion: 1, decision: "approve" });
  expect((await read()).assignments.some(a => a.shift_id === s.c.id && a.person_id === person(f.claimant))).toBe(true);
  const move = await command({ action: "request.create", kind: "move", sourceAssignmentId: s.aa.id, targetShiftId: s.b.id, reason: "Prefer afternoon" }, f.claimant);
  await command({ action: "request.decide", id: move.id, expectedVersion: 1, decision: "approve" });
  const d = await read(); expect(d.assignments.find(a => a.id === s.aa.id)?.state).toBe("released");
  const rejected = await command({ action: "request.create", kind: "extra", targetShiftId: s.a.id, reason: "Extra morning work" }, f.tie);
  await expect(command({ action: "request.decide", id: rejected.id, expectedVersion: 1, decision: "reject" })).rejects.toMatchObject({ code: "ROSTER_REASON" });
  await command({ action: "request.decide", id: rejected.id, expectedVersion: 1, decision: "reject", reason: "Already enough coverage" });
  const withdrawn = await command({ action: "request.create", kind: "extra", targetShiftId: s.a.id, reason: "Extra morning work" }, f.tie);
  await command({ action: "request.decide", id: withdrawn.id, expectedVersion: 1, decision: "withdraw" }, f.tie);
  expect((await read()).requests.map(r => r.state)).toContain("withdrawn");
});
it("blocks self approval, unauthorized managers, cross-department and cross-company access", async () => {
  const s = await setup();
  await expect(command({ action: "week.create", departmentId: f.department, weekStart: addDays(monday, 7) }, f.claimant)).rejects.toMatchObject({ code: "PERMISSION_REQUIRED" });
  await expect(command({ action: "request.create", kind: "extra", targetShiftId: s.c.id, reason: "I can cover this" }, f.manager)).rejects.toMatchObject({ code: "ROSTER_STAFF" });
  const outsider = await f.member("outside", "employee", 1, f.accountsDepartment);
  await expect(read(outsider)).rejects.toMatchObject({ code: "ROSTER_SCOPE" });
  await expect(command({ action: "request.create", kind: "extra", targetShiftId: s.c.id, reason: "Wrong department" }, outsider)).rejects.toMatchObject({ code: "ROSTER_STAFF" });
  const other = await seedExpenseCompany(db, env.H001_FILES);
  await expect(command({ action: "shift.cancel", id: s.a.id, expectedVersion: 1, reason: "Wrong tenant" }, other.owner)).rejects.toMatchObject({ code: "NOT_FOUND" });
  const limited = { ...f.manager, departmentId: f.accountsDepartment, scope: { holdingId:f.manager.holdingId,holdingSlug:f.manager.holdingSlug,holdingName:f.manager.holdingName,companyId:f.company,branchId:f.manager.branchId, kind: "department" as const, departmentId: f.accountsDepartment, departmentSlug: "accounts", departmentName: "Accounts" } };
  await expect(read(limited)).rejects.toMatchObject({ code: "ROSTER_SCOPE" });
});
it("invalidates requests after a source shift edit and requires a published-change reason", async () => {
  const s = await setup(), r = await swap(s), d = await read();
  const edit = { action: "shift.save", id: s.a.id, weekId: s.w.id, expectedVersion: d.week!.version, date: monday, name: "Earlier", startTime: "05:00", endTime: "13:00", capacity: 2, personIds: [person(f.claimant)] };
  await expect(command(edit)).rejects.toMatchObject({ code: "ROSTER_REASON" });
  await command({ ...edit, reason: "Start earlier for delivery" });
  expect((await read()).requests.find(x => x.id === r.id)?.state).toBe("invalidated");
  await expect(command({ action: "request.decide", id: r.id, expectedVersion: 1, decision: "accept" }, f.tie)).rejects.toMatchObject({ code: "ROSTER_CHANGED" });
});
it("checks overnight intervals, approved leave, and engagement end dates", async () => {
  const w = await createWeek();
  const night = await addShift(w.id, [f.claimant], monday, "22:00", "06:00");
  const d = await read(), s = d.shifts.find(s => s.id === night.id)!;
  expect(s.ends_at - s.starts_at).toBe(8 * 3600000);
  await expect(addShift(w.id, [f.claimant], addDays(monday, 1), "05:00", "13:00")).rejects.toMatchObject({ code: "ROSTER_CONFLICT" });
  await db.prepare("UPDATE hr_engagements SET end_date=? WHERE person_id=?").bind(monday, person(f.claimant)).run();
  await expect(publish(w.id)).rejects.toMatchObject({ code: "ROSTER_CONFLICT" });
  await db.prepare("UPDATE hr_engagements SET end_date=NULL WHERE person_id=?").bind(person(f.claimant)).run();
  await db.prepare("INSERT INTO employees(id,user_id,company_id,branch_id,department_id,person_id,display_name,status,created_at) VALUES(?,?,?,?,?,?,'Test','active',1)").bind(`emp-${f.claimant.userId}`, f.claimant.userId, f.company, f.branch, f.department, person(f.claimant)).run();
  await db.prepare("INSERT INTO leave_requests(id,number,employee_id,person_id,company_id,branch_id,start_date,days,state,created_by,created_at,updated_at) VALUES(?,?,?,?,?,?,?,1,'approved',?,1,1)").bind(key(), key(), `emp-${f.claimant.userId}`, person(f.claimant), f.company, f.branch, addDays(monday, 1), f.claimant.userId).run();
  await expect(publish(w.id)).rejects.toMatchObject({ code: "ROSTER_CONFLICT" });
  expect((await read()).conflicts[0].message).toContain("Approved leave");
});
it("flags leave approved after publication, including legacy employee-linked leave", async () => {
  const s = await setup();
  await db.prepare("INSERT INTO employees(id,user_id,company_id,branch_id,department_id,person_id,display_name,status,created_at) VALUES(?,?,?,?,?,?,'Test','active',1)").bind(`emp-${f.claimant.userId}`, f.claimant.userId, f.company, f.branch, f.department, person(f.claimant)).run();
  await db.prepare("INSERT INTO leave_requests(id,number,employee_id,company_id,branch_id,start_date,days,state,created_by,created_at,updated_at) VALUES(?,?,?,?,?,?,1,'approved',?,1,1)").bind(key(), key(), `emp-${f.claimant.userId}`, f.company, f.branch, monday, f.claimant.userId).run();
  const data = await read();
  expect(data.week?.state).toBe("published");
  expect(data.conflicts).toContainEqual(expect.objectContaining({ shiftId: s.a.id, personId: person(f.claimant), message: expect.stringContaining("Approved leave") }));
  expect(data.assignments.find(a => a.id === s.aa.id)?.state).toBe("assigned");
});
it("revalidates eligibility when a coworker accepts", async () => {
  const s = await setup(), r = await swap(s);
  await db.prepare("UPDATE hr_engagements SET status='ended' WHERE person_id=?").bind(person(f.claimant)).run();
  await expect(command({ action: "request.decide", id: r.id, expectedVersion: 1, decision: "accept" }, f.tie)).rejects.toMatchObject({ code: "ROSTER_CONFLICT" });
});
it("serializes competing approvals for the last slot", async () => {
  const w = await createWeek(), s = await addShift(w.id, [], monday, "06:00", "14:00", 1); await publish(w.id);
  const a = await command({ action: "request.create", kind: "extra", targetShiftId: s.id, reason: "Cover morning" }, f.claimant);
  const b = await command({ action: "request.create", kind: "extra", targetShiftId: s.id, reason: "Cover morning" }, f.tie);
  const results = await Promise.allSettled([a, b].map(r => command({ action: "request.decide", id: r.id, expectedVersion: 1, decision: "approve" })));
  expect(results.filter(r => r.status === "fulfilled")).toHaveLength(1);
  expect(results.filter(r => r.status === "rejected")).toHaveLength(1);
  expect((await read()).assignments.filter(a => a.shift_id === s.id && a.state === "assigned")).toHaveLength(1);
});
it("serializes concurrent overlapping assignments in different weeks and preserves command replay", async () => {
  const a = await createWeek(), b = await createWeek(addDays(monday, 7));
  const results = await Promise.allSettled([addShift(a.id, [f.claimant], addDays(monday, 6), "22:00", "06:00"), addShift(b.id, [f.claimant], addDays(monday, 7), "05:00", "13:00")]);
  expect(results.filter(r => r.status === "fulfilled")).toHaveLength(1);
  const id = key(), input = { action: "template.save", departmentId: f.department, name: "Repeat", startTime: "07:00", endTime: "15:00", capacity: 1 };
  expect(await command(input, f.manager, id)).toEqual(await command(input, f.manager, id));
  await expect(command({ ...input, name: "Changed" }, f.manager, id)).rejects.toMatchObject({ code: "IDEMPOTENCY_CONFLICT" });
});
it("lets coworkers reject, releases cancelled shifts, and prevents stale edits", async () => {
  const s = await setup(), r = await swap(s);
  await command({ action: "request.decide", id: r.id, expectedVersion: 1, decision: "reject", reason: "Cannot cover morning" }, f.tie);
  await expect(command({ action: "shift.cancel", id: s.a.id, expectedVersion: 1, reason: "Branch closes early" })).rejects.toMatchObject({code:"ROSTER_COVERAGE"});
  await addShift(s.w.id,[f.claimant],addDays(monday,2));
  await command({ action: "shift.cancel", id: s.a.id, expectedVersion: 1, reason: "Branch closes early" });
  const d = await read();
  expect(d.shifts.find(x => x.id === s.a.id)?.state).toBe("cancelled");
  expect(d.assignments.filter(a => a.shift_id === s.a.id && a.state === "assigned")).toHaveLength(0);
  await expect(command({ action: "shift.cancel", id: s.a.id, expectedVersion: 1, reason: "Repeated" })).rejects.toMatchObject({ code: "ROSTER_CHANGED" });
});
it("requires each roster Position holder before publication and excuses approved full-week leave", async () => {
  const w=await createWeek();
  await addShift(w.id,[f.claimant]);
  const version=(await db.prepare("SELECT version FROM people_roster_weeks WHERE id=?").bind(w.id).first<{version:number}>())!.version;
  await expect(command({action:"week.publish",id:w.id,expectedVersion:version})).rejects.toMatchObject({code:"ROSTER_COVERAGE"});
  await db.prepare("INSERT INTO employees(id,user_id,company_id,branch_id,department_id,person_id,display_name,status,created_at) VALUES(?,?,?,?,?,?,'Test','active',1)")
    .bind(`emp-${f.tie.userId}`,f.tie.userId,f.company,f.branch,f.department,person(f.tie)).run();
  await db.prepare("INSERT INTO leave_requests(id,number,employee_id,person_id,company_id,branch_id,start_date,end_date,days,state,created_by,created_at,updated_at) VALUES(?,?,?,?,?,?,?,?,5,'approved',?,1,1)")
    .bind(key(),key(),`emp-${f.tie.userId}`,person(f.tie),f.company,f.branch,monday,addDays(monday,4),f.tie.userId).run();
  await command({action:"week.publish",id:w.id,expectedVersion:version});
});
it("includes staff who start midweek in publication coverage", async () => {
  await db.prepare("UPDATE hr_engagements SET start_date=? WHERE person_id=?")
    .bind(addDays(monday,2),person(f.claimant)).run();
  const w=await createWeek();
  await addShift(w.id,[f.tie]);
  let version=(await db.prepare("SELECT version FROM people_roster_weeks WHERE id=?").bind(w.id).first<{version:number}>())!.version;
  await expect(command({action:"week.publish",id:w.id,expectedVersion:version})).rejects.toMatchObject({code:"ROSTER_COVERAGE"});
  await addShift(w.id,[f.claimant],addDays(monday,3));
  version=(await db.prepare("SELECT version FROM people_roster_weeks WHERE id=?").bind(w.id).first<{version:number}>())!.version;
  await command({action:"week.publish",id:w.id,expectedVersion:version});
});
it("includes a weekend-only engagement in its partial first week", async () => {
  await db.prepare("UPDATE hr_engagements SET start_date=?,end_date=? WHERE person_id=?")
    .bind(addDays(monday,5),addDays(monday,6),person(f.tie)).run();
  const w=await createWeek();
  await addShift(w.id,[f.claimant]);
  const version=(await db.prepare("SELECT version FROM people_roster_weeks WHERE id=?").bind(w.id).first<{version:number}>())!.version;
  await expect(command({action:"week.publish",id:w.id,expectedVersion:version})).rejects.toMatchObject({code:"ROSTER_COVERAGE"});
});
it("prepares a visible current-week browser roster through the normal independent manager and retains it on repeat", async () => {
  const prepared = await prepareLocalBrowserRosterExample(db, f.manager, f.claimant);
  expect(prepared.weekStart).toBe(rosterMonday());
  const worker = await rosterData(db, f.claimant, { department: f.department });
  expect(worker.week).toMatchObject({ id: prepared.weekId, state: "published" });
  expect(worker.selfRosterRequired).toBe(true);
  expect(worker.shifts.find(shift => shift.id === prepared.shiftId)).toMatchObject({ name: "Morning", state: "active" });
  expect(worker.assignments.filter(assignment => assignment.shift_id === prepared.shiftId && assignment.state === "assigned").map(assignment => assignment.person_id).sort()).toEqual([person(f.claimant), person(f.tie)].sort());
  const before = await db.prepare("SELECT count(*)::int n FROM command_receipts WHERE command_id LIKE ?").bind(`browser-roster-${f.company}-%`).first<{ n: number }>();
  expect(await prepareLocalBrowserRosterExample(db, f.manager, f.claimant)).toEqual(prepared);
  expect(await db.prepare("SELECT count(*)::int n FROM command_receipts WHERE command_id LIKE ?").bind(`browser-roster-${f.company}-%`).first<{ n: number }>()).toEqual(before);
});

it("does not grant roster management to a browser fixture's ordinary worker", async () => {
  await expect(prepareLocalBrowserRosterExample(db, f.claimant, f.tie)).rejects.toThrow("NORMAL_BROWSER_ROSTER_AUTHORITY_REQUIRED");
  expect((await db.prepare("SELECT count(*)::int n FROM people_roster_weeks WHERE company_id=?").bind(f.company).first<{ n: number }>())?.n).toBe(0);
});

it("releases future assignments and closes pending requests with a Position exit", async () => {
  const s=await setup();
  const request=await swap(s);
  const engagement=`eng-${f.claimant.userId}`;
  const exitSteps=[
    db.prepare("UPDATE organization_positions SET roster_required=0 WHERE id=(SELECT position_id FROM hr_engagements WHERE id=?)").bind(engagement),
    ...rosterPositionExitStatements(db,f.manager,[engagement],key(),"Moved to backoffice"),
  ];
  await db.batch(exitSteps);
  const assignment=await db.prepare("SELECT state FROM people_roster_assignments WHERE id=?").bind(s.aa.id).first<{state:string}>();
  const pending=await db.prepare("SELECT state FROM people_roster_requests WHERE id=?").bind(request.id).first<{state:string}>();
  expect(assignment?.state).toBe("released");
  expect(pending?.state).toBe("invalidated");
  await expect(command({action:"request.create",kind:"extra",targetShiftId:s.c.id,reason:"Cover this"},f.claimant)).rejects.toMatchObject({code:"ROSTER_STAFF"});
});
