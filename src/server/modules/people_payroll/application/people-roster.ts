import {z} from 'zod';import { hasPermission } from "@/lib/identity_access/permissions";
import { addDays, rosterMonday, rosterPending, rosterCommandSchema, type RosterData, type RosterWeek, type RosterShift, type RosterAssignment, type RosterRequest, type RosterDepartment, type ShiftTemplate, type RosterHistory } from "../domain/roster";import type {WorkContext} from '../../identity_access/public/authorization';import type {StaffPerson} from '../../operations/public/staff-policy';import type {activeStaff,rosterStaffPeople} from '../../operations/public/staff-policy';import {BusinessRuleError,type Clock,type IdentifierSource} from '../../shared';import {fail,type Result,type ShiftWithWeek,expected,future,publishedReason,type RosterGap} from '../domain/people-roster';export interface RosterCommands<Effect>{replay<Result=Record<string,unknown>>(actor:WorkContext,key:string,operation:string,id:string,input:unknown):Promise<Result|null>;commit<Result>(actor:WorkContext,key:string,operation:string,input:unknown,id:string,version:number,result:Result,effects:Effect[],scope?:{companyId:string;branchId:string|null;departmentId:string|null}):Promise<Result>;}export interface PeopleRosterRepository{readDepartments(fact0:string | null,fact1:string,fact2:string,fact3:string,fact4:string | null,fact5:string | null,fact6:string | null,fact7:boolean,fact8:string | null):Promise<(RosterDepartment)[]>;
readWeek(fact0:string,fact1:string):Promise<RosterWeek|null>;
readShift(fact0:string,fact1:string):Promise<ShiftWithWeek|null>;
readTimes(fact0:string,fact1:string,fact2:string,fact3:string,fact4:string,fact5:string,fact6:string,fact7:string,fact8:string,fact9:string):Promise<{ starts_at: number; ends_at: number; local_start: string; local_end: string }|null>;
readRosterGaps(fact0:string,fact1:string,fact2:string,fact3:string,fact4:string,fact5:string,fact6:string,fact7:string,fact8:string,fact9:string):Promise<(RosterGap)[]>;
readEscalateRosterCoverage():Promise<({ company_id: string; department_id: string; branch_id: string | null })[]>;
applyEscalateRosterCoverage(fact0:string,fact1:string,fact2:string,fact3:string,fact4:string,fact5:string,fact6:string,fact7:number,fact8:string,fact9:number,fact10:number,fact11:string | null):Promise<{meta:{changes:number};success:boolean}>;
readAssignmentIssues(fact0:number,fact1:string,fact2:number,fact3:string):Promise<{ first_day: string; last_day: string }|null>;
readAssignmentIssues2(fact0:string,fact1:string,fact2:string,fact3:string):Promise<Record<string,unknown>|null>;
readAssignmentIssues3(fact0:string,fact1:string,fact2:number,fact3:number,fact4:string):Promise<Record<string,unknown>|null>;
readCapacity(fact0:string,fact1:string):Promise<{ n: number }|null>;
applyAssign(fact0:string,fact1:string,fact2:string,fact3:string):Promise<{meta:{changes:number};success:boolean}>;
readNotice(fact0:string,fact1:string):Promise<{ user_id: string }|null>;
applyNotice(fact0:string,fact1:string,fact2:string,fact3:"roster_week" | "roster_request",fact4:string,fact5:number,fact6:string,fact7:number):Promise<{meta:{changes:number};success:boolean}>;
readNotifyManagers(fact0:string,fact1:string,fact2:string,fact3:string,fact4:number,fact5:number,fact6:string | null,fact7:string):Promise<({ user_id: string })[]>;
applyNotifyManagers(fact0:string,fact1:string,fact2:string,fact3:string,fact4:number,fact5:"A roster change needs your review.",fact6:number):Promise<{meta:{changes:number};success:boolean}>;
applyHistory(fact0:string,fact1:string,fact2:string,fact3:string | null,fact4:string | null,fact5:string,fact6:string,fact7:string,fact8:string,fact9:string,fact10:number):Promise<{meta:{changes:number};success:boolean}>;
readInvalidate(fact0:number,fact1:string,fact2:string,fact3:string,fact4:string,fact5:string):Promise<(RosterRequest)[]>;
readRescheduleRosterShift(fact0:string,fact1:string,fact2:"roster.manage"):Promise<Record<string,unknown>|null>;
applyRescheduleRosterShift(fact0:string):Promise<{meta:{changes:number};success:boolean}>;
readRescheduleRosterShift2(fact0:string):Promise<(RosterAssignment)[]>;
applyRescheduleRosterShift2(fact0:string,fact1:string,fact2:string,fact3:number,fact4:number,fact5:string):Promise<{meta:{changes:number};success:boolean}>;
readRescheduleRosterShift3(fact0:number,fact1:string):Promise<RosterWeek|null>;
readReadRequest(fact0:string,fact1:string):Promise<RosterRequest|null>;
readRequestParticipants(fact0:string,fact1:string,fact2:string):Promise<Record<string,unknown>|null>;
readRequestCheck(fact0:string):Promise<RosterAssignment|null>;
readRequestCheck2(fact0:string):Promise<RosterAssignment|null>;
applyRosterCommand(fact0:string):Promise<{meta:{changes:number};success:boolean}>;
readRosterCommand(fact0:string,fact1:string,fact2:string):Promise<Record<string,unknown>|null>;
readRosterCommand2(fact0:string,fact1:string,fact2:string):Promise<ShiftTemplate|null>;
applyRosterCommand2(fact0:string,fact1:string,fact2:string,fact3:string,fact4:string,fact5:string,fact6:number,fact7:boolean,fact8:number,fact9:string,fact10:number):Promise<{meta:{changes:number};success:boolean}>;
readRosterCommand3(fact0:string):Promise<Record<string,unknown>|null>;
readRosterCommand4(fact0:string,fact1:string,fact2:string):Promise<Record<string,unknown>|null>;
applyRosterCommand3(fact0:string,fact1:string,fact2:string | null,fact3:string,fact4:string,fact5:string,fact6:string,fact7:number,fact8:number):Promise<{meta:{changes:number};success:boolean}>;
readRosterCommand5(fact0:string):Promise<(RosterShift)[]>;
applyRosterCommand4(fact0:string,fact1:string,fact2:string | null,fact3:string,fact4:string,fact5:string,fact6:string,fact7:number,fact8:number,fact9:number):Promise<{meta:{changes:number};success:boolean}>;
readRosterCommand6(fact0:string):Promise<(RosterAssignment)[]>;
readRosterCommand7(fact0:string):Promise<(RosterShift)[]>;
readRosterCommand8(fact0:string):Promise<(RosterAssignment)[]>;
applyRosterCommand5(fact0:string,fact1:number,fact2:string):Promise<{meta:{changes:number};success:boolean}>;
readRosterCommand9(fact0:string,fact1:string,fact2:string):Promise<Record<string,unknown>|null>;
readRosterCommand10(fact0:string):Promise<(RosterAssignment)[]>;
readRosterCommand11(fact0:string,fact1:string):Promise<Record<string,unknown>|null>;
applyRosterCommand6(fact0:string,fact1:string,fact2:string | null,fact3:string,fact4:string,fact5:string,fact6:string,fact7:number,fact8:number,fact9:number,fact10:number):Promise<{meta:{changes:number};success:boolean}>;
applyRosterCommand7(fact0:string):Promise<{meta:{changes:number};success:boolean}>;
applyRosterCommand8(fact0:number,fact1:string):Promise<{meta:{changes:number};success:boolean}>;
readRosterCommand12(fact0:string):Promise<(RosterAssignment)[]>;
applyRosterCommand9(fact0:string):Promise<{meta:{changes:number};success:boolean}>;
applyRosterCommand10(fact0:string):Promise<{meta:{changes:number};success:boolean}>;
readRosterCommand13(fact0:number,fact1:string):Promise<RosterWeek|null>;
readRosterCommand14(fact0:string):Promise<RosterAssignment|null>;
readRosterCommand15(fact0:string):Promise<RosterAssignment|null>;
readRosterCommand16(fact0:string,fact1:string,fact2:string):Promise<Record<string,unknown>|null>;
applyRosterCommand11(fact0:string,fact1:string,fact2:string,fact3:"extra" | "move" | "swap",fact4:string,fact5:string | null,fact6:string | null,fact7:number | null,fact8:string,fact9:string | null,fact10:number | null,fact11:"pending_coworker" | "pending_manager" | "approved" | "rejected" | "withdrawn" | "invalidated",fact12:string,fact13:string,fact14:number,fact15:number):Promise<{meta:{changes:number};success:boolean}>;
readRosterCommand17(fact0:string):Promise<(Record<string,unknown>)[]>;
applyRosterCommand12(fact0:string):Promise<{meta:{changes:number};success:boolean}>;
applyRosterCommand13(fact0:number,fact1:string):Promise<{meta:{changes:number};success:boolean}>;
applyRosterCommand14(fact0:"pending_manager" | "approved" | "rejected" | "withdrawn",fact1:string,fact2:"accept" | "approve" | "reject" | "withdraw",fact3:string,fact4:string,fact5:number,fact6:string):Promise<{meta:{changes:number};success:boolean}>;
readRosterData(fact0:string,fact1:string,fact2:boolean,fact3:string,fact4:string):Promise<(RosterWeek)[]>;
readRosterData2(fact0:string):Promise<(RosterShift)[]>;
readRosterData3(fact0:string):Promise<(RosterAssignment)[]>;
readRosterData4(fact0:string,fact1:string):Promise<(ShiftTemplate)[]>;
readRosterData5(fact0:string,fact1:string,fact2:boolean,fact3:string | null,fact4:string | null):Promise<(RosterRequest)[]>;
readRosterData6(fact0:string):Promise<RosterAssignment|null>;
readRosterData7(fact0:string,fact1:string,fact2:boolean,fact3:string | null,fact4:string,fact5:string | null,fact6:string | null):Promise<(RosterHistory)[]>;}type Tail<Value extends unknown[]> =Value extends [unknown,...infer Rest]?Rest:never;export interface PeopleRosterPorts<Effect>{repository:PeopleRosterRepository;clock:Clock;identifiers:IdentifierSource;today():string;commands:RosterCommands<Effect>;activeStaff(...args:Tail<Parameters<typeof activeStaff>>):ReturnType<typeof activeStaff>;rosterStaffPeople(...args:Tail<Parameters<typeof rosterStaffPeople>>):ReturnType<typeof rosterStaffPeople>;transaction<Result>(run:(ports:PeopleRosterPorts<Effect>)=>Promise<Result>):Promise<Result>;}export function createPeopleRosterApplication<Effect>(ports:PeopleRosterPorts<Effect>){const repository=ports.repository;async function self(actor: WorkContext) {
  try { return await ports.activeStaff(actor.companyId, actor.userId); }
  catch (e) { if (e instanceof BusinessRuleError && e.code === "STAFF_LINK_REQUIRED") return null; throw e; }
}
async function departments(actor: WorkContext, staff: StaffPerson | null, manager: boolean) {
  return repository.readDepartments(actor.departmentId,actor.companyId,actor.companyId,actor.companyId,actor.branchId,actor.branchId,actor.departmentId,manager,staff?.department_id ?? null);
}
async function department(actor: WorkContext, id: string, staff: StaffPerson | null, manager: boolean) {
  const found = (await departments(actor, staff, manager)).find(d => d.id === id);
  return found ?? fail("ROSTER_SCOPE", "This department is outside your roster access.", 403);
}
async function week(actor: WorkContext, id: string) {
  const found = await repository.readWeek(id,actor.companyId);
  return found ?? fail("NOT_FOUND", "Roster week not found.", 404);
}
async function shift(actor: WorkContext, id: string) {
  const found = await repository.readShift(id,actor.companyId);
  return found ?? fail("NOT_FOUND", "Shift not found.", 404);
}
async function times(date: string, start: string, end: string, timezone: string) {
  if (start === end) fail("ROSTER_TIME", "Start and end times must differ.", 422);
  const finish = end < start ? addDays(date, 1) : date;
  const v = await repository.readTimes(`${date} ${start}`,timezone,`${finish} ${end}`,timezone,`${date} ${start}`,timezone,timezone,`${finish} ${end}`,timezone,timezone);
  if (!v || v.local_start !== `${date} ${start}` || v.local_end !== `${finish} ${end}` || v.ends_at <= v.starts_at) fail("ROSTER_TIME", "This local time does not exist in the roster timezone.", 422);
  return { starts_at: v!.starts_at, ends_at: v!.ends_at };
}
async function eligible(w: Pick<RosterWeek, "company_id" | "branch_id" | "department_id">, personId: string, date: string) {
  const matches = (await ports.rosterStaffPeople(w.company_id, w.department_id, date, date)).filter(p => p.person_id === personId && p.branch_id === w.branch_id && p.roster_required === 1);
  if (matches.length !== 1) fail("ROSTER_STAFF", "Choose staff whose active Position requires a roster in this department.");
  return matches[0];
}
async function rosterGaps(companyId: string, departmentId: string, start: string) {
  return repository.readRosterGaps(start,companyId,departmentId,start,start,start,start,start,start,start);
}
async function escalateRosterCoverage(today:string) {
  const departments = await repository.readEscalateRosterCoverage();
  let missing = 0;
  for (const department of departments) for (const weekStart of [today, addDays(today, 7)]) {
    for (const gap of await rosterGaps(department.company_id, department.department_id, weekStart)) {
      missing++;
      await repository.applyEscalateRosterCoverage(department.department_id,department.company_id,department.company_id,`roster-coverage-${gap.engagement_id}-${weekStart}`,department.company_id,`${department.department_id}:${weekStart}:${gap.person_id}`,`${gap.legal_name} needs a roster shift for the week of ${weekStart}.`,ports.clock.now(),department.company_id,ports.clock.now(),ports.clock.now(),department.branch_id);
    }
  }
  return { missing };
}
async function requireCoverage(w:RosterWeek|ShiftWithWeek) {
  const gaps=await rosterGaps(w.company_id,w.department_id,w.week_start);
  if(gaps.length) fail("ROSTER_COVERAGE",`Assign at least one shift to: ${gaps.map(g=>g.legal_name).join(", ")}.`);
}
async function assignmentIssues(w: Pick<RosterWeek, "company_id" | "branch_id" | "department_id" | "timezone">, s: RosterShift, personId: string, exclude: string[] = [], engagementId?: string) {
  const issues: string[] = [];
  let p: StaffPerson;
  try { p = await eligible(w, personId, s.local_date); }
  catch (e) { if (e instanceof BusinessRuleError && e.code === "ROSTER_STAFF") return [e.message]; throw e; }
  if (engagementId && p.engagement_id !== engagementId) issues.push("The staff engagement changed; reassign this shift.");
  const dates = await repository.readAssignmentIssues(s.starts_at,w.timezone,s.ends_at,w.timezone);
  if (dates!.first_day < p.start_date || p.end_date && dates!.last_day > p.end_date) issues.push("Shift falls outside the staff engagement dates.");
  const leave = await repository.readAssignmentIssues2(w.company_id,personId,dates!.last_day,dates!.first_day);
  if (leave) issues.push("Approved leave overlaps this shift.");
  const collision = await repository.readAssignmentIssues3(w.company_id,personId,s.ends_at,s.starts_at,`{${exclude.join(",")}}`);
  if (collision) issues.push("Another assigned shift overlaps this time.");
  return issues;
}
async function validateAssignment(w: RosterWeek | ShiftWithWeek, s: RosterShift, personId: string, exclude: string[] = [], engagementId?: string) {
  const issues = await assignmentIssues(w, s, personId, exclude, engagementId);
  if (issues.length) fail("ROSTER_CONFLICT", issues.join(" "));
  return eligible(w, personId, s.local_date);
}
async function capacity(s: RosterShift, exclude: string[] = []) {
  const count = await repository.readCapacity(s.id,`{${exclude.join(",")}}`);
  if (count!.n >= s.capacity) fail("ROSTER_FULL", "This shift has no open slots remaining.");
}
async function assign(s: RosterShift, p: StaffPerson) {
  const id = ports.identifiers.next();
  await repository.applyAssign(id,s.id,p.person_id,p.engagement_id);
  return id;
}
async function notice(actor: WorkContext, type: "roster_week" | "roster_request", recordId: string, version: number, message: string, personIds: string[]) {
  for (const personId of new Set(personIds)) {
    const p = await repository.readNotice(personId,actor.companyId);
    if (p) await repository.applyNotice(ports.identifiers.next(),actor.companyId,p.user_id,type,recordId,version,message,ports.clock.now());
  }
}
async function notifyManagers(actor: WorkContext, r: RosterRequest, w: ShiftWithWeek) {
  const users = await repository.readNotifyManagers(w.department_id,actor.companyId,actor.companyId,actor.companyId,ports.clock.now(),ports.clock.now(),w.branch_id,`{${[r.requester_person_id, r.coworker_person_id].filter(Boolean).join(",")}}`);
  for (const u of users) await repository.applyNotifyManagers(ports.identifiers.next(),actor.companyId,u.user_id,r.id,r.version,"A roster change needs your review.",ports.clock.now());
}
async function history(actor: WorkContext, departmentId: string, recordId: string, action: string, reason: string, evidence: unknown, weekId: string | null = null, requestId: string | null = null) {
  await repository.applyHistory(ports.identifiers.next(),actor.companyId,departmentId,weekId,requestId,recordId,action,actor.userId,reason,JSON.stringify(evidence),ports.clock.now());
}
async function invalidate(actor: WorkContext, shiftIds: string[], assignmentIds: string[], except = "") {
  const changed = await repository.readInvalidate(ports.clock.now(),actor.companyId,except,`{${shiftIds.join(",")}}`,`{${assignmentIds.join(",")}}`,`{${assignmentIds.join(",")}}`);
  for (const r of changed) {
    await history(actor, r.department_id, r.id, "request.invalidated", r.decision_note, {}, null, r.id);
    await notice(actor, "roster_request", r.id, r.version, "Your roster request needs to be submitted again because its shift changed.", [r.requester_person_id, ...(r.coworker_person_id ? [r.coworker_person_id] : [])]);
  }
}
async function rescheduleRosterShift(
  actor: WorkContext,
  input: { id: string; expectedVersion: number; date: string; startTime: string; endTime: string; reason: string },
) {
  if (actor.beta) fail("ROSTER_UNAVAILABLE", "Rosters are unavailable in this workspace.", 403);
  if (
    !hasPermission(actor, "roster.manage") ||
    !(await repository.readRescheduleRosterShift(actor.role,actor.companyId,"roster.manage"))
  )
    fail("PERMISSION_REQUIRED", "Your selected role cannot perform this roster action.", 403);
  await repository.applyRescheduleRosterShift(actor.companyId);
  const current = await shift(actor, input.id);
  await department(actor, current.department_id, await self(actor), true);
  expected(current.version, input.expectedVersion);
  future(current,ports.clock.now());
  publishedReason(current, input.reason);
  if (current.state !== "active") fail("ROSTER_CHANGED", "The shift is no longer active.");
  if (input.date < current.week_start || input.date > addDays(current.week_start, 6))
    fail("ROSTER_DATE", "The shift must stay within its original roster week.", 422);
  const nextTimes = await times(input.date, input.startTime, input.endTime, current.timezone);
  const next: RosterShift = {
    ...current,
    local_date: input.date,
    start_time: input.startTime,
    end_time: input.endTime,
    ...nextTimes,
    version: current.version + 1,
  };
  future(next,ports.clock.now());
  const assignments = await repository.readRescheduleRosterShift2(current.id);
  for (const assignment of assignments)
    await validateAssignment(current, next, assignment.person_id, assignments.map((value) => value.id), assignment.engagement_id);
  await repository.applyRescheduleRosterShift2(next.local_date,next.start_time,next.end_time,next.starts_at,next.ends_at,next.id);
  await invalidate(actor, [next.id], assignments.map((assignment) => assignment.id));
  const updatedWeek = await repository.readRescheduleRosterShift3(ports.clock.now(),next.week_id);
  await history(actor, current.department_id, next.id, "shift.reschedule", input.reason, {
    from: { date: current.local_date, startTime: current.start_time, endTime: current.end_time },
    to: { date: next.local_date, startTime: next.start_time, endTime: next.end_time },
  }, next.week_id);
  if (current.week_state === "published")
    await notice(actor, "roster_week", next.week_id, updatedWeek!.version, `A published shift changed: ${next.name}, ${next.local_date}.`, assignments.map((assignment) => assignment.person_id));
  return { id: next.id, version: next.version, status: "active", warnings: [] as string[] };
}
async function readRequest(actor: WorkContext, id: string) {
  const r = await repository.readReadRequest(id,actor.companyId);
  return r ?? fail("NOT_FOUND", "Roster request not found.", 404);
}
async function requestParticipants(actor: WorkContext, r: RosterRequest) {
  const same = async (id: string | null) => !!id && !!await repository.readRequestParticipants(id,actor.companyId,actor.userId);
  return { own: await same(r.requester_person_id), coworker: await same(r.coworker_person_id) };
}
async function requestCheck(actor: WorkContext, r: RosterRequest) {
  const target = await shift(actor, r.target_shift_id);
  if (target.state !== "active" || target.week_state !== "published") fail("ROSTER_CHANGED", "The destination is no longer a published shift.");
  future(target,ports.clock.now());
  let source: ShiftWithWeek | null = null;
  let sourceAssignment: RosterAssignment | null = null;
  let targetAssignment: RosterAssignment | null = null;
  if (r.source_assignment_id) {
    sourceAssignment = await repository.readRequestCheck(r.source_assignment_id);
    if (!sourceAssignment || sourceAssignment.state !== "assigned" || sourceAssignment.person_id !== r.requester_person_id || sourceAssignment.version !== r.source_version) fail("ROSTER_CHANGED", "The original assignment changed.");
    source = await shift(actor, sourceAssignment!.shift_id);
    if (source.department_id !== target.department_id || source.branch_id !== target.branch_id || source.state !== "active" || source.week_state !== "published" || source.id === target.id) fail("ROSTER_SCOPE", "Choose a different published shift in the same department.");
    future(source,ports.clock.now());
  }
  if (r.kind === "swap") {
    targetAssignment = await repository.readRequestCheck2(r.target_assignment_id!);
    if (!targetAssignment || targetAssignment.state !== "assigned" || targetAssignment.shift_id !== target.id || targetAssignment.person_id !== r.coworker_person_id || targetAssignment.version !== r.target_version || r.coworker_person_id === r.requester_person_id) fail("ROSTER_CHANGED", "The coworker's assignment changed.");
  }
  const exclude = [sourceAssignment?.id, targetAssignment?.id].filter((x): x is string => !!x);
  const requester = await validateAssignment(target, target, r.requester_person_id, exclude, sourceAssignment?.engagement_id);
  await capacity(target, exclude);
  let coworker: StaffPerson | null = null;
  if (r.coworker_person_id && source) {
    coworker = await validateAssignment(source, source, r.coworker_person_id, exclude, targetAssignment?.engagement_id);
    await capacity(source, exclude);
  }
  return { target, source, sourceAssignment, targetAssignment, requester, coworker };
}
async function rosterCommand(actor: WorkContext, raw: unknown, key: string): Promise<Result> {
  const parsed = rosterCommandSchema.safeParse(raw);
  if (!parsed.success) fail("VALIDATION_FAILED", parsed.error.issues[0]?.message ?? "Invalid roster command.", 422);
  const input = parsed.data!;
  if (actor.beta) fail("ROSTER_UNAVAILABLE", "Rosters are unavailable in this workspace.", 403);
  const operation = `roster.${input.action}`;
  const recordId = "id" in input && input.id || `roster-${key}`;
  return ports.transaction(context=>createPeopleRosterApplication(context).applyCommand(actor,input,key,operation,recordId));
}
async function rosterData(actor: WorkContext, query: { department?: string; week?: string } = {}): Promise<RosterData> {
  if (actor.beta) fail("ROSTER_UNAVAILABLE", "Rosters are unavailable in this workspace.", 403);
  const canManage = hasPermission(actor, "roster.manage"), canApprove = hasPermission(actor, "roster.approve");
  const manager = canManage || canApprove || hasPermission(actor, "roster.read"), staff = await self(actor);
  const dd = await departments(actor, staff, manager);
  const selected = query.department ? dd.find(d => d.id === query.department) : dd.find(d => d.id === staff?.department_id) ?? dd[0];
  if (query.department && !selected) fail("ROSTER_SCOPE", "This department is outside your roster access.", 403);
  const start = query.week ?? rosterMonday(ports.today());
  if (!z.iso.date().safeParse(start).success || rosterMonday(start) !== start) fail("ROSTER_WEEK", "Choose a valid Monday.", 422);
  const data: RosterData = { asOf: ports.clock.now(), departments: dd, departmentId: selected?.id ?? null, weekStart: start, week: null, weeks: [], templates: [], shifts: [], assignments: [], requests: [], people: [], history: [], conflicts: [], coverageGaps: [], selfPersonId: staff?.roster_required===1?staff.person_id:null, selfRosterRequired:staff?.roster_required===1, canManage, canApprove };
  if (!selected) return data;
  data.weeks = await repository.readRosterData(actor.companyId,selected.id,manager,addDays(start, -35),addDays(start, 35));
  data.week = data.weeks.find(w => w.week_start === start) ?? null;
  const ids = `{${data.weeks.map(w => w.id).join(",")}}`;
  data.shifts = await repository.readRosterData2(ids);
  data.assignments = await repository.readRosterData3(ids);
  if (canManage) {
    data.templates = await repository.readRosterData4(actor.companyId,selected.id);
    data.people = (await ports.rosterStaffPeople(actor.companyId, selected.id, start, addDays(start,6))).filter(p=>p.roster_required===1).map(p => ({ person_id: p.person_id, legal_name: p.legal_name, engagement_id: p.engagement_id }));
    const current=rosterMonday(ports.today());
    data.coverageGaps = [...await rosterGaps(actor.companyId,selected.id,current),...await rosterGaps(actor.companyId,selected.id,addDays(current,7))];
  }
  data.requests = await repository.readRosterData5(actor.companyId,selected.id,manager,staff?.person_id ?? null,staff?.person_id ?? null);
  // Include referenced published shifts outside the selected calendar range for clear request details.
  for (const r of data.requests) {
    for (const assignmentId of [r.source_assignment_id, r.target_assignment_id].filter((x): x is string => !!x)) {
      if (!data.assignments.some(a => a.id === assignmentId)) {
        const a = await repository.readRosterData6(assignmentId);
        if (a) data.assignments.push(a);
      }
    }
    const source = data.assignments.find(a => a.id === r.source_assignment_id);
    for (const shiftId of [r.target_shift_id, source?.shift_id].filter((x): x is string => !!x)) {
      if (!data.shifts.some(s => s.id === shiftId)) { const s = await shift(actor, shiftId); data.shifts.push(s); }
    }
    const who = await requestParticipants(actor, r);
    r.permittedActions = rosterPending(r.state) ? [
      ...(who.own ? ["withdraw"] : []),
      ...(who.coworker && r.state === "pending_coworker" ? ["accept", "reject"] : []),
      ...(canApprove && !who.own && !who.coworker && r.state === "pending_manager" ? ["approve", "reject"] : []),
    ] : [];
  }
  data.history = await repository.readRosterData7(actor.companyId,selected.id,manager,data.week?.id ?? null,actor.companyId,staff?.person_id ?? null,staff?.person_id ?? null);
  if (data.week) for (const s of data.shifts.filter(s => s.week_id === data.week!.id && s.state === "active")) {
    const aa = data.assignments.filter(a => a.shift_id === s.id && a.state === "assigned");
    if (aa.length > s.capacity) data.conflicts.push({ shiftId: s.id, message: "Required headcount is exceeded." });
    for (const a of aa) {
      if (!manager && a.person_id !== staff?.person_id) continue;
      const issues = await assignmentIssues(data.week, s, a.person_id, [a.id], a.engagement_id);
      for (const message of issues) data.conflicts.push({ shiftId: s.id, personId: a.person_id, message: `${a.person_name}: ${message}` });
    }
  }
  return data;
}
async function rosterNotificationLink(actor: WorkContext, type: string, id: string) {
  const staff = await self(actor), manager = ["roster.read", "roster.manage", "roster.approve"].some(p => hasPermission(actor, p));
  let w: RosterWeek;
  if (type === "roster_request") {
    const r = await readRequest(actor, id), who = await requestParticipants(actor, r);
    if (!manager && !who.own && !who.coworker) fail("NOT_FOUND", "Request not found.", 404);
    const s = await shift(actor, r.target_shift_id); w = await week(actor, s.week_id);
  } else { w = await week(actor, id); if (!manager && w.state !== "published") fail("NOT_FOUND", "Roster not found.", 404); }
  await department(actor, w.department_id, staff, manager);
  return `/people/roster?department=${encodeURIComponent(w.department_id)}&week=${w.week_start}${type === "roster_request" ? `&view=requests&request=${encodeURIComponent(id)}` : ""}`;
}
async function applyCommand(actor:WorkContext,input:z.infer<typeof rosterCommandSchema>,key:string,operation:string,recordId:string):Promise<Result>{
    // The company revision serializes capacity and overlap checks across different weeks.
    // SERIALIZABLE retries recompute every check after a concurrent change.
    const replay = await ports.commands.replay<Result>(actor, key, operation, recordId, input);
    if (replay) return replay;
    await repository.applyRosterCommand(actor.companyId);
    const staff = await self(actor);
    const manager = ["roster.read", "roster.manage", "roster.approve"].some(p => hasPermission(actor, p));
    let departmentId = "", weekId: string | null = null, requestId: string | null = null;
    let result: Result;
    const need = async (permission: string) => {
      if (!hasPermission(actor, permission) || !await repository.readRosterCommand(actor.role,actor.companyId,permission)) fail("PERMISSION_REQUIRED", "Your selected role cannot perform this roster action.", 403);
    };
    if (!input.action.startsWith("request.")) await need("roster.manage");
    if (input.action === "template.save") {
      await department(actor, input.departmentId, staff, true);
      departmentId = input.departmentId;
      if (input.startTime === input.endTime) fail("ROSTER_TIME", "Start and end times must differ.", 422);
      const old = input.id ? await repository.readRosterCommand2(input.id,actor.companyId,departmentId) : null;
      if (input.id && !old) fail("NOT_FOUND", "Template not found.", 404);
      if (old) expected(old.version, input.expectedVersion);
      result = { id: recordId, version: (old?.version ?? 0) + 1 };
      await repository.applyRosterCommand2(result.id,actor.companyId,departmentId,input.name,input.startTime,input.endTime,input.capacity,input.active,result.version,actor.userId,ports.clock.now());
    } else if (input.action === "week.create") {
      const d = await department(actor, input.departmentId, staff, true);
      departmentId = d.id;
      if (rosterMonday(input.weekStart) !== input.weekStart) fail("ROSTER_WEEK", "Choose the Monday beginning the roster week.", 422);
      if (!await repository.readRosterCommand3(input.timezone)) fail("ROSTER_TIMEZONE", "Choose a valid IANA timezone.", 422);
      if (await repository.readRosterCommand4(actor.companyId,d.id,input.weekStart)) fail("ROSTER_EXISTS", "This department already has a roster for that week.");
      const w: RosterWeek = { id: recordId, company_id: actor.companyId, branch_id: d.branch_id, department_id: d.id, week_start: input.weekStart, timezone: input.timezone, state: "draft", version: 1 };
      await repository.applyRosterCommand3(w.id,w.company_id,w.branch_id,w.department_id,w.week_start,w.timezone,actor.userId,ports.clock.now(),ports.clock.now());
      if (input.copyWeekId) {
        const source = await week(actor, input.copyWeekId);
        if (source.department_id !== w.department_id) fail("ROSTER_SCOPE", "Copy a week from the same department.");
        const offset = Math.round((Date.parse(w.week_start) - Date.parse(source.week_start)) / 86400000);
        for (const s of await repository.readRosterCommand5(source.id)) {
          const date = addDays(s.local_date, offset), t = await times(date, s.start_time, s.end_time, w.timezone);
          const copy: RosterShift = { ...s, id: ports.identifiers.next(), week_id: w.id, local_date: date, ...t, version: 1 };
          future(copy,ports.clock.now());
          await repository.applyRosterCommand4(copy.id,w.id,s.template_id,s.name,date,s.start_time,s.end_time,t.starts_at,t.ends_at,s.capacity);
          for (const a of await repository.readRosterCommand6(s.id)) await assign(copy, await validateAssignment(w, copy, a.person_id));
        }
      }
      weekId = w.id; result = { id: w.id, version: 1, state: "draft" };
    } else if (input.action === "week.publish") {
      const w = await week(actor, input.id);
      await department(actor, w.department_id, staff, true); departmentId = w.department_id; weekId = w.id;
      expected(w.version, input.expectedVersion);
      if (w.state !== "draft") fail("ROSTER_CHANGED", "This roster is already published.");
      const shifts = await repository.readRosterCommand7(w.id);
      if (!shifts.length) fail("ROSTER_EMPTY", "Add at least one shift before publishing.");
      const assigned: string[] = [];
      for (const s of shifts) {
        future(s,ports.clock.now());
        const aa = await repository.readRosterCommand8(s.id);
        if (aa.length > s.capacity) fail("ROSTER_FULL", "A shift exceeds its required headcount.");
        for (const a of aa) { await validateAssignment(w, s, a.person_id, [a.id], a.engagement_id); assigned.push(a.person_id); }
      }
      await requireCoverage(w);
      result = { id: w.id, version: w.version + 1, state: "published" };
      await repository.applyRosterCommand5(actor.userId,ports.clock.now(),w.id);
      // Include department members so open slots are discoverable even without an assignment.
      await notice(actor, "roster_week", w.id, result.version, `The roster for ${w.week_start} is published.`, [...assigned, ...(await ports.rosterStaffPeople(actor.companyId, departmentId, w.week_start, addDays(w.week_start, 6))).filter(p=>p.roster_required===1).map(p => p.person_id)]);
    } else if (input.action === "shift.save") {
      const w = await week(actor, input.weekId);
      await department(actor, w.department_id, staff, true); departmentId = w.department_id; weekId = w.id;
      expected(w.version, input.expectedVersion); publishedReason(w, input.reason);
      if (input.date < w.week_start || input.date > addDays(w.week_start, 6)) fail("ROSTER_DATE", "The shift must start within this roster week.", 422);
      if (new Set(input.personIds).size !== input.personIds.length || input.personIds.length > input.capacity) fail("ROSTER_CAPACITY", "Assign each staff member once, within the shift headcount.", 422);
      if (input.templateId && !await repository.readRosterCommand9(input.templateId,actor.companyId,w.department_id)) fail("ROSTER_TEMPLATE", "Choose an active template from this department.");
      const old = input.id ? await shift(actor, input.id) : null;
      if (old && (old.week_id !== w.id || old.state !== "active")) fail("ROSTER_CHANGED", "The shift is unavailable in this week.");
      if (old) future(old,ports.clock.now());
      const t = await times(input.date, input.startTime, input.endTime, w.timezone);
      const s: RosterShift = { id: recordId, week_id: w.id, template_id: input.templateId ?? null, name: input.name, local_date: input.date, start_time: input.startTime, end_time: input.endTime, ...t, capacity: input.capacity, state: "active", version: (old?.version ?? 0) + 1 };
      future(s,ports.clock.now());
      const oldAssignments = old ? await repository.readRosterCommand10(old.id) : [];
      const people: StaffPerson[] = [];
      for (const personId of [...input.personIds].sort()) {
        await repository.readRosterCommand11(personId,actor.companyId);
        people.push(await validateAssignment(w, s, personId, oldAssignments.map(a => a.id)));
      }
      await repository.applyRosterCommand6(s.id,w.id,s.template_id,s.name,s.local_date,s.start_time,s.end_time,s.starts_at,s.ends_at,s.capacity,s.version);
      await repository.applyRosterCommand7(s.id);
      for (const p of people) await assign(s, p);
      await invalidate(actor, [s.id], oldAssignments.map(a => a.id));
      await repository.applyRosterCommand8(ports.clock.now(),w.id);
      if(w.state==="published") await requireCoverage(w);
      if (w.state === "published") await notice(actor, "roster_week", w.id, w.version + 1, `A published shift changed: ${s.name}, ${s.local_date}.`, [...input.personIds, ...oldAssignments.map(a => a.person_id)]);
      result = { id: s.id, version: s.version };
    } else if (input.action === "shift.cancel") {
      const s = await shift(actor, input.id);
      await department(actor, s.department_id, staff, true); departmentId = s.department_id; weekId = s.week_id;
      expected(s.version, input.expectedVersion); future(s,ports.clock.now()); publishedReason(s, input.reason);
      if (s.state !== "active") fail("ROSTER_CHANGED", "The shift was already cancelled.");
      const aa = await repository.readRosterCommand12(s.id);
      await repository.applyRosterCommand9(s.id);
      await repository.applyRosterCommand10(s.id);
      await invalidate(actor, [s.id], aa.map(a => a.id));
      const w = await repository.readRosterCommand13(ports.clock.now(),s.week_id);
      if(s.week_state==="published") await requireCoverage(w!);
      if (s.week_state === "published") await notice(actor, "roster_week", s.week_id, w!.version, `${s.name} on ${s.local_date} was cancelled.`, aa.map(a => a.person_id));
      result = { id: s.id, version: s.version + 1, state: "cancelled" };
    } else if (input.action === "request.create") {
      if (!staff || !hasPermission(actor, "self.roster")) fail("STAFF_LINK_REQUIRED", "An active staff profile is required.", 403);
      if (staff!.roster_required !== 1) fail("ROSTER_STAFF", "Your Position does not require roster shifts.", 403);
      const target = await shift(actor, input.targetShiftId);
      await department(actor, target.department_id, staff, false); departmentId = target.department_id;
      const source = input.sourceAssignmentId ? await repository.readRosterCommand14(input.sourceAssignmentId) : null;
      const dest = input.targetAssignmentId ? await repository.readRosterCommand15(input.targetAssignmentId) : null;
      if (input.kind === "extra" && input.sourceAssignmentId || input.kind !== "extra" && !source || input.kind === "swap" && !dest || input.kind !== "swap" && input.targetAssignmentId) fail("ROSTER_REQUEST", "Choose the assignments required for this request.", 422);
      const r = { id: recordId, company_id: actor.companyId, department_id: target.department_id, kind: input.kind, requester_person_id: staff!.person_id, coworker_person_id: dest?.person_id ?? null, source_assignment_id: source?.id ?? null, source_version: source?.version ?? null, target_shift_id: target.id, target_assignment_id: dest?.id ?? null, target_version: dest?.version ?? null, state: input.kind === "swap" ? "pending_coworker" : "pending_manager", reason: input.reason, decision_note: "", version: 1 } as RosterRequest;
      await requestCheck(actor, r);
      if (await repository.readRosterCommand16(actor.companyId,r.requester_person_id,target.id)) fail("ROSTER_DUPLICATE", "You already have a pending request for this shift.");
      await repository.applyRosterCommand11(r.id,r.company_id,r.department_id,r.kind,r.requester_person_id,r.coworker_person_id,r.source_assignment_id,r.source_version,r.target_shift_id,r.target_assignment_id,r.target_version,r.state,r.reason,actor.userId,ports.clock.now(),ports.clock.now());
      requestId = r.id;
      if (r.coworker_person_id) await notice(actor, "roster_request", r.id, 1, "A coworker requested a shift swap with you.", [r.coworker_person_id]);
      else await notifyManagers(actor, r, target);
      result = { id: r.id, version: 1, state: r.state };
    } else {
      const r = await readRequest(actor, input.id);
      await department(actor, r.department_id, staff, manager); departmentId = r.department_id; requestId = r.id;
      expected(r.version, input.expectedVersion);
      if (!rosterPending(r.state)) fail("ROSTER_CHANGED", "This request is no longer pending.");
      const who = await requestParticipants(actor, r);
      let next: RosterRequest["state"];
      if (input.decision === "withdraw") {
        if (!who.own) fail("ROSTER_PARTICIPANT", "Only the requester can withdraw this request.", 403);
        next = "withdrawn";
      } else if (input.decision === "accept") {
        if (!who.coworker || r.state !== "pending_coworker") fail("ROSTER_PARTICIPANT", "Only the selected coworker can accept this swap.", 403);
        await requestCheck(actor, r); next = "pending_manager";
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
          const c = await requestCheck(actor, r);
          const aa = [c.sourceAssignment, c.targetAssignment].filter((a): a is RosterAssignment => !!a);
          await repository.readRosterCommand17(`{${[c.target.id, c.source?.id].filter(Boolean).join(",")}}`);
          for (const a of aa) await repository.applyRosterCommand12(a.id);
          await assign(c.target, c.requester);
          if (c.coworker && c.source) await assign(c.source, c.coworker);
          await invalidate(actor, [], aa.map(a => a.id), r.id);
          await repository.applyRosterCommand13(ports.clock.now(),`{${[c.target.week_id, c.source?.week_id].filter(Boolean).join(",")}}`);
          for(const weekId of new Set([c.target.week_id,c.source?.week_id].filter((id):id is string=>!!id))) await requireCoverage(await week(actor,weekId));
          next = "approved";
        }
      }
      result = { id: r.id, version: r.version + 1, state: next };
      await repository.applyRosterCommand14(next,input.reason,input.decision,actor.userId,actor.userId,ports.clock.now(),r.id);
      await notice(actor, "roster_request", r.id, result.version, next === "pending_manager" ? "The coworker accepted your swap. A manager will review it." : `Your roster request was ${next}.`, [r.requester_person_id, ...(r.coworker_person_id ? [r.coworker_person_id] : [])]);
      if (next === "pending_manager") await notifyManagers(actor, { ...r, version: result.version }, await shift(actor, r.target_shift_id));
    }
    const reason = "reason" in input ? input.reason : "";
    await history(actor, departmentId, result.id, input.action, reason, input, weekId, requestId);
    return ports.commands.commit(actor, key, operation, input, recordId, result.version, result, [], { companyId: actor.companyId, branchId: (await department(actor, departmentId, staff, manager)).branch_id, departmentId });
  }return {escalateRosterCoverage,rescheduleRosterShift,rosterCommand,rosterData,rosterNotificationLink,applyCommand};}
