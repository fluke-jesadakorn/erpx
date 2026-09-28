import { requireClearanceCompletion, requireClearancePlan } from "../retirement-clearance";
import type { PreparationTask } from "@/lib/people-lifecycle";
import "server-only";
import { calendarCommandSchema, type CalendarCommand } from "@/lib/calendar";
import { hasPermission } from "@/lib/permissions";
import type { WorkContext } from "../authorization";
import type { PostgresDatabase } from "../runtime/postgres";
import type { SqlDatabase } from "../runtime/types";
import { executeCommand, replayCommand } from "../commands";
import {
  calendarAccess,
  requirePlacement,
  needPermission,
  one,
  run,
  rows,
  fail,
  type CalendarAccess,
} from "./access";
import {
  appointmentWarningMessages,
  type AppointmentRow,
} from "./appointments";
import type { AnnouncementRow } from "./announcements";
import { activeStaffSelect } from "../staff-policy";
import { rescheduleRosterShift } from "../people-roster";
import { publishInboxEvent } from "../inbox";
import { addDays } from "@/lib/people-roster";
import { expandCalendarSeries, type RecurrenceRule } from "@/lib/calendar-recurrence";
import { localDateTimeToInstant, localMinuteAt } from "@/lib/calendar-timeline";
import { prepareAttachmentBinding } from "../attachments/service";
type Result = {
  id: string;
  version: number;
  status: string;
  warnings: string[];
};
export async function localInterval(
  db: SqlDatabase,
  start: string,
  end: string,
  zone: string,
) {
  const found = await one<{
    starts_at: number;
    ends_at: number;
    start_local: string;
    end_local: string;
  }>(
    db,
    `SELECT (extract(epoch FROM (?::timestamp AT TIME ZONE ?))*1000)::bigint starts_at,(extract(epoch FROM (?::timestamp AT TIME ZONE ?))*1000)::bigint ends_at,to_char((?::timestamp AT TIME ZONE ?) AT TIME ZONE ?,'YYYY-MM-DD"T"HH24:MI') start_local,to_char((?::timestamp AT TIME ZONE ?) AT TIME ZONE ?,'YYYY-MM-DD"T"HH24:MI') end_local`,
    start,
    zone,
    end,
    zone,
    start,
    zone,
    zone,
    end,
    zone,
    zone,
  );
  if (
    !found ||
    found.start_local !== start ||
    found.end_local !== end ||
    found.ends_at <= found.starts_at ||
    found.ends_at - found.starts_at > 31 * 86400000
  )
    fail(
      "Choose valid local times, with the end after the start (up to 31 days).",
      "CALENDAR_TIME",
      422,
    );
  return found;
}
function expected(actual: number, supplied: number | undefined) {
  if (actual !== supplied)
    fail(
      "This record changed. Refresh before trying again.",
      "VERSION_CONFLICT",
      409,
    );
}
async function eligiblePerson(
  db: SqlDatabase,
  a: CalendarAccess,
  id: string,
  scopeRequired: boolean,
) {
  const found = await rows<{
    person_id: string;
    user_id: string;
    department_id: string | null;
    branch_id: string | null;
  }>(
    db,
    `${activeStaffSelect} AND p.id=?`,
    a.actor.companyId,
    Date.now(),
    Date.now(),
    id,
  );
  if (found.length !== 1)
    fail(
      "Choose a colleague with one active engagement and account.",
      "CALENDAR_STAFF",
      422,
    );
  if (scopeRequired)
    requirePlacement(a, found[0].branch_id, found[0].department_id);
  return found[0];
}
async function manageAppointment(
  db: SqlDatabase,
  a: CalendarAccess,
  r: AppointmentRow,
) {
  requirePlacement(a, r.branch_id, r.department_id);
  if (r.created_by!==a.actor.userId && r.organizer_person_id !== a.staff?.person_id)
    await needPermission(db, a.actor, "calendar.appointments.manage");
  if (r.status !== "scheduled")
    fail(
      "Only scheduled appointments can be changed.",
      "INVALID_TRANSITION",
      409,
    );
}
type SeriesRow={frequency:RecurrenceRule["frequency"];interval_count:number;weekdays:string;end_kind:RecurrenceRule["endKind"];occurrence_count:number|null;until_date:string|null};
async function seriesFor(db:SqlDatabase,id:string){return db.prepare("SELECT frequency,interval_count,weekdays,end_kind,occurrence_count,until_date FROM calendar_series WHERE appointment_id=?")
  .bind(id).first<SeriesRow>();}
