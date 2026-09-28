import "server-only";
import { z } from "zod";
import { hasPermission } from "@/lib/permissions";
import { addDays, rosterMonday, rosterPending, rosterCommandSchema, type RosterData, type RosterWeek, type RosterShift, type RosterAssignment, type RosterRequest, type RosterDepartment, type ShiftTemplate, type RosterHistory } from "@/lib/people-roster";
import type { WorkContext } from "./authorization";
import type { SqlDatabase } from "./runtime/types";
import type { PostgresDatabase } from "./runtime/postgres";
import { activeStaff, rosterStaffPeople, type StaffPerson } from "./staff-policy";
import { ApiError } from "./http";
import { executeCommand, replayCommand } from "./commands";

const fail = (code: string, message: string, status = 409): never => { throw new ApiError(status, code, message); };
const one = <T>(db: SqlDatabase, sql: string, ...args: (string | number | null | boolean)[]) => db.prepare(sql).bind(...args).first<T>();
const rows = async <T>(db: SqlDatabase, sql: string, ...args: (string | number | null | boolean)[]) => (await db.prepare(sql).bind(...args).all<T>()).results;
const run = (db: SqlDatabase, sql: string, ...args: (string | number | null | boolean)[]) => db.prepare(sql).bind(...args).run();
const uid = () => crypto.randomUUID();
const pending = "('pending_coworker','pending_manager')";
type Result = { id: string; version: number; state?: string };
type ShiftWithWeek = RosterShift & { company_id: string; branch_id: string | null; department_id: string; week_start: string; timezone: string; week_state: RosterWeek["state"] };
const shiftSelect = "SELECT s.*,w.company_id,w.branch_id,w.department_id,w.week_start,w.timezone,w.state week_state FROM people_roster_shifts s JOIN people_roster_weeks w ON w.id=s.week_id";

async function self(db: SqlDatabase, actor: WorkContext) {
  try { return await activeStaff(db, actor.companyId, actor.userId); }
  catch (e) { if (e instanceof ApiError && e.code === "STAFF_LINK_REQUIRED") return null; throw e; }
}
async function departments(db: SqlDatabase, actor: WorkContext, staff: StaffPerson | null, manager: boolean) {
  return rows<RosterDepartment>(db, `WITH RECURSIVE subtree AS (
    SELECT id FROM organization_nodes WHERE id=? AND company_id=?
    UNION ALL SELECT n.id FROM organization_nodes n JOIN subtree p ON n.parent_id=p.id WHERE n.company_id=? AND n.status='active')
    SELECT d.id,d.display_name name,d.branch_id,b.display_name branch_name FROM organization_nodes d
    LEFT JOIN organization_nodes b ON b.id=d.branch_id
    WHERE d.company_id=? AND d.kind='department' AND d.status='active'
    AND (?::text IS NULL OR d.branch_id=?) AND (?::text IS NULL OR d.id IN (SELECT id FROM subtree))
    AND (? OR d.id=?) ORDER BY b.display_name,d.display_name`, actor.departmentId, actor.companyId, actor.companyId, actor.companyId, actor.branchId, actor.branchId, actor.departmentId, manager, staff?.department_id ?? null);
}
async function department(db: SqlDatabase, actor: WorkContext, id: string, staff: StaffPerson | null, manager: boolean) {
  const found = (await departments(db, actor, staff, manager)).find(d => d.id === id);
  return found ?? fail("ROSTER_SCOPE", "This department is outside your roster access.", 403);
}
async function week(db: SqlDatabase, actor: WorkContext, id: string) {
  const found = await one<RosterWeek>(db, "SELECT * FROM people_roster_weeks WHERE id=? AND company_id=?", id, actor.companyId);
  return found ?? fail("NOT_FOUND", "Roster week not found.", 404);
}
async function shift(db: SqlDatabase, actor: WorkContext, id: string) {
  const found = await one<ShiftWithWeek>(db, `${shiftSelect} WHERE s.id=? AND w.company_id=?`, id, actor.companyId);
  return found ?? fail("NOT_FOUND", "Shift not found.", 404);
}
function expected(actual: number, supplied: number | undefined) { if (actual !== supplied) fail("ROSTER_CHANGED", "This roster changed. Refresh before trying again."); }
function future(s: RosterShift) { if (s.starts_at <= Date.now()) fail("ROSTER_STARTED", "A shift that has started can no longer be changed."); }
function publishedReason(w: RosterWeek | ShiftWithWeek, reason: string) {
  if (("week_state" in w ? w.week_state : w.state) === "published" && reason.trim().length < 3) fail("ROSTER_REASON", "Explain the change to this published roster.", 422);
}
async function times(db: SqlDatabase, date: string, start: string, end: string, timezone: string) {
  if (start === end) fail("ROSTER_TIME", "Start and end times must differ.", 422);
  const finish = end < start ? addDays(date, 1) : date;
  const v = await one<{ starts_at: number; ends_at: number; local_start: string; local_end: string }>(db, `SELECT
    (extract(epoch FROM (?::timestamp AT TIME ZONE ?))*1000)::bigint starts_at,
    (extract(epoch FROM (?::timestamp AT TIME ZONE ?))*1000)::bigint ends_at,
    to_char((?::timestamp AT TIME ZONE ?) AT TIME ZONE ?,'YYYY-MM-DD HH24:MI') local_start,
    to_char((?::timestamp AT TIME ZONE ?) AT TIME ZONE ?,'YYYY-MM-DD HH24:MI') local_end`,
    `${date} ${start}`, timezone, `${finish} ${end}`, timezone, `${date} ${start}`, timezone, timezone, `${finish} ${end}`, timezone, timezone);
  if (!v || v.local_start !== `${date} ${start}` || v.local_end !== `${finish} ${end}` || v.ends_at <= v.starts_at) fail("ROSTER_TIME", "This local time does not exist in the roster timezone.", 422);
  return { starts_at: v!.starts_at, ends_at: v!.ends_at };
}
async function eligible(db: SqlDatabase, w: Pick<RosterWeek, "company_id" | "branch_id" | "department_id">, personId: string, date: string) {
  const matches = (await rosterStaffPeople(db, w.company_id, w.department_id, date, date)).filter(p => p.person_id === personId && p.branch_id === w.branch_id && p.roster_required === 1);
  if (matches.length !== 1) fail("ROSTER_STAFF", "Choose staff whose active Position requires a roster in this department.");
  return matches[0];
}
type RosterGap = { person_id: string; legal_name: string; engagement_id: string; week_start: string };
async function rosterGaps(db: SqlDatabase, companyId: string, departmentId: string, start: string) {
  return rows<RosterGap>(db, `SELECT p.id person_id,p.legal_name,e.id engagement_id,?::date::text week_start
    FROM hr_people p JOIN hr_engagements e ON e.person_id=p.id AND e.company_id=p.company_id
    JOIN organization_positions pos ON pos.id=e.position_id AND pos.company_id=e.company_id AND pos.active=1 AND pos.roster_required=1
    WHERE p.company_id=? AND e.department_id=? AND p.status='active' AND p.merged_into_id IS NULL AND p.user_id IS NOT NULL
      AND e.status='active' AND e.start_date::date<=(?::date+6) AND (e.end_date IS NULL OR e.end_date::date>=?::date)
      AND EXISTS (SELECT 1 FROM assignments grant_row WHERE grant_row.company_id=p.company_id AND grant_row.user_id=p.user_id
        AND iam_assignment_active(grant_row.id,?) AND iam_principal_active(grant_row.user_id,grant_row.company_id,?))
      AND EXISTS (SELECT 1 FROM generate_series(?::date,?::date+6,interval '1 day') day WHERE extract(isodow FROM day)<=5
        AND day::date>=e.start_date::date AND (e.end_date IS NULL OR day::date<=e.end_date::date)
        AND NOT EXISTS (SELECT 1 FROM leave_requests l LEFT JOIN employees emp ON emp.id=l.employee_id AND emp.company_id=l.company_id
          WHERE l.company_id=e.company_id AND l.state='approved' AND coalesce(l.person_id,emp.person_id)=p.id
          AND (EXISTS (SELECT 1 FROM leave_day_segments ds WHERE ds.request_id=l.id AND ds.company_id=l.company_id AND ds.leave_date::date=day::date AND ds.segment='full')
            OR NOT EXISTS (SELECT 1 FROM leave_day_segments ds WHERE ds.request_id=l.id) AND day::date BETWEEN l.start_date::date AND coalesce(l.end_date::date,l.start_date::date+l.days::int-1)
              AND (day::date<>l.start_date::date OR l.start_segment='full') AND (l.end_date IS NULL OR day::date<>l.end_date::date OR l.end_segment='full'))))
      AND NOT EXISTS (SELECT 1 FROM people_roster_assignments a JOIN people_roster_shifts s ON s.id=a.shift_id AND s.state='active'
        JOIN people_roster_weeks w ON w.id=s.week_id WHERE w.company_id=e.company_id AND w.department_id=e.department_id AND w.week_start=?::date
        AND a.engagement_id=e.id AND a.state='assigned') ORDER BY p.legal_name,p.id`, start, companyId, departmentId, start, start, Date.now(), Date.now(), start, start, start);
}
async function requireCoverage(db:SqlDatabase,w:RosterWeek|ShiftWithWeek) {
  const gaps=await rosterGaps(db,w.company_id,w.department_id,w.week_start);
  if(gaps.length) fail("ROSTER_COVERAGE",`Assign at least one shift to: ${gaps.map(g=>g.legal_name).join(", ")}.`);
}
async function assignmentIssues(db: SqlDatabase, w: Pick<RosterWeek, "company_id" | "branch_id" | "department_id" | "timezone">, s: RosterShift, personId: string, exclude: string[] = [], engagementId?: string) {
  const issues: string[] = [];
  let p: StaffPerson;
  try { p = await eligible(db, w, personId, s.local_date); }
  catch (e) { if (e instanceof ApiError && e.code === "ROSTER_STAFF") return [e.message]; throw e; }
  if (engagementId && p.engagement_id !== engagementId) issues.push("The staff engagement changed; reassign this shift.");
  const dates = await one<{ first_day: string; last_day: string }>(db, "SELECT to_char(to_timestamp(?::bigint/1000.0) AT TIME ZONE ?,'YYYY-MM-DD') first_day,to_char(to_timestamp((?::bigint-1)/1000.0) AT TIME ZONE ?,'YYYY-MM-DD') last_day", s.starts_at, w.timezone, s.ends_at, w.timezone);
  if (dates!.first_day < p.start_date || p.end_date && dates!.last_day > p.end_date) issues.push("Shift falls outside the staff engagement dates.");
  const leave = await one(db, `SELECT l.id FROM leave_requests l LEFT JOIN employees e ON e.id=l.employee_id AND e.company_id=l.company_id
    WHERE l.company_id=? AND l.state='approved' AND coalesce(l.person_id,e.person_id)=?
    AND EXISTS(SELECT 1 FROM (
 SELECT ds.leave_date::date absence_date FROM leave_day_segments ds WHERE ds.request_id=l.id AND ds.company_id=l.company_id
 UNION ALL SELECT generate_series(l.start_date::date,coalesce(l.end_date::date,l.start_date::date+l.days::int-1),interval '1 day')::date WHERE NOT EXISTS(SELECT 1 FROM leave_day_segments ds WHERE ds.request_id=l.id)
 ) absence WHERE absence_date<=?::date AND absence_date>=?::date) LIMIT 1`, w.company_id, personId, dates!.last_day, dates!.first_day);
  if (leave) issues.push("Approved leave overlaps this shift.");
  const collision = await one(db, `SELECT a.id FROM people_roster_assignments a JOIN people_roster_shifts s ON s.id=a.shift_id JOIN people_roster_weeks w ON w.id=s.week_id
    WHERE w.company_id=? AND a.person_id=? AND a.state='assigned' AND s.state='active' AND s.starts_at<? AND s.ends_at>?
    AND NOT (a.id=ANY(?::text[])) LIMIT 1`, w.company_id, personId, s.ends_at, s.starts_at, `{${exclude.join(",")}}`);
  if (collision) issues.push("Another assigned shift overlaps this time.");
  return issues;
}
async function validateAssignment(db: SqlDatabase, w: RosterWeek | ShiftWithWeek, s: RosterShift, personId: string, exclude: string[] = [], engagementId?: string) {
  const issues = await assignmentIssues(db, w, s, personId, exclude, engagementId);
  if (issues.length) fail("ROSTER_CONFLICT", issues.join(" "));
  return eligible(db, w, personId, s.local_date);
}
async function capacity(db: SqlDatabase, s: RosterShift, exclude: string[] = []) {
  const count = await one<{ n: number }>(db, "SELECT count(*)::int n FROM people_roster_assignments WHERE shift_id=? AND state='assigned' AND NOT(id=ANY(?::text[]))", s.id, `{${exclude.join(",")}}`);
  if (count!.n >= s.capacity) fail("ROSTER_FULL", "This shift has no open slots remaining.");
}
async function assign(db: SqlDatabase, s: RosterShift, p: StaffPerson) {
  const id = uid();
  await run(db, "INSERT INTO people_roster_assignments(id,shift_id,person_id,engagement_id) VALUES(?,?,?,?)", id, s.id, p.person_id, p.engagement_id);
  return id;
}
async function notice(db: SqlDatabase, actor: WorkContext, type: "roster_week" | "roster_request", recordId: string, version: number, message: string, personIds: string[]) {
  for (const personId of new Set(personIds)) {
    const p = await one<{ user_id: string }>(db, "SELECT user_id FROM hr_people WHERE id=? AND company_id=? AND user_id IS NOT NULL", personId, actor.companyId);
    if (p) await run(db, "INSERT INTO staff_notifications(id,company_id,user_id,record_type,record_id,record_version,message,created_at) VALUES(?,?,?,?,?,?,?,?) ON CONFLICT DO NOTHING", uid(), actor.companyId, p.user_id, type, recordId, version, message, Date.now());
  }
}
async function notifyManagers(db: SqlDatabase, actor: WorkContext, r: RosterRequest, w: ShiftWithWeek) {
  const users = await rows<{ user_id: string }>(db, `WITH RECURSIVE ancestors AS (
    SELECT id,parent_id FROM organization_nodes WHERE id=? AND company_id=?
    UNION ALL SELECT n.id,n.parent_id FROM organization_nodes n JOIN ancestors c ON n.id=c.parent_id WHERE n.company_id=? AND n.status='active')
    SELECT DISTINCT a.user_id FROM assignments a WHERE a.company_id=? AND iam_assignment_active(a.id,?) AND iam_principal_active(a.user_id,a.company_id,?) AND iam_role_allows(a.role,a.company_id,'roster.approve')
    AND (a.scope_kind='holding' OR a.scope_kind='branch' AND a.scope_node_id=? OR a.scope_kind='department' AND a.scope_node_id IN (SELECT id FROM ancestors))
    AND NOT EXISTS(SELECT 1 FROM hr_people p WHERE p.id=ANY(?::text[]) AND iam_same_person(p.user_id,a.user_id,p.company_id))`, w.department_id, actor.companyId, actor.companyId, actor.companyId, Date.now(), Date.now(), w.branch_id, `{${[r.requester_person_id, r.coworker_person_id].filter(Boolean).join(",")}}`);
  for (const u of users) await run(db, "INSERT INTO staff_notifications(id,company_id,user_id,record_type,record_id,record_version,message,created_at) VALUES(?,?,?,'roster_request',?,?,?,?) ON CONFLICT DO NOTHING", uid(), actor.companyId, u.user_id, r.id, r.version, "A roster change needs your review.", Date.now());
}
async function history(db: SqlDatabase, actor: WorkContext, departmentId: string, recordId: string, action: string, reason: string, evidence: unknown, weekId: string | null = null, requestId: string | null = null) {
  await run(db, "INSERT INTO people_roster_history(id,company_id,department_id,week_id,request_id,record_id,action,actor_id,reason,evidence_json,created_at) VALUES(?,?,?,?,?,?,?,?,?,?,?)", uid(), actor.companyId, departmentId, weekId, requestId, recordId, action, actor.userId, reason, JSON.stringify(evidence), Date.now());
}
async function invalidate(db: SqlDatabase, actor: WorkContext, shiftIds: string[], assignmentIds: string[], except = "") {
  const changed = await rows<RosterRequest>(db, `UPDATE people_roster_requests SET state='invalidated',decision_note='The referenced shift or assignment changed.',version=version+1,updated_at=?
    WHERE company_id=? AND state IN ${pending} AND id!=? AND (target_shift_id=ANY(?::text[]) OR source_assignment_id=ANY(?::text[]) OR target_assignment_id=ANY(?::text[])) RETURNING *`, Date.now(), actor.companyId, except, `{${shiftIds.join(",")}}`, `{${assignmentIds.join(",")}}`, `{${assignmentIds.join(",")}}`);
  for (const r of changed) {
    await history(db, actor, r.department_id, r.id, "request.invalidated", r.decision_note, {}, null, r.id);
    await notice(db, actor, "roster_request", r.id, r.version, "Your roster request needs to be submitted again because its shift changed.", [r.requester_person_id, ...(r.coworker_person_id ? [r.coworker_person_id] : [])]);
  }
}