async function appointmentUsers(db:SqlDatabase,companyId:string,id:string){
  return (await db.prepare("SELECT user_id FROM calendar_attendees WHERE appointment_id=? AND company_id=?")
    .bind(id,companyId).all<{user_id:string}>()).results.map(row=>row.user_id);
}
async function calendarNotice(db:SqlDatabase,actor:WorkContext,id:string,version:number,kind:string,preview:string,users:string[],sourceType="calendar_appointment"){
  if(!users.length)return;
  await publishInboxEvent(db,{key:`calendar:${id}:${version}:${kind}`,companyId:actor.companyId,
    sourceType,sourceId:id,sourceVersion:version,actorUserId:actor.userId,category:"calendar",
    preview,recipientIds:users.filter(user=>user!==actor.userId)});
}
function ruleFrom(row:SeriesRow):RecurrenceRule{return {frequency:row.frequency,intervalCount:row.interval_count,
  weekdays:row.weekdays?row.weekdays.split(",").map(Number):[],endKind:row.end_kind,
  occurrenceCount:row.occurrence_count,untilDate:row.until_date};}
async function assertOccurrence(old:AppointmentRow,row:SeriesRow,key:string){
  const original=localMinuteAt(old.starts_at,old.timezone),finish=localMinuteAt(old.ends_at,old.timezone);
  const from=localDateTimeToInstant(key,old.timezone);
  const occurrences=expandCalendarSeries(`${original.date}T${original.time}`,`${finish.date}T${finish.time}`,
    old.timezone,ruleFrom(row),from,from+60000,1);
  if(occurrences[0]?.key!==key)fail("This occurrence is unavailable.","OCCURRENCE_NOT_FOUND",404);
}
async function taskCommand(
  db: SqlDatabase,
  a: CalendarAccess,
  input: Extract<CalendarCommand, { action: "task.plan" | "task.complete" }>,
  recordId: string,
  commandId: string,
): Promise<Result> {
  const t = await one<PreparationTask & {
    id: string;
    kind: string;
    status: string;
    version: number;
    privileged: boolean;
    assignee_id: string | null;
    branch_id: string | null;
    department_id: string | null;
  }>(
    db,
    `SELECT t.*,coalesce(e.branch_id,p.branch_id) branch_id,coalesce(e.department_id,p.department_id) department_id FROM people_preparation_tasks t LEFT JOIN people_lifecycle_cases c ON c.id=t.case_id AND c.company_id=t.company_id LEFT JOIN hr_engagements e ON e.id=c.engagement_id AND e.company_id=c.company_id LEFT JOIN organization_positions p ON p.id=t.position_id AND p.company_id=t.company_id WHERE t.id=? AND t.company_id=?`,
    input.id,
    a.actor.companyId,
  );
  if (!t) fail("Task not found.", "NOT_FOUND", 404);
  requirePlacement(a, t.branch_id, t.department_id);
  expected(t.version, input.expectedVersion);
  if (t.status !== "pending")
    fail(
      "Only pending preparation tasks can be changed.",
      "INVALID_TRANSITION",
      409,
    );
  const it =
    hasPermission(a.actor, "iam.directory.manage") &&
    !a.actor.branchId &&
    !a.actor.departmentId;
  if (input.action === "task.plan") {
    if (t.clearance_type) {
      await requireClearancePlan(db,a.actor,t,input.assigneeId);
      if (!input.dueDate) fail("Clearance tasks need a due date.","CLEARANCE_DUE_DATE",422);
    }
    else if (t.privileged || !hasPermission(a.actor, "organization.manage")) {
      if (!it) fail("This preparation task requires Company IT authority.");
      await needPermission(db, a.actor, "iam.directory.manage");
    } else await needPermission(db, a.actor, "organization.manage");
    if (input.assigneeId && !t.clearance_type) {
      if (t.privileged) {
        if (
          !(await one(
            db,
            `SELECT 1 FROM assignments WHERE company_id=? AND user_id=? AND scope_kind='holding' AND iam_role_allows(role,company_id,'iam.directory.manage') AND iam_assignment_active(id,?) LIMIT 1`,
            a.actor.companyId,
            input.assigneeId,
            Date.now(),
          ))
        )
          fail(
            "Access and directory tasks require an active Company IT assignee.",
            "CALENDAR_ASSIGNEE",
            422,
          );
      } else {
        const p = await one<{ id: string }>(
          db,
          "SELECT id FROM hr_people WHERE company_id=? AND user_id=? AND merged_into_id IS NULL",
          a.actor.companyId,
          input.assigneeId,
        );
        if (!p)
          fail(
            "Choose an active staff assignee in this Company.",
            "CALENDAR_ASSIGNEE",
            422,
          );
        await eligiblePerson(db, a, p.id, false);
      }
    }
    await run(
      db,
      "UPDATE people_preparation_tasks SET due_date=?,assignee_id=?,responsibility=?,version=version+1,updated_at=? WHERE id=? AND company_id=?",
      input.dueDate,
      input.assigneeId,
      input.responsibility,
      Date.now(),
      input.id,
      a.actor.companyId,
    );
    if (t.clearance_type && input.assigneeId && input.assigneeId !== t.assignee_id) await run(db,"INSERT INTO staff_notifications(id,company_id,user_id,record_type,record_id,record_version,message,created_at) VALUES(?,?,?,'retirement_clearance',?,?,?,?) ON CONFLICT DO NOTHING",`${t.id}-assignment-${t.version+1}`,a.actor.companyId,input.assigneeId,t.id,t.version+1,t.title,Date.now());
  } else {
    if (t.kind === "position")
      fail(
        "Position preparation completes through the access package workflow.",
        "PRIVILEGED_WORKFLOW",
        409,
      );
    if (!t.clearance_type && input.status === "not_required") fail("Not required is available for retirement clearance tasks.","INVALID_TRANSITION",422);
    if (t.clearance_type) await requireClearanceCompletion(db,a.actor,t,input);
    else if (
      t.privileged ||
      !a.staff ||
      !(await one(
        db,
        "SELECT 1 WHERE iam_same_person(?,?,?)",
        t.assignee_id,
        a.actor.userId,
        a.actor.companyId,
      ))
    ) {
      if (!it)
        fail(
          "Only the assigned colleague can complete this task; IT work requires Company IT authority.",
        );
      await needPermission(db, a.actor, "iam.directory.manage");
    }
    const attachmentBinding =
      t.clearance_type && input.attachmentSetIds.length
        ? await prepareAttachmentBinding(db, a.actor, {
            attachmentSetIds: input.attachmentSetIds,
            bindingContract: {
              targetType: "people_clearance_task",
              bindingType: "retirement_clearance_completion",
              contexts: [{ purposeKey: "clearance_evidence", componentKind: "action", componentRef: String(t.clearance_type) }],
            },
            targetId: t.id,
            expectedTargetVersion: input.expectedVersion,
            commandId,
            bindingId: t.id,
          })
        : null;
    if (attachmentBinding) await db.batch(attachmentBinding.guards);
    await run(
      db,
      "UPDATE people_preparation_tasks SET status=?,completion_reason=?,document_references=?::jsonb,completed_by=?,completed_at=?,version=version+1,updated_at=? WHERE id=? AND company_id=?",
      input.status,
      input.reason,
      JSON.stringify(input.references),
      a.actor.userId,
      Date.now(),
      Date.now(),
      input.id,
      a.actor.companyId,
    );
    if (attachmentBinding) await db.batch(attachmentBinding.steps);
  }
  return {
    id: recordId,
    version: t.version + 1,
    status: input.action === "task.complete" ? input.status : "pending",
    warnings: [],
  };
}
export async function calendarCommand(
  database: PostgresDatabase,
  actor: WorkContext,
  raw: unknown,
  key: string,
): Promise<Result> {
  const input = calendarCommandSchema.parse(raw),
    operation = `calendar.${input.action}`,
    recordId = ("id" in input && input.id) || `calendar-${key}`;
  if (!/^[a-zA-Z0-9:_.-]{8,180}$/.test(key))
    fail("Use a valid Idempotency-Key.", "IDEMPOTENCY_KEY_REQUIRED", 422);
  return database.transaction(async (db) => {
    const replay = await replayCommand<Result>(
      db,
      actor,
      key,
      operation,
      recordId,
      input,
    );
    if (replay) return replay;
    const a = await calendarAccess(db, actor);
    let result: Result;
    if (input.action === "appointment.reschedule") {
      const old = await one<AppointmentRow>(
        db,
        "SELECT * FROM calendar_appointments WHERE id=? AND company_id=?",
        input.id,
        actor.companyId,
      );
      if (!old) fail("Appointment not found.", "NOT_FOUND", 404);
      await manageAppointment(db, a, old);
      expected(old.version, input.expectedVersion);
      const times = await localInterval(db, input.startLocal, input.endLocal, input.timezone);
      const series=await seriesFor(db,input.id);
      if(input.occurrenceId&&series)await assertOccurrence(old,series,input.occurrenceId);
      const participants = await rows<{ person_id: string }>(
        db,
        "SELECT person_id FROM calendar_appointment_participants WHERE appointment_id=? AND company_id=?",
        input.id,
        actor.companyId,
      );
      const warnings = await appointmentWarningMessages(
        db,
        actor.companyId,
        participants.map((participant) => participant.person_id),
        times.starts_at,
        times.ends_at,
        input.timezone,
        input.id,
      );
      let futureSplitId:string|null=null;
      if(series&&input.occurrenceId&&input.scope==="single"){
        await run(db,`INSERT INTO calendar_occurrence_exceptions(company_id,appointment_id,occurrence_key,start_local,end_local)
          VALUES(?,?,?,?,?) ON CONFLICT(appointment_id,occurrence_key) DO UPDATE SET start_local=excluded.start_local,end_local=excluded.end_local,cancelled=false,version=calendar_occurrence_exceptions.version+1`,
          actor.companyId,input.id,input.occurrenceId,input.startLocal,input.endLocal);
        await run(db,"UPDATE calendar_appointments SET version=version+1,updated_at=? WHERE id=? AND company_id=?",Date.now(),input.id,actor.companyId);
      }else if(series&&input.occurrenceId&&input.scope==="future"){
        const newId=`calendar-${key}`,before=addDays(input.occurrenceId.slice(0,10),-1);
        futureSplitId=newId;
        const original=localMinuteAt(old.starts_at,old.timezone),finish=localMinuteAt(old.ends_at,old.timezone);
        let remaining=series.occurrence_count;
        if(series.end_kind==="count"&&remaining){
          const ordinal=expandCalendarSeries(`${original.date}T${original.time}`,`${finish.date}T${finish.time}`,
            old.timezone,ruleFrom(series),old.starts_at,localDateTimeToInstant(input.occurrenceId,old.timezone)+1,1000).length;
          remaining=Math.max(1,remaining-ordinal+1);
        }
        await run(db,"UPDATE calendar_series SET end_kind='until',until_date=?,occurrence_count=NULL,version=version+1 WHERE appointment_id=?",before,input.id);
        await run(db,"UPDATE calendar_appointments SET version=version+1,updated_at=? WHERE id=? AND company_id=?",Date.now(),input.id,actor.companyId);
        await run(db,`INSERT INTO calendar_appointments(id,company_id,branch_id,department_id,organizer_person_id,title,description,starts_at,ends_at,timezone,location,meeting_url,private,guests,status,version,created_by,created_at,updated_at,all_day,start_date,end_date)
          SELECT ?,company_id,branch_id,department_id,organizer_person_id,title,description,?,?,?,location,meeting_url,private,guests,status,1,created_by,?,?,?,?,?
          FROM calendar_appointments WHERE id=? AND company_id=?`,newId,times.starts_at,times.ends_at,input.timezone,Date.now(),Date.now(),old.all_day,
          old.all_day?input.startLocal.slice(0,10):null,old.all_day?input.endLocal.slice(0,10):null,input.id,actor.companyId);
        await run(db,`INSERT INTO calendar_series(appointment_id,company_id,frequency,interval_count,weekdays,end_kind,occurrence_count,until_date)
          VALUES(?,?,?,?,?,?,?,?)`,newId,actor.companyId,series.frequency,series.interval_count,series.weekdays,
          series.end_kind,series.end_kind==="count"?remaining:null,series.until_date);
        await run(db,`INSERT INTO calendar_attendees(company_id,appointment_id,user_id,person_id,attendance,response,reminder_minutes)
          SELECT company_id,?,user_id,person_id,attendance,CASE WHEN attendance='organizer' THEN 'accepted' ELSE 'pending' END,reminder_minutes
          FROM calendar_attendees WHERE appointment_id=? AND company_id=?`,newId,input.id,actor.companyId);
        await run(db,`INSERT INTO calendar_appointment_participants(company_id,appointment_id,person_id)
          SELECT company_id,?,person_id FROM calendar_appointment_participants WHERE appointment_id=? AND company_id=?`,newId,input.id,actor.companyId);
      }else{
        await run(db,"UPDATE calendar_appointments SET starts_at=?,ends_at=?,timezone=?,start_date=?,end_date=?,version=version+1,updated_at=? WHERE id=? AND company_id=?",
          times.starts_at,times.ends_at,input.timezone,old.all_day?input.startLocal.slice(0,10):null,
          old.all_day?input.endLocal.slice(0,10):null,Date.now(),input.id,actor.companyId);
      }
      result = { id: input.id, version: old.version + 1, status: old.status, warnings };
      await calendarNotice(db,actor,input.id,result.version,"moved",`Meeting time changed: ${old.title}`,
        (await appointmentUsers(db,actor.companyId,input.id)).filter(user=>user!==actor.userId));
      if(futureSplitId)await calendarNotice(db,actor,futureSplitId,1,"invited",`Meeting invitation: ${old.title}`,
        (await appointmentUsers(db,actor.companyId,futureSplitId)).filter(user=>user!==actor.userId));
    } else if (input.action === "announcement.reschedule") {
      await needPermission(db, actor, "calendar.announcements.publish");
      const old = await one<AnnouncementRow>(
        db,
        "SELECT * FROM calendar_announcements WHERE id=? AND company_id=?",
        input.id,
        actor.companyId,
      );
      if (!old) fail("Announcement not found.", "NOT_FOUND", 404);
      requirePlacement(a, old.branch_id, old.department_id);
      expected(old.version, input.expectedVersion);
      if (!["draft", "published"].includes(old.status))
        fail("Only active announcements can be rescheduled.", "INVALID_TRANSITION", 409);
      await run(
        db,
        "UPDATE calendar_announcements SET start_date=?,end_date=?,version=version+1,updated_at=? WHERE id=? AND company_id=?",
        input.startDate,
        input.endDate,
        Date.now(),
        input.id,
        actor.companyId,
      );
      result = { id: input.id, version: old.version + 1, status: old.status, warnings: [] };
    } else if (input.action === "task.reschedule") {
      const task = await one<{ assignee_id: string | null; responsibility: string }>(
        db,
        "SELECT assignee_id,responsibility FROM people_preparation_tasks WHERE id=? AND company_id=?",
        input.id,
        actor.companyId,
      );
      if (!task) fail("Task not found.", "NOT_FOUND", 404);
      result = await taskCommand(
        db,
        a,
        {
          action: "task.plan",
          id: input.id,
          expectedVersion: input.expectedVersion,
          dueDate: input.dueDate,
          assigneeId: task.assignee_id,
          responsibility: task.responsibility,
        },
        recordId,
        key,
      );
    } else if (input.action === "shift.reschedule") {
      result = await rescheduleRosterShift(db, actor, input);
    } else if (input.action === "appointment.save") {
      const old = input.id
        ? await one<AppointmentRow>(
            db,
            "SELECT * FROM calendar_appointments WHERE id=? AND company_id=?",
            input.id,
            actor.companyId,
          )
        : null;
      if (input.id && !old) fail("Appointment not found.", "NOT_FOUND", 404);
      if (old) {
        await manageAppointment(db, a, old);
        expected(old.version, input.expectedVersion);
      }
      const organizer = input.organizerPersonId?await eligiblePerson(db,a,input.organizerPersonId,true):null;
      if (input.organizerPersonId && input.organizerPersonId !== a.staff?.person_id)
        await needPermission(db, actor, "calendar.appointments.manage");
      const ids=[...new Set([...(input.organizerPersonId?[input.organizerPersonId]:[]),...input.participantIds])];
      const chosen=new Map<string,{userId:string;personId:string|null;attendance:"organizer"|"required"|"optional"}>();
      chosen.set(organizer?.user_id??actor.userId,{userId:organizer?.user_id??actor.userId,
        personId:input.organizerPersonId??a.staff?.person_id??null,attendance:"organizer"});
      for(const personId of ids){
        const person=await eligiblePerson(db,a,personId,false);
        chosen.set(person.user_id,{userId:person.user_id,personId,attendance:person.user_id===(organizer?.user_id??actor.userId)?"organizer":"required"});
      }
      for(const attendee of input.attendees??[]){
        const person=await db.prepare("SELECT id FROM hr_people WHERE company_id=? AND user_id=? AND merged_into_id IS NULL LIMIT 1")
          .bind(actor.companyId,attendee.userId).first<{id:string}>();
        chosen.set(attendee.userId,{userId:attendee.userId,personId:person?.id??null,
          attendance:attendee.userId===(organizer?.user_id??actor.userId)?"organizer":attendee.attendance});
      }
      if(chosen.size>100)fail("Choose up to 100 attendees.","TOO_MANY_ATTENDEES",422);
      const userIds=[...chosen.keys()],now=Date.now();
      const active=(await db.prepare(`SELECT DISTINCT user_id FROM assignments WHERE company_id=? AND user_id IN (${userIds.map(()=>"?").join(",")})
        AND revoked_at IS NULL AND iam_assignment_active(id,?) AND iam_principal_active(user_id,company_id,?)`)
        .bind(actor.companyId,...userIds,now,now).all<{user_id:string}>()).results;
      if(active.length!==userIds.length)
        fail("Choose active accounts in this Company.","CALENDAR_ATTENDEE_UNAVAILABLE",422);
      const startLocal=input.allDay?`${input.startDate}T00:00`:input.startLocal!;
      const endLocal=input.allDay?`${input.endDate}T00:00`:input.endLocal!;
      const times=await localInterval(db,startLocal,endLocal,input.timezone);
      const warnings = await appointmentWarningMessages(
        db,
        actor.companyId,
        [...chosen.values()].flatMap(value=>value.personId?[value.personId]:[]),
        times.starts_at,
        times.ends_at,
        input.timezone,
        recordId,
        userIds,
      );
      result = {
        id: recordId,
        version: (old?.version ?? 0) + 1,
        status: "scheduled",
        warnings,
      };
      await run(
        db,
        `INSERT INTO calendar_appointments(id,company_id,branch_id,department_id,organizer_person_id,title,description,starts_at,ends_at,timezone,location,meeting_url,private,guests,status,version,created_by,created_at,updated_at,all_day,start_date,end_date) VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?,?::jsonb,'scheduled',?,?,?,?,?,?,?) ON CONFLICT(id) DO UPDATE SET branch_id=excluded.branch_id,department_id=excluded.department_id,organizer_person_id=excluded.organizer_person_id,title=excluded.title,description=excluded.description,starts_at=excluded.starts_at,ends_at=excluded.ends_at,timezone=excluded.timezone,location=excluded.location,meeting_url=excluded.meeting_url,private=excluded.private,guests=excluded.guests,version=excluded.version,updated_at=excluded.updated_at,all_day=excluded.all_day,start_date=excluded.start_date,end_date=excluded.end_date`,
        recordId,
        actor.companyId,
        organizer?.branch_id??actor.branchId,
        organizer?.department_id??actor.departmentId,
        input.organizerPersonId??null,
        input.title,
        input.description,
        times.starts_at,
        times.ends_at,
        input.timezone,
        input.location,
        input.meetingUrl,
        input.private,
        JSON.stringify(input.guests),
        result.version,
        old?.created_by ?? actor.userId,
        old?.created_at ?? now,
        now,
        input.allDay,
        input.allDay?input.startDate??null:null,
        input.allDay?input.endDate??null:null,
      );
      const oldAttendees=(await db.prepare("SELECT user_id,response,reminder_minutes,response_at FROM calendar_attendees WHERE appointment_id=? AND company_id=?")
        .bind(recordId,actor.companyId).all<{user_id:string;response:string;reminder_minutes:number|null;response_at:number|null}>()).results;
      const oldByUser=new Map(oldAttendees.map(row=>[row.user_id,row]));
      await run(db,"DELETE FROM calendar_attendees WHERE appointment_id=? AND company_id=?",recordId,actor.companyId);
      for(const entry of chosen.values()){
        const previous=oldByUser.get(entry.userId);
        await run(db,`INSERT INTO calendar_attendees(company_id,appointment_id,user_id,person_id,attendance,response,reminder_minutes,response_at)
          VALUES(?,?,?,?,?,?,?,?)`,actor.companyId,recordId,entry.userId,entry.personId,entry.attendance,
          entry.attendance==="organizer"?"accepted":previous?.response??"pending",previous?.reminder_minutes===undefined?15:previous.reminder_minutes,
          previous?.response_at??null);
      }
      await run(
        db,
        "DELETE FROM calendar_appointment_participants WHERE appointment_id=? AND company_id=?",
        recordId,
        actor.companyId,
      );
      for (const person of [...new Set([...chosen.values()].flatMap(value=>value.personId?[value.personId]:[]))])
        await run(
          db,
          "INSERT INTO calendar_appointment_participants(company_id,appointment_id,person_id) VALUES(?,?,?)",
          actor.companyId,
          recordId,
          person,
        );
      if(input.recurrence===null){
        await run(db,"DELETE FROM calendar_series WHERE appointment_id=? AND company_id=?",recordId,actor.companyId);
      }else if(input.recurrence){
        const rule=input.recurrence;
        if(rule.endKind==="until"&&rule.untilDate!<startLocal.slice(0,10))
          fail("Recurrence must end on or after the first event.","CALENDAR_RECURRENCE_END",422);
        await run(db,`INSERT INTO calendar_series(appointment_id,company_id,frequency,interval_count,weekdays,end_kind,occurrence_count,until_date)
          VALUES(?,?,?,?,?,?,?,?) ON CONFLICT(appointment_id) DO UPDATE SET frequency=excluded.frequency,interval_count=excluded.interval_count,
          weekdays=excluded.weekdays,end_kind=excluded.end_kind,occurrence_count=excluded.occurrence_count,until_date=excluded.until_date,version=calendar_series.version+1`,
          recordId,actor.companyId,rule.frequency,rule.intervalCount,rule.weekdays.join(","),rule.endKind,
          rule.endKind==="count"?rule.occurrenceCount??null:null,rule.endKind==="until"?rule.untilDate??null:null);
      }
      const added=userIds.filter(id=>!oldByUser.has(id)&&id!==actor.userId);
      const retained=userIds.filter(id=>oldByUser.has(id)&&id!==actor.userId);
      const removed=oldAttendees.map(row=>row.user_id).filter(id=>!chosen.has(id)&&id!==actor.userId);
      await calendarNotice(db,actor,recordId,result.version,"invited",`Meeting invitation: ${input.title}`,added);
      await calendarNotice(db,actor,recordId,result.version,"updated",`Meeting updated: ${input.title}`,retained);
      await calendarNotice(db,actor,recordId,result.version,"removed","A meeting invitation was withdrawn.",removed,"calendar_removed");
    } else if (input.action === "appointment.respond") {
      const r=await one<AppointmentRow>(db,"SELECT * FROM calendar_appointments WHERE id=? AND company_id=?",input.id,actor.companyId);
      if(!r||r.status!=="scheduled")fail("Meeting unavailable.","NOT_FOUND",404);
      const attendee=await one<{attendance:string}>(db,"SELECT attendance FROM calendar_attendees WHERE company_id=? AND appointment_id=? AND user_id=?",
        actor.companyId,input.id,actor.userId);
      if(!attendee||attendee.attendance==="organizer")fail("This invitation is unavailable.","INVITATION_UNAVAILABLE",403);
      const series=await seriesFor(db,input.id);
      if(input.occurrenceId&&input.scope==="single"){
        if(!series)fail("This meeting does not repeat.","OCCURRENCE_NOT_FOUND",404);
        await assertOccurrence(r,series,input.occurrenceId);
        await run(db,`INSERT INTO calendar_occurrence_responses(company_id,appointment_id,occurrence_key,user_id,response,responded_at)
          VALUES(?,?,?,?,?,?) ON CONFLICT(appointment_id,occurrence_key,user_id) DO UPDATE SET response=excluded.response,responded_at=excluded.responded_at`,
          actor.companyId,input.id,input.occurrenceId,actor.userId,input.response,Date.now());
      }else await run(db,"UPDATE calendar_attendees SET response=?,response_at=? WHERE company_id=? AND appointment_id=? AND user_id=?",
        input.response,Date.now(),actor.companyId,input.id,actor.userId);
      await run(db,"UPDATE calendar_appointments SET version=version+1,updated_at=? WHERE id=? AND company_id=?",Date.now(),input.id,actor.companyId);
      result={id:input.id,version:r.version+1,status:r.status,warnings:[]};
      const organizer=await one<{user_id:string}>(db,"SELECT user_id FROM calendar_attendees WHERE appointment_id=? AND company_id=? AND attendance='organizer'",
        input.id,actor.companyId);
      await calendarNotice(db,actor,input.id,result.version,`response:${actor.userId}:${key}`,`Meeting response: ${input.response}`,
        organizer?[organizer.user_id]:[]);
    } else if (input.action === "appointment.reminder.set") {
      const r=await one<AppointmentRow>(db,"SELECT * FROM calendar_appointments WHERE id=? AND company_id=?",input.id,actor.companyId);
      if(!r||r.status!=="scheduled")fail("Meeting unavailable.","NOT_FOUND",404);
      const changed=await db.prepare("UPDATE calendar_attendees SET reminder_minutes=? WHERE company_id=? AND appointment_id=? AND user_id=? RETURNING user_id")
        .bind(input.minutes,actor.companyId,input.id,actor.userId).first();
      if(!changed)fail("This invitation is unavailable.","INVITATION_UNAVAILABLE",403);
      await run(db,"DELETE FROM calendar_reminder_deliveries WHERE company_id=? AND appointment_id=? AND user_id=? AND delivered_at IS NULL",
        actor.companyId,input.id,actor.userId);
      await run(db,"UPDATE calendar_appointments SET version=version+1,updated_at=? WHERE id=? AND company_id=?",Date.now(),input.id,actor.companyId);
      result={id:input.id,version:r.version+1,status:r.status,warnings:[]};
    } else if (input.action === "appointment.status") {
      const r = await one<AppointmentRow>(
        db,
        "SELECT * FROM calendar_appointments WHERE id=? AND company_id=?",
        input.id,
        actor.companyId,
      );
      if (!r) fail("Appointment not found.", "NOT_FOUND", 404);
      await manageAppointment(db, a, r);
      expected(r.version, input.expectedVersion);
      const series=await seriesFor(db,input.id);
      if(input.occurrenceId&&series)await assertOccurrence(r,series,input.occurrenceId);
      if (input.status !== "cancelled" && r.starts_at > Date.now())
        fail(
          "Record completion or a no-show after the appointment starts.",
          "INVALID_TRANSITION",
          409,
        );
      if(input.status==="cancelled"&&series&&input.occurrenceId&&input.scope==="single"){
        await run(db,`INSERT INTO calendar_occurrence_exceptions(company_id,appointment_id,occurrence_key,cancelled)
          VALUES(?,?,?,true) ON CONFLICT(appointment_id,occurrence_key) DO UPDATE SET cancelled=true,version=calendar_occurrence_exceptions.version+1`,
          actor.companyId,input.id,input.occurrenceId);
        await run(db,"UPDATE calendar_appointments SET version=version+1,updated_at=? WHERE id=? AND company_id=?",Date.now(),input.id,actor.companyId);
      }else if(input.status==="cancelled"&&series&&input.occurrenceId&&input.scope==="future"){
        await run(db,"UPDATE calendar_series SET end_kind='until',until_date=?,occurrence_count=NULL,version=version+1 WHERE appointment_id=?",
          addDays(input.occurrenceId.slice(0,10),-1),input.id);
        await run(db,"UPDATE calendar_appointments SET version=version+1,updated_at=? WHERE id=? AND company_id=?",Date.now(),input.id,actor.companyId);
      }else await run(db,"UPDATE calendar_appointments SET status=?,version=version+1,updated_at=? WHERE id=? AND company_id=?",
        input.status,Date.now(),input.id,actor.companyId);
      result = {
        id: input.id,
        version: r.version + 1,
        status: input.scope==="single"||input.scope==="future"?r.status:input.status,
        warnings: [],
      };
      await calendarNotice(db,actor,input.id,result.version,input.status,`Meeting ${input.status}: ${r.title}`,
        (await appointmentUsers(db,actor.companyId,input.id)).filter(user=>user!==actor.userId));
    } else if (input.action === "announcement.save") {
      await needPermission(db, actor, "calendar.announcements.publish");
      const old = input.id
        ? await one<AnnouncementRow>(
            db,
            "SELECT * FROM calendar_announcements WHERE id=? AND company_id=?",
            input.id,
            actor.companyId,
          )
        : null;
      if (input.id && !old) fail("Announcement not found.", "NOT_FOUND", 404);
      if (old) {
        requirePlacement(a, old.branch_id, old.department_id);
        expected(old.version, input.expectedVersion);
      }
      const audience = a.nodes.find(
        (n) =>
          n.id === input.audienceId &&
          ["company", "branch", "department"].includes(n.kind),
      );
      if (!audience || !a.scopeIds.includes(audience.id))
        fail("Choose an audience inside the selected workspace.");
      const branch =
          audience.kind === "branch" ? audience.id : audience.branchId,
        department = audience.kind === "department" ? audience.id : null;
      result = {
        id: recordId,
        version: (old?.version ?? 0) + 1,
        status: old?.status ?? "draft",
        warnings: [],
      };
      await run(
        db,
        `INSERT INTO calendar_announcements(id,company_id,branch_id,department_id,title,content,start_date,end_date,status,version,created_by,created_at,updated_at) VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?) ON CONFLICT(id) DO UPDATE SET branch_id=excluded.branch_id,department_id=excluded.department_id,title=excluded.title,content=excluded.content,start_date=excluded.start_date,end_date=excluded.end_date,version=excluded.version,updated_at=excluded.updated_at`,
        recordId,
        actor.companyId,
        branch,
        department,
        input.title,
        input.content,
        input.startDate,
        input.endDate,
        result.status,
        result.version,
        old?.created_by ?? actor.userId,
        old?.created_at ?? Date.now(),
        Date.now(),
      );
    } else if (input.action === "announcement.status") {
      await needPermission(db, actor, "calendar.announcements.publish");
      const r = await one<AnnouncementRow>(
        db,
        "SELECT * FROM calendar_announcements WHERE id=? AND company_id=?",
        input.id,
        actor.companyId,
      );
      if (!r) fail("Announcement not found.", "NOT_FOUND", 404);
      requirePlacement(a, r.branch_id, r.department_id);
      expected(r.version, input.expectedVersion);
      if (
        r.status === input.status ||
        (input.status === "withdrawn" && r.status !== "published")
      )
        fail(
          "This announcement cannot make that transition.",
          "INVALID_TRANSITION",
          409,
        );
      await run(
        db,
        "UPDATE calendar_announcements SET status=?,version=version+1,updated_at=? WHERE id=? AND company_id=?",
        input.status,
        Date.now(),
        input.id,
        actor.companyId,
      );
      result = {
        id: input.id,
        version: r.version + 1,
        status: input.status,
        warnings: [],
      };
    } else result = await taskCommand(db, a, input, recordId, key);
    await run(
      db,
      "INSERT INTO calendar_history(id,company_id,source,record_id,action,actor_id,version,created_at) VALUES(?,?,?,?,?,?,?,?)",
      crypto.randomUUID(),
      actor.companyId,
      input.action.split(".")[0],
      recordId,
      input.action + ("status" in input ? `.${input.status}` : ""),
      actor.userId,
      result.version,
      Date.now(),
    );
    // Transactional command receipt, authority guards, audit and outbox are shared with the other workflows.
    return executeCommand(
      db,
      actor,
      key,
      operation,
      input,
      recordId,
      result.version,
      result,
      [],
    );
  });
}