export async function rescheduleRosterShift(
  db: SqlDatabase,
  actor: WorkContext,
  input: { id: string; expectedVersion: number; date: string; startTime: string; endTime: string; reason: string },
) {
  if (actor.beta) fail("ROSTER_UNAVAILABLE", "Rosters are unavailable in this workspace.", 403);
  if (
    !hasPermission(actor, "roster.manage") ||
    !(await one(db, "SELECT 1 WHERE iam_role_allows(?,?,?)", actor.role, actor.companyId, "roster.manage"))
  )
    fail("PERMISSION_REQUIRED", "Your selected role cannot perform this roster action.", 403);
  await run(db, "INSERT INTO people_roster_locks(company_id) VALUES(?) ON CONFLICT(company_id) DO UPDATE SET version=people_roster_locks.version+1", actor.companyId);
  const current = await shift(db, actor, input.id);
  await department(db, actor, current.department_id, await self(db, actor), true);
  expected(current.version, input.expectedVersion);
  future(current);
  publishedReason(current, input.reason);
  if (current.state !== "active") fail("ROSTER_CHANGED", "The shift is no longer active.");
  if (input.date < current.week_start || input.date > addDays(current.week_start, 6))
    fail("ROSTER_DATE", "The shift must stay within its original roster week.", 422);
  const nextTimes = await times(db, input.date, input.startTime, input.endTime, current.timezone);
  const next: RosterShift = {
    ...current,
    local_date: input.date,
    start_time: input.startTime,
    end_time: input.endTime,
    ...nextTimes,
    version: current.version + 1,
  };
  future(next);
  const assignments = await rows<RosterAssignment>(db, "SELECT * FROM people_roster_assignments WHERE shift_id=? AND state='assigned' FOR UPDATE", current.id);
  for (const assignment of assignments)
    await validateAssignment(db, current, next, assignment.person_id, assignments.map((value) => value.id), assignment.engagement_id);
  await run(
    db,
    "UPDATE people_roster_shifts SET local_date=?,start_time=?,end_time=?,starts_at=?,ends_at=?,version=version+1 WHERE id=?",
    next.local_date,
    next.start_time,
    next.end_time,
    next.starts_at,
    next.ends_at,
    next.id,
  );
  await invalidate(db, actor, [next.id], assignments.map((assignment) => assignment.id));
  const updatedWeek = await one<RosterWeek>(db, "UPDATE people_roster_weeks SET version=version+1,updated_at=? WHERE id=? RETURNING *", Date.now(), next.week_id);
  await history(db, actor, current.department_id, next.id, "shift.reschedule", input.reason, {
    from: { date: current.local_date, startTime: current.start_time, endTime: current.end_time },
    to: { date: next.local_date, startTime: next.start_time, endTime: next.end_time },
  }, next.week_id);
  if (current.week_state === "published")
    await notice(db, actor, "roster_week", next.week_id, updatedWeek!.version, `A published shift changed: ${next.name}, ${next.local_date}.`, assignments.map((assignment) => assignment.person_id));
  return { id: next.id, version: next.version, status: "active", warnings: [] as string[] };
}
async function readRequest(db: SqlDatabase, actor: WorkContext, id: string) {
  const r = await one<RosterRequest>(db, "SELECT * FROM people_roster_requests WHERE id=? AND company_id=?", id, actor.companyId);
  return r ?? fail("NOT_FOUND", "Roster request not found.", 404);
}
async function requestParticipants(db: SqlDatabase, actor: WorkContext, r: RosterRequest) {
  const same = async (id: string | null) => !!id && !!await one(db, "SELECT 1 FROM hr_people WHERE id=? AND company_id=? AND iam_same_person(user_id,?,company_id)", id, actor.companyId, actor.userId);
  return { own: await same(r.requester_person_id), coworker: await same(r.coworker_person_id) };
}
async function requestCheck(db: SqlDatabase, actor: WorkContext, r: RosterRequest) {
  const target = await shift(db, actor, r.target_shift_id);
  if (target.state !== "active" || target.week_state !== "published") fail("ROSTER_CHANGED", "The destination is no longer a published shift.");
  future(target);
  let source: ShiftWithWeek | null = null;
  let sourceAssignment: RosterAssignment | null = null;
  let targetAssignment: RosterAssignment | null = null;
  if (r.source_assignment_id) {
    sourceAssignment = await one<RosterAssignment>(db, "SELECT * FROM people_roster_assignments WHERE id=?", r.source_assignment_id);
    if (!sourceAssignment || sourceAssignment.state !== "assigned" || sourceAssignment.person_id !== r.requester_person_id || sourceAssignment.version !== r.source_version) fail("ROSTER_CHANGED", "The original assignment changed.");
    source = await shift(db, actor, sourceAssignment!.shift_id);
    if (source.department_id !== target.department_id || source.branch_id !== target.branch_id || source.state !== "active" || source.week_state !== "published" || source.id === target.id) fail("ROSTER_SCOPE", "Choose a different published shift in the same department.");
    future(source);
  }
  if (r.kind === "swap") {
    targetAssignment = await one<RosterAssignment>(db, "SELECT * FROM people_roster_assignments WHERE id=?", r.target_assignment_id!);
    if (!targetAssignment || targetAssignment.state !== "assigned" || targetAssignment.shift_id !== target.id || targetAssignment.person_id !== r.coworker_person_id || targetAssignment.version !== r.target_version || r.coworker_person_id === r.requester_person_id) fail("ROSTER_CHANGED", "The coworker's assignment changed.");
  }
  const exclude = [sourceAssignment?.id, targetAssignment?.id].filter((x): x is string => !!x);
  const requester = await validateAssignment(db, target, target, r.requester_person_id, exclude, sourceAssignment?.engagement_id);
  await capacity(db, target, exclude);
  let coworker: StaffPerson | null = null;
  if (r.coworker_person_id && source) {
    coworker = await validateAssignment(db, source, source, r.coworker_person_id, exclude, targetAssignment?.engagement_id);
    await capacity(db, source, exclude);
  }
  return { target, source, sourceAssignment, targetAssignment, requester, coworker };
}

export async function rosterCommand(database: PostgresDatabase, actor: WorkContext, raw: unknown, key: string): Promise<Result> {
  const parsed = rosterCommandSchema.safeParse(raw);
  if (!parsed.success) fail("VALIDATION_FAILED", parsed.error.issues[0]?.message ?? "Invalid roster command.", 422);
  const input = parsed.data!;
  if (actor.beta) fail("ROSTER_UNAVAILABLE", "Rosters are unavailable in this workspace.", 403);
  const operation = `roster.${input.action}`;
  const recordId = "id" in input && input.id || `roster-${key}`;
  return database.transaction(async db => {
    // The company revision serializes capacity and overlap checks across different weeks.
    // SERIALIZABLE retries recompute every check after a concurrent change.
    const replay = await replayCommand<Result>(db, actor, key, operation, recordId, input);
    if (replay) return replay;
    await run(db, "INSERT INTO people_roster_locks(company_id) VALUES(?) ON CONFLICT(company_id) DO UPDATE SET version=people_roster_locks.version+1", actor.companyId);
    const staff = await self(db, actor);
    const manager = ["roster.read", "roster.manage", "roster.approve"].some(p => hasPermission(actor, p));
    let departmentId = "", weekId: string | null = null, requestId: string | null = null;
    let result: Result;
    const need = async (permission: string) => {
      if (!hasPermission(actor, permission) || !await one(db, "SELECT 1 WHERE iam_role_allows(?,?,?)", actor.role, actor.companyId, permission)) fail("PERMISSION_REQUIRED", "Your selected role cannot perform this roster action.", 403);
    };
    if (!input.action.startsWith("request.")) await need("roster.manage");
    if (input.action === "template.save") {
      await department(db, actor, input.departmentId, staff, true);
      departmentId = input.departmentId;
      if (input.startTime === input.endTime) fail("ROSTER_TIME", "Start and end times must differ.", 422);
      const old = input.id ? await one<ShiftTemplate>(db, "SELECT * FROM people_roster_templates WHERE id=? AND company_id=? AND department_id=?", input.id, actor.companyId, departmentId) : null;
      if (input.id && !old) fail("NOT_FOUND", "Template not found.", 404);
      if (old) expected(old.version, input.expectedVersion);
      result = { id: recordId, version: (old?.version ?? 0) + 1 };
      await run(db, `INSERT INTO people_roster_templates(id,company_id,department_id,name,start_time,end_time,capacity,active,version,created_by,updated_at) VALUES(?,?,?,?,?,?,?,?,?,?,?)
        ON CONFLICT(id) DO UPDATE SET name=excluded.name,start_time=excluded.start_time,end_time=excluded.end_time,capacity=excluded.capacity,active=excluded.active,version=excluded.version,updated_at=excluded.updated_at`, result.id, actor.companyId, departmentId, input.name, input.startTime, input.endTime, input.capacity, input.active, result.version, actor.userId, Date.now());
    } else if (input.action === "week.create") {
      const d = await department(db, actor, input.departmentId, staff, true);
      departmentId = d.id;
      if (rosterMonday(input.weekStart) !== input.weekStart) fail("ROSTER_WEEK", "Choose the Monday beginning the roster week.", 422);
      if (!await one(db, "SELECT 1 FROM pg_timezone_names WHERE name=?", input.timezone)) fail("ROSTER_TIMEZONE", "Choose a valid IANA timezone.", 422);
      if (await one(db, "SELECT 1 FROM people_roster_weeks WHERE company_id=? AND department_id=? AND week_start=?::date", actor.companyId, d.id, input.weekStart)) fail("ROSTER_EXISTS", "This department already has a roster for that week.");
      const w: RosterWeek = { id: recordId, company_id: actor.companyId, branch_id: d.branch_id, department_id: d.id, week_start: input.weekStart, timezone: input.timezone, state: "draft", version: 1 };
      await run(db, "INSERT INTO people_roster_weeks(id,company_id,branch_id,department_id,week_start,timezone,created_by,created_at,updated_at) VALUES(?,?,?,?,?,?,?,?,?)", w.id, w.company_id, w.branch_id, w.department_id, w.week_start, w.timezone, actor.userId, Date.now(), Date.now());
      if (input.copyWeekId) {
        const source = await week(db, actor, input.copyWeekId);
        if (source.department_id !== w.department_id) fail("ROSTER_SCOPE", "Copy a week from the same department.");
        const offset = Math.round((Date.parse(w.week_start) - Date.parse(source.week_start)) / 86400000);
        for (const s of await rows<RosterShift>(db, "SELECT * FROM people_roster_shifts WHERE week_id=? AND state='active' ORDER BY starts_at,id", source.id)) {
          const date = addDays(s.local_date, offset), t = await times(db, date, s.start_time, s.end_time, w.timezone);
          const copy: RosterShift = { ...s, id: uid(), week_id: w.id, local_date: date, ...t, version: 1 };
          future(copy);
          await run(db, "INSERT INTO people_roster_shifts(id,week_id,template_id,name,local_date,start_time,end_time,starts_at,ends_at,capacity) VALUES(?,?,?,?,?,?,?,?,?,?)", copy.id, w.id, s.template_id, s.name, date, s.start_time, s.end_time, t.starts_at, t.ends_at, s.capacity);
          for (const a of await rows<RosterAssignment>(db, "SELECT * FROM people_roster_assignments WHERE shift_id=? AND state='assigned'", s.id)) await assign(db, copy, await validateAssignment(db, w, copy, a.person_id));
        }
      }
      weekId = w.id; result = { id: w.id, version: 1, state: "draft" };
    } else if (input.action === "week.publish") {
      const w = await week(db, actor, input.id);
      await department(db, actor, w.department_id, staff, true); departmentId = w.department_id; weekId = w.id;
      expected(w.version, input.expectedVersion);
      if (w.state !== "draft") fail("ROSTER_CHANGED", "This roster is already published.");
      const shifts = await rows<RosterShift>(db, "SELECT * FROM people_roster_shifts WHERE week_id=? AND state='active'", w.id);
      if (!shifts.length) fail("ROSTER_EMPTY", "Add at least one shift before publishing.");
      const assigned: string[] = [];
      for (const s of shifts) {
        future(s);
        const aa = await rows<RosterAssignment>(db, "SELECT * FROM people_roster_assignments WHERE shift_id=? AND state='assigned'", s.id);
        if (aa.length > s.capacity) fail("ROSTER_FULL", "A shift exceeds its required headcount.");
        for (const a of aa) { await validateAssignment(db, w, s, a.person_id, [a.id], a.engagement_id); assigned.push(a.person_id); }
      }
      await requireCoverage(db,w);
      result = { id: w.id, version: w.version + 1, state: "published" };
      await run(db, "UPDATE people_roster_weeks SET state='published',published_by=?,version=version+1,updated_at=? WHERE id=?", actor.userId, Date.now(), w.id);
      // Include department members so open slots are discoverable even without an assignment.
      await notice(db, actor, "roster_week", w.id, result.version, `The roster for ${w.week_start} is published.`, [...assigned, ...(await rosterStaffPeople(db, actor.companyId, departmentId, w.week_start, addDays(w.week_start, 6))).filter(p=>p.roster_required===1).map(p => p.person_id)]);
    } else if (input.action === "shift.save") {
      const w = await week(db, actor, input.weekId);
      await department(db, actor, w.department_id, staff, true); departmentId = w.department_id; weekId = w.id;
      expected(w.version, input.expectedVersion); publishedReason(w, input.reason);
      if (input.date < w.week_start || input.date > addDays(w.week_start, 6)) fail("ROSTER_DATE", "The shift must start within this roster week.", 422);
      if (new Set(input.personIds).size !== input.personIds.length || input.personIds.length > input.capacity) fail("ROSTER_CAPACITY", "Assign each staff member once, within the shift headcount.", 422);
      if (input.templateId && !await one(db, "SELECT 1 FROM people_roster_templates WHERE id=? AND company_id=? AND department_id=? AND active", input.templateId, actor.companyId, w.department_id)) fail("ROSTER_TEMPLATE", "Choose an active template from this department.");
      const old = input.id ? await shift(db, actor, input.id) : null;
      if (old && (old.week_id !== w.id || old.state !== "active")) fail("ROSTER_CHANGED", "The shift is unavailable in this week.");
      if (old) future(old);
      const t = await times(db, input.date, input.startTime, input.endTime, w.timezone);
      const s: RosterShift = { id: recordId, week_id: w.id, template_id: input.templateId ?? null, name: input.name, local_date: input.date, start_time: input.startTime, end_time: input.endTime, ...t, capacity: input.capacity, state: "active", version: (old?.version ?? 0) + 1 };
      future(s);
      const oldAssignments = old ? await rows<RosterAssignment>(db, "SELECT * FROM people_roster_assignments WHERE shift_id=? AND state='assigned' FOR UPDATE", old.id) : [];
      const people: StaffPerson[] = [];
      for (const personId of [...input.personIds].sort()) {
        await one(db, "SELECT id FROM hr_people WHERE id=? AND company_id=? FOR UPDATE", personId, actor.companyId);
        people.push(await validateAssignment(db, w, s, personId, oldAssignments.map(a => a.id)));
      }
      await run(db, `INSERT INTO people_roster_shifts(id,week_id,template_id,name,local_date,start_time,end_time,starts_at,ends_at,capacity,version) VALUES(?,?,?,?,?,?,?,?,?,?,?)
        ON CONFLICT(id) DO UPDATE SET template_id=excluded.template_id,name=excluded.name,local_date=excluded.local_date,start_time=excluded.start_time,end_time=excluded.end_time,starts_at=excluded.starts_at,ends_at=excluded.ends_at,capacity=excluded.capacity,version=excluded.version`, s.id, w.id, s.template_id, s.name, s.local_date, s.start_time, s.end_time, s.starts_at, s.ends_at, s.capacity, s.version);
      await run(db, "UPDATE people_roster_assignments SET state='released',version=version+1 WHERE shift_id=? AND state='assigned'", s.id);
      for (const p of people) await assign(db, s, p);
      await invalidate(db, actor, [s.id], oldAssignments.map(a => a.id));
      await run(db, "UPDATE people_roster_weeks SET version=version+1,updated_at=? WHERE id=?", Date.now(), w.id);
      if(w.state==="published") await requireCoverage(db,w);
      if (w.state === "published") await notice(db, actor, "roster_week", w.id, w.version + 1, `A published shift changed: ${s.name}, ${s.local_date}.`, [...input.personIds, ...oldAssignments.map(a => a.person_id)]);
      result = { id: s.id, version: s.version };
    } else if (input.action === "shift.cancel") {
      const s = await shift(db, actor, input.id);
      await department(db, actor, s.department_id, staff, true); departmentId = s.department_id; weekId = s.week_id;
      expected(s.version, input.expectedVersion); future(s); publishedReason(s, input.reason);
      if (s.state !== "active") fail("ROSTER_CHANGED", "The shift was already cancelled.");
      const aa = await rows<RosterAssignment>(db, "SELECT * FROM people_roster_assignments WHERE shift_id=? AND state='assigned' FOR UPDATE", s.id);
      await run(db, "UPDATE people_roster_shifts SET state='cancelled',version=version+1 WHERE id=?", s.id);
      await run(db, "UPDATE people_roster_assignments SET state='released',version=version+1 WHERE shift_id=? AND state='assigned'", s.id);
      await invalidate(db, actor, [s.id], aa.map(a => a.id));
      const w = await one<RosterWeek>(db, "UPDATE people_roster_weeks SET version=version+1,updated_at=? WHERE id=? RETURNING *", Date.now(), s.week_id);
      if(s.week_state==="published") await requireCoverage(db,w!);
      if (s.week_state === "published") await notice(db, actor, "roster_week", s.week_id, w!.version, `${s.name} on ${s.local_date} was cancelled.`, aa.map(a => a.person_id));
      result = { id: s.id, version: s.version + 1, state: "cancelled" };
    } else if (input.action === "request.create") {
      if (!staff || !hasPermission(actor, "self.roster")) fail("STAFF_LINK_REQUIRED", "An active staff profile is required.", 403);
      if (staff!.roster_required !== 1) fail("ROSTER_STAFF", "Your Position does not require roster shifts.", 403);
      const target = await shift(db, actor, input.targetShiftId);
      await department(db, actor, target.department_id, staff, false); departmentId = target.department_id;
      const source = input.sourceAssignmentId ? await one<RosterAssignment>(db, "SELECT * FROM people_roster_assignments WHERE id=?", input.sourceAssignmentId) : null;
      const dest = input.targetAssignmentId ? await one<RosterAssignment>(db, "SELECT * FROM people_roster_assignments WHERE id=?", input.targetAssignmentId) : null;
      if (input.kind === "extra" && input.sourceAssignmentId || input.kind !== "extra" && !source || input.kind === "swap" && !dest || input.kind !== "swap" && input.targetAssignmentId) fail("ROSTER_REQUEST", "Choose the assignments required for this request.", 422);
      const r = { id: recordId, company_id: actor.companyId, department_id: target.department_id, kind: input.kind, requester_person_id: staff!.person_id, coworker_person_id: dest?.person_id ?? null, source_assignment_id: source?.id ?? null, source_version: source?.version ?? null, target_shift_id: target.id, target_assignment_id: dest?.id ?? null, target_version: dest?.version ?? null, state: input.kind === "swap" ? "pending_coworker" : "pending_manager", reason: input.reason, decision_note: "", version: 1 } as RosterRequest;
      await requestCheck(db, actor, r);
      if (await one(db, `SELECT 1 FROM people_roster_requests WHERE company_id=? AND requester_person_id=? AND target_shift_id=? AND state IN ${pending}`, actor.companyId, r.requester_person_id, target.id)) fail("ROSTER_DUPLICATE", "You already have a pending request for this shift.");
      await run(db, "INSERT INTO people_roster_requests(id,company_id,department_id,kind,requester_person_id,coworker_person_id,source_assignment_id,source_version,target_shift_id,target_assignment_id,target_version,state,reason,created_by,created_at,updated_at) VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?)", r.id, r.company_id, r.department_id, r.kind, r.requester_person_id, r.coworker_person_id, r.source_assignment_id, r.source_version, r.target_shift_id, r.target_assignment_id, r.target_version, r.state, r.reason, actor.userId, Date.now(), Date.now());
      requestId = r.id;
      if (r.coworker_person_id) await notice(db, actor, "roster_request", r.id, 1, "A coworker requested a shift swap with you.", [r.coworker_person_id]);
      else await notifyManagers(db, actor, r, target);
      result = { id: r.id, version: 1, state: r.state };
    } else {
      const r = await readRequest(db, actor, input.id);
      await department(db, actor, r.department_id, staff, manager); departmentId = r.department_id; requestId = r.id;
      expected(r.version, input.expectedVersion);
      if (!rosterPending(r.state)) fail("ROSTER_CHANGED", "This request is no longer pending.");
      const who = await requestParticipants(db, actor, r);
      let next: RosterRequest["state"];
      if (input.decision === "withdraw") {
        if (!who.own) fail("ROSTER_PARTICIPANT", "Only the requester can withdraw this request.", 403);
        next = "withdrawn";
      } else if (input.decision === "accept") {
        if (!who.coworker || r.state !== "pending_coworker") fail("ROSTER_PARTICIPANT", "Only the selected coworker can accept this swap.", 403);
        await requestCheck(db, actor, r); next = "pending_manager";
      } else {
        const peerReject = input.decision === "reject" && who.coworker && r.state === "pending_coworker";
        if (!peerReject) {
          await need("roster.approve");
          if (who.own || who.coworker) fail("ROSTER_INDEPENDENT_REVIEW", "A manager who is not part of this request must review it.", 403);
          if (r.state !== "pending_manager") fail("ROSTER_COWORKER_REQUIRED", "Wait for the coworker to accept the swap first.");
        }
        if (input.decision === "reject") {
          if (input.reason.trim().length < 3) fail("ROSTER_REASON", "Give a reason for rejecting the request.", 422);
          next = "rejected";
        } else {
          const c = await requestCheck(db, actor, r);
          const aa = [c.sourceAssignment, c.targetAssignment].filter((a): a is RosterAssignment => !!a);
          await rows(db, "SELECT id FROM people_roster_shifts WHERE id=ANY(?::text[]) ORDER BY id FOR UPDATE", `{${[c.target.id, c.source?.id].filter(Boolean).join(",")}}`);
          for (const a of aa) await run(db, "UPDATE people_roster_assignments SET state='released',version=version+1 WHERE id=?", a.id);
          await assign(db, c.target, c.requester);
          if (c.coworker && c.source) await assign(db, c.source, c.coworker);
          await invalidate(db, actor, [], aa.map(a => a.id), r.id);
          await run(db, "UPDATE people_roster_weeks SET version=version+1,updated_at=? WHERE id=ANY(?::text[])", Date.now(), `{${[c.target.week_id, c.source?.week_id].filter(Boolean).join(",")}}`);
          for(const weekId of new Set([c.target.week_id,c.source?.week_id].filter((id):id is string=>!!id))) await requireCoverage(db,await week(db,actor,weekId));
          next = "approved";
        }
      }
      result = { id: r.id, version: r.version + 1, state: next };
      await run(db, "UPDATE people_roster_requests SET state=?,decision_note=?,accepted_by=CASE WHEN ?='accept' THEN ? ELSE accepted_by END,decided_by=?,version=version+1,updated_at=? WHERE id=?", next, input.reason, input.decision, actor.userId, actor.userId, Date.now(), r.id);
      await notice(db, actor, "roster_request", r.id, result.version, next === "pending_manager" ? "The coworker accepted your swap. A manager will review it." : `Your roster request was ${next}.`, [r.requester_person_id, ...(r.coworker_person_id ? [r.coworker_person_id] : [])]);
      if (next === "pending_manager") await notifyManagers(db, actor, { ...r, version: result.version }, await shift(db, actor, r.target_shift_id));
    }
    const reason = "reason" in input ? input.reason : "";
    await history(db, actor, departmentId, result.id, input.action, reason, input, weekId, requestId);
    return executeCommand(db, actor, key, operation, input, recordId, result.version, result, [], { companyId: actor.companyId, branchId: (await department(db, actor, departmentId, staff, manager)).branch_id, departmentId });
  });
}

export async function rosterData(db: SqlDatabase, actor: WorkContext, query: { department?: string; week?: string } = {}): Promise<RosterData> {
  if (actor.beta) fail("ROSTER_UNAVAILABLE", "Rosters are unavailable in this workspace.", 403);
  const canManage = hasPermission(actor, "roster.manage"), canApprove = hasPermission(actor, "roster.approve");
  const manager = canManage || canApprove || hasPermission(actor, "roster.read"), staff = await self(db, actor);
  const dd = await departments(db, actor, staff, manager);
  const selected = query.department ? dd.find(d => d.id === query.department) : dd.find(d => d.id === staff?.department_id) ?? dd[0];
  if (query.department && !selected) fail("ROSTER_SCOPE", "This department is outside your roster access.", 403);
  const start = query.week ?? rosterMonday();
  if (!z.iso.date().safeParse(start).success || rosterMonday(start) !== start) fail("ROSTER_WEEK", "Choose a valid Monday.", 422);
  const data: RosterData = { asOf: Date.now(), departments: dd, departmentId: selected?.id ?? null, weekStart: start, week: null, weeks: [], templates: [], shifts: [], assignments: [], requests: [], people: [], history: [], conflicts: [], coverageGaps: [], selfPersonId: staff?.roster_required===1?staff.person_id:null, selfRosterRequired:staff?.roster_required===1, canManage, canApprove };
  if (!selected) return data;
  data.weeks = await rows<RosterWeek>(db, "SELECT * FROM people_roster_weeks WHERE company_id=? AND department_id=? AND (? OR state='published') AND week_start BETWEEN ?::date AND ?::date ORDER BY week_start", actor.companyId, selected.id, manager, addDays(start, -35), addDays(start, 35));
  data.week = data.weeks.find(w => w.week_start === start) ?? null;
  const ids = `{${data.weeks.map(w => w.id).join(",")}}`;
  data.shifts = await rows<RosterShift>(db, "SELECT * FROM people_roster_shifts WHERE week_id=ANY(?::text[]) ORDER BY starts_at,name,id", ids);
  data.assignments = await rows<RosterAssignment>(db, `SELECT a.*,p.legal_name person_name FROM people_roster_assignments a JOIN hr_people p ON p.id=a.person_id JOIN people_roster_shifts s ON s.id=a.shift_id WHERE s.week_id=ANY(?::text[]) AND a.state='assigned' ORDER BY p.legal_name`, ids);
  if (canManage) {
    data.templates = await rows<ShiftTemplate>(db, "SELECT * FROM people_roster_templates WHERE company_id=? AND department_id=? ORDER BY start_time,name", actor.companyId, selected.id);
    data.people = (await rosterStaffPeople(db, actor.companyId, selected.id, start, addDays(start,6))).filter(p=>p.roster_required===1).map(p => ({ person_id: p.person_id, legal_name: p.legal_name, engagement_id: p.engagement_id }));
    const current=rosterMonday();
    data.coverageGaps = [...await rosterGaps(db,actor.companyId,selected.id,current),...await rosterGaps(db,actor.companyId,selected.id,addDays(current,7))];
    for(const gap of data.coverageGaps) await run(db,`INSERT INTO staff_notifications(id,company_id,user_id,record_type,record_id,record_version,message,created_at)
      SELECT gen_random_uuid()::text,?,a.user_id,'roster_coverage',?,1,?,? FROM assignments a
      WHERE a.company_id=? AND iam_assignment_active(a.id,?) AND iam_principal_active(a.user_id,a.company_id,?)
        AND iam_role_allows(a.role,a.company_id,'roster.manage')
        AND (a.scope_kind='holding' OR a.scope_kind='branch' AND a.scope_node_id=? OR a.scope_kind='department' AND a.scope_node_id=?)
      ON CONFLICT DO NOTHING`,actor.companyId,`${selected.id}:${gap.week_start}:${gap.person_id}`,`${gap.legal_name} needs a roster shift for the week of ${gap.week_start}.`,Date.now(),actor.companyId,Date.now(),Date.now(),selected.branch_id,selected.id);
  }
  data.requests = await rows<RosterRequest>(db, `SELECT r.*,p.legal_name requester_name,c.legal_name coworker_name FROM people_roster_requests r JOIN hr_people p ON p.id=r.requester_person_id LEFT JOIN hr_people c ON c.id=r.coworker_person_id
    WHERE r.company_id=? AND r.department_id=? AND (? OR r.requester_person_id=? OR r.coworker_person_id=?) ORDER BY r.created_at DESC LIMIT 200`, actor.companyId, selected.id, manager, staff?.person_id ?? null, staff?.person_id ?? null);
  // Include referenced published shifts outside the selected calendar range for clear request details.
  for (const r of data.requests) {
    for (const assignmentId of [r.source_assignment_id, r.target_assignment_id].filter((x): x is string => !!x)) {
      if (!data.assignments.some(a => a.id === assignmentId)) {
        const a = await one<RosterAssignment>(db, "SELECT a.*,p.legal_name person_name FROM people_roster_assignments a JOIN hr_people p ON p.id=a.person_id WHERE a.id=?", assignmentId);
        if (a) data.assignments.push(a);
      }
    }
    const source = data.assignments.find(a => a.id === r.source_assignment_id);
    for (const shiftId of [r.target_shift_id, source?.shift_id].filter((x): x is string => !!x)) {
      if (!data.shifts.some(s => s.id === shiftId)) { const s = await shift(db, actor, shiftId); data.shifts.push(s); }
    }
    const who = await requestParticipants(db, actor, r);
    r.permittedActions = rosterPending(r.state) ? [
      ...(who.own ? ["withdraw"] : []),
      ...(who.coworker && r.state === "pending_coworker" ? ["accept", "reject"] : []),
      ...(canApprove && !who.own && !who.coworker && r.state === "pending_manager" ? ["approve", "reject"] : []),
    ] : [];
  }
  data.history = await rows<RosterHistory>(db, `SELECT h.id,h.record_id,h.action,h.reason,h.created_at,COALESCE(p.legal_name,'Roster manager') actor_name
    FROM people_roster_history h LEFT JOIN hr_people p ON p.user_id=h.actor_id AND p.company_id=h.company_id AND p.merged_into_id IS NULL
    WHERE h.company_id=? AND h.department_id=? AND ((? AND (h.week_id=? OR h.request_id IS NOT NULL)) OR h.request_id IN
    (SELECT id FROM people_roster_requests WHERE company_id=? AND (requester_person_id=? OR coworker_person_id=?))) ORDER BY h.created_at DESC LIMIT 60`, actor.companyId, selected.id, manager, data.week?.id ?? null, actor.companyId, staff?.person_id ?? null, staff?.person_id ?? null);
  if (data.week) for (const s of data.shifts.filter(s => s.week_id === data.week!.id && s.state === "active")) {
    const aa = data.assignments.filter(a => a.shift_id === s.id && a.state === "assigned");
    if (aa.length > s.capacity) data.conflicts.push({ shiftId: s.id, message: "Required headcount is exceeded." });
    for (const a of aa) {
      if (!manager && a.person_id !== staff?.person_id) continue;
      const issues = await assignmentIssues(db, data.week, s, a.person_id, [a.id], a.engagement_id);
      for (const message of issues) data.conflicts.push({ shiftId: s.id, personId: a.person_id, message: `${a.person_name}: ${message}` });
    }
  }
  return data;
}

export async function rosterNotificationLink(db: SqlDatabase, actor: WorkContext, type: string, id: string) {
  const staff = await self(db, actor), manager = ["roster.read", "roster.manage", "roster.approve"].some(p => hasPermission(actor, p));
  let w: RosterWeek;
  if (type === "roster_request") {
    const r = await readRequest(db, actor, id), who = await requestParticipants(db, actor, r);
    if (!manager && !who.own && !who.coworker) fail("NOT_FOUND", "Request not found.", 404);
    const s = await shift(db, actor, r.target_shift_id); w = await week(db, actor, s.week_id);
  } else { w = await week(db, actor, id); if (!manager && w.state !== "published") fail("NOT_FOUND", "Roster not found.", 404); }
  await department(db, actor, w.department_id, staff, manager);
  return `/people/roster?department=${encodeURIComponent(w.department_id)}&week=${w.week_start}${type === "roster_request" ? `&view=requests&request=${encodeURIComponent(id)}` : ""}`;
}
