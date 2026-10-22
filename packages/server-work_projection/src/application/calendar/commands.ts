type CalendarReadResult<Row>={results:Row[]};
import type { PreparationTask } from "@erpx/feature-people_payroll/lib/people-lifecycle";
import { calendarCommandSchema, type CalendarCommand } from "@erpx/server-work_projection/public/rules/calendar-domain";
import { hasPermission } from "@erpx/feature-identity_access/lib/permissions";
import type { WorkContext } from "@erpx/server-identity_access/public/authorization";
import { addDays } from "@erpx/server-people_payroll/public/rules/people-roster-domain";
import { expandCalendarSeries, type RecurrenceRule } from "@erpx/feature-work_projection/lib/calendar-recurrence";
import { localDateTimeToInstant, localMinuteAt } from "@erpx/feature-work_projection/lib/calendar-timeline";
import type {CalendarAccess} from "@erpx/server-work_projection/domain/calendar/access";
import type {Result,SeriesRow} from "@erpx/server-work_projection/domain/calendar/commands";
import type {Clock,IdentifierSource} from "@erpx/server-shared/application/ports";
import type {AppointmentRow} from "@erpx/server-work_projection/domain/calendar/appointments";
import type {AnnouncementRow} from "@erpx/server-work_projection/domain/calendar/announcements";
export type CalendarCommandsUnit<Effect>={read1(facts:{value1:string;value2:string;value3:string;}):Promise<{ starts_at: number; ends_at: number; start_local: string; end_local: string; } | null>;read2(facts:{value1:string;value2:number;value3:string;}):Promise<{ person_id: string; user_id: string; department_id: string | null; branch_id: string | null; }[]>;read3(facts:{value1:string;}):Promise<SeriesRow | null>;read4(facts:{value1:string;value2:string;}):Promise<CalendarReadResult<{ user_id: string; }>>;read5(facts:{value1:string;value2:string;}):Promise<(PreparationTask & { id: string; kind: string; status: string; version: number; privileged: boolean; assignee_id: string | null; branch_id: string | null; department_id: string | null; }) | null>;read6(facts:{value1:string;value2:string;value3:number;}):Promise<unknown>;read7(facts:{value1:string;value2:string;}):Promise<{ id: string; } | null>;write8(facts:{value1:string | null;value2:string | null;value3:string;value4:number;value5:string;value6:string;}):Promise<unknown>;write9(facts:{value1:string;value2:string;value3:string;value4:string;value5:number;value6:string;value7:number;}):Promise<unknown>;read10(facts:{value1:string | null;value2:string;value3:string;}):Promise<unknown>;write11(facts:{value1:"completed" | "not_required";value2:string;value3:string;value4:string;value5:number;value6:string;value7:string;}):Promise<unknown>;read12(facts:{value1:string;value2:string;}):Promise<AppointmentRow | null>;read13(facts:{value1:string;value2:string;}):Promise<{ person_id: string; }[]>;write14(facts:{value1:string;value2:string;value3:string;value4:string;value5:string;}):Promise<unknown>;write15(facts:{value1:number;value2:string;value3:string;}):Promise<unknown>;write16(facts:{value1:string;value2:string;}):Promise<unknown>;write18(facts:{value1:string;value2:number;value3:number;value4:string;value5:number;value6:boolean;value7:string | null;value8:string | null;value9:string;value10:string;}):Promise<unknown>;write19(facts:{value1:string;value2:string;value3:"daily" | "weekly" | "monthly" | "yearly";value4:number;value5:string;value6:"never" | "count" | "until";value7:number | null;value8:string | null;}):Promise<unknown>;write20(facts:{value1:string;value2:string;value3:string;}):Promise<unknown>;write21(facts:{value1:string;value2:string;value3:string;}):Promise<unknown>;write22(facts:{value1:number;value2:number;value3:string;value4:string | null;value5:string | null;value6:number;value7:string;value8:string;}):Promise<unknown>;read23(facts:{value1:string;value2:string;}):Promise<AnnouncementRow | null>;write24(facts:{value1:string;value2:string;value3:number;value4:string;value5:string;}):Promise<unknown>;read25(facts:{value1:string;value2:string;}):Promise<{ assignee_id: string | null; responsibility: string; } | null>;read26(facts:{value1:string;value2:string;}):Promise<AppointmentRow | null>;read27(facts:{value1:string;value2:string;}):Promise<{ id: string; } | null>;read28(facts:{value1:string;value2:string[];value3:number;}):Promise<CalendarReadResult<{ user_id: string; }>>;write29(facts:{value1:string;value2:string;value3:string | null;value4:string | null;value5:string | null;value6:string;value7:string;value8:number;value9:number;value10:string;value11:string;value12:string;value13:boolean;value14:string;value15:number;value16:string;value17:number;value18:number;value19:boolean;value20:string | null;value21:string | null;}):Promise<unknown>;read30(facts:{value1:string;value2:string;}):Promise<CalendarReadResult<{ user_id: string; response: string; reminder_minutes: number | null; response_at: number | null; }>>;write31(facts:{value1:string;value2:string;}):Promise<unknown>;write32(facts:{value1:string;value2:string;value3:string;value4:string | null;value5:"optional" | "organizer" | "required";value6:string;value7:number | null;value8:number | null;}):Promise<unknown>;write33(facts:{value1:string;value2:string;}):Promise<unknown>;write34(facts:{value1:string;value2:string;value3:string;}):Promise<unknown>;write35(facts:{value1:string;value2:string;}):Promise<unknown>;write36(facts:{value1:string;value2:string;value3:"daily" | "weekly" | "monthly" | "yearly";value4:number;value5:string;value6:"never" | "count" | "until";value7:number | null;value8:string | null;}):Promise<unknown>;read37(facts:{value1:string;value2:string;}):Promise<AppointmentRow | null>;read38(facts:{value1:string;value2:string;value3:string;}):Promise<{ attendance: string; } | null>;write39(facts:{value1:string;value2:string;value3:string;value4:string;value5:"accepted" | "tentative" | "declined";value6:number;}):Promise<unknown>;write40(facts:{value1:"accepted" | "tentative" | "declined";value2:number;value3:string;value4:string;value5:string;}):Promise<unknown>;read42(facts:{value1:string;value2:string;}):Promise<{ user_id: string; } | null>;read44(facts:{value1:number | null;value2:string;value3:string;value4:string;}):Promise<Record<string, unknown> | null>;write45(facts:{value1:string;value2:string;value3:string;}):Promise<unknown>;write48(facts:{value1:string;value2:string;value3:string;}):Promise<unknown>;write50(facts:{value1:string;value2:string;}):Promise<unknown>;write52(facts:{value1:"cancelled" | "completed" | "no_show";value2:number;value3:string;value4:string;}):Promise<unknown>;read53(facts:{value1:string;value2:string;}):Promise<AnnouncementRow | null>;write54(facts:{value1:string;value2:string;value3:string | null;value4:string | null;value5:string;value6:string;value7:string;value8:string;value9:string;value10:number;value11:string;value12:number;value13:number;}):Promise<unknown>;write56(facts:{value1:"published" | "withdrawn";value2:number;value3:string;value4:string;}):Promise<unknown>;write57(facts:{value1:string;value2:string;value3:string;value4:string;value5:string;value6:string;value7:number;value8:number;}):Promise<unknown>;reschedule(actor:WorkContext,input:Extract<CalendarCommand,{action:"shift.reschedule"}>):Promise<Result>;access(actor:WorkContext):Promise<CalendarAccess>;needPermission(actor:WorkContext,permission:string):Promise<void>;warnings(companyId:string,ids:string[],start:number,end:number,zone:string,except:string,userIds?:string[]):Promise<string[]>;replay(actor:WorkContext,key:string,operation:string,recordId:string,input:CalendarCommand):Promise<Result|null>;execute(actor:WorkContext,key:string,operation:string,input:CalendarCommand,recordId:string,version:number,result:Result,effects:Effect[]):Promise<Result>;batch(effects:Effect[]):Promise<unknown>;bindAttachments(actor:WorkContext,input:{attachmentSetIds:string[];bindingContract:import("@erpx/server-files/public/attachments/types").AttachmentBindingContract;targetId:string;expectedTargetVersion:number;commandId:string;bindingId:string}):Promise<{guards:Effect[];steps:Effect[]}>;clearancePlan(actor:WorkContext,task:PreparationTask,assigneeId:string|null):Promise<void>;clearanceCompletion(actor:WorkContext,task:PreparationTask,input:Extract<CalendarCommand,{action:"task.complete"}>):Promise<void>;publish(input:import("@erpx/server-collaboration/public/inbox").InboxEventInput):Promise<string>;transaction<T>(callback:(tx:CalendarCommandsUnit<Effect>)=>Promise<T>):Promise<T>;};
export function createCalendarCommands<Effect>(ports:{clock:Clock;identifiers:IdentifierSource;fail(message:string,code?:string,status?:number):never;requirePlacement(access:CalendarAccess,branch:string|null,department:string|null):void}){




















async function localInterval(
  db: CalendarCommandsUnit<Effect>,
  start: string,
  end: string,
  zone: string,
) {
  const found = await db.read1({value1:start,value2:zone,value3:end});
  if (
    !found ||
    found.start_local !== start ||
    found.end_local !== end ||
    found.ends_at <= found.starts_at ||
    found.ends_at - found.starts_at > 31 * 86400000
  )
    ports.fail(
      "Choose valid local times, with the end after the start (up to 31 days).",
      "CALENDAR_TIME",
      422,
    );
  return found;
}
function expected(actual: number, supplied: number | undefined) {
  if (actual !== supplied)
    ports.fail(
      "This record changed. Refresh before trying again.",
      "VERSION_CONFLICT",
      409,
    );
}
async function eligiblePerson(
  db: CalendarCommandsUnit<Effect>,
  a: CalendarAccess,
  id: string,
  scopeRequired: boolean,
) {
  const found = await db.read2({value1:a.actor.companyId,value2:ports.clock.now(),value3:id});
  if (found.length !== 1)
    ports.fail(
      "Choose a colleague with one active engagement and account.",
      "CALENDAR_STAFF",
      422,
    );
  if (scopeRequired)
    ports.requirePlacement(a, found[0].branch_id, found[0].department_id);
  return found[0];
}
async function manageAppointment(
  db: CalendarCommandsUnit<Effect>,
  a: CalendarAccess,
  r: AppointmentRow,
) {
  ports.requirePlacement(a, r.branch_id, r.department_id);
  if (r.created_by!==a.actor.userId && r.organizer_person_id !== a.staff?.person_id)
    await db.needPermission( a.actor, "calendar.appointments.manage");
  if (r.status !== "scheduled")
    ports.fail(
      "Only scheduled appointments can be changed.",
      "INVALID_TRANSITION",
      409,
    );
}

async function seriesFor(db:CalendarCommandsUnit<Effect>,id:string){return db.read3({value1:id});}
async function appointmentUsers(db:CalendarCommandsUnit<Effect>,companyId:string,id:string){
  return (await db.read4({value1:id,value2:companyId})).results.map(row=>row.user_id);
}
async function calendarNotice(db:CalendarCommandsUnit<Effect>,actor:WorkContext,id:string,version:number,kind:string,preview:string,users:string[],sourceType="calendar_appointment"){
  if(!users.length)return;
  await db.publish({key:`calendar:${id}:${version}:${kind}`,companyId:actor.companyId,
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
  if(occurrences[0]?.key!==key)ports.fail("This occurrence is unavailable.","OCCURRENCE_NOT_FOUND",404);
}
async function taskCommand(
  db: CalendarCommandsUnit<Effect>,
  a: CalendarAccess,
  input: Extract<CalendarCommand, { action: "task.plan" | "task.complete" }>,
  recordId: string,
  commandId: string,
): Promise<Result> {
  const t = await db.read5({value1:input.id,value2:a.actor.companyId});
  if (!t) ports.fail("Task not found.", "NOT_FOUND", 404);
  ports.requirePlacement(a, t.branch_id, t.department_id);
  expected(t.version, input.expectedVersion);
  if (t.status !== "pending")
    ports.fail(
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
      await db.clearancePlan(a.actor,t,input.assigneeId);
      if (!input.dueDate) ports.fail("Clearance tasks need a due date.","CLEARANCE_DUE_DATE",422);
    }
    else if (t.privileged || !hasPermission(a.actor, "organization.manage")) {
      if (!it) ports.fail("This preparation task requires Company IT authority.");
      await db.needPermission( a.actor, "iam.directory.manage");
    } else await db.needPermission( a.actor, "organization.manage");
    if (input.assigneeId && !t.clearance_type) {
      if (t.privileged) {
        if (
          !(await db.read6({value1:a.actor.companyId,value2:input.assigneeId,value3:ports.clock.now()}))
        )
          ports.fail(
            "Access and directory tasks require an active Company IT assignee.",
            "CALENDAR_ASSIGNEE",
            422,
          );
      } else {
        const p = await db.read7({value1:a.actor.companyId,value2:input.assigneeId});
        if (!p)
          ports.fail(
            "Choose an active staff assignee in this Company.",
            "CALENDAR_ASSIGNEE",
            422,
          );
        await eligiblePerson(db, a, p.id, false);
      }
    }
    await db.write8({value1:input.dueDate,value2:input.assigneeId,value3:input.responsibility,value4:ports.clock.now(),value5:input.id,value6:a.actor.companyId});
    if (t.clearance_type && input.assigneeId && input.assigneeId !== t.assignee_id) await db.write9({value1:`${t.id}-assignment-${t.version+1}`,value2:a.actor.companyId,value3:input.assigneeId,value4:t.id,value5:t.version+1,value6:t.title,value7:ports.clock.now()});
  } else {
    if (t.kind === "position")
      ports.fail(
        "Position preparation completes through the access package workflow.",
        "PRIVILEGED_WORKFLOW",
        409,
      );
    if (!t.clearance_type && input.status === "not_required") ports.fail("Not required is available for retirement clearance tasks.","INVALID_TRANSITION",422);
    if (t.clearance_type) await db.clearanceCompletion(a.actor,t,input);
    else if (
      t.privileged ||
      !a.staff ||
      !(await db.read10({value1:t.assignee_id,value2:a.actor.userId,value3:a.actor.companyId}))
    ) {
      if (!it)
        ports.fail(
          "Only the assigned colleague can complete this task; IT work requires Company IT authority.",
        );
      await db.needPermission( a.actor, "iam.directory.manage");
    }
    const attachmentBinding =
      t.clearance_type && input.attachmentSetIds.length
        ? await db.bindAttachments( a.actor, {
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
    await db.write11({value1:input.status,value2:input.reason,value3:JSON.stringify(input.references),value4:a.actor.userId,value5:ports.clock.now(),value6:input.id,value7:a.actor.companyId});
    if (attachmentBinding) await db.batch(attachmentBinding.steps);
  }
  return {
    id: recordId,
    version: t.version + 1,
    status: input.action === "task.complete" ? input.status : "pending",
    warnings: [],
  };
}
async function calendarCommand(
  database: CalendarCommandsUnit<Effect>,
  actor: WorkContext,
  raw: unknown,
  key: string,
): Promise<Result> {
  const input = calendarCommandSchema.parse(raw),
    operation = `calendar.${input.action}`,
    recordId = ("id" in input && input.id) || `calendar-${key}`;
  if (!/^[a-zA-Z0-9:_.-]{8,180}$/.test(key))
    ports.fail("Use a valid Idempotency-Key.", "IDEMPOTENCY_KEY_REQUIRED", 422);
  return database.transaction(async (db) => {
    const replay = await db.replay(
      actor,
      key,
      operation,
      recordId,
      input,
    );
    if (replay) return replay;
    const a = await db.access(actor);
    let result: Result;
    if (input.action === "appointment.reschedule") {
      const old = await db.read12({value1:input.id,value2:actor.companyId});
      if (!old) ports.fail("Appointment not found.", "NOT_FOUND", 404);
      await manageAppointment(db, a, old);
      expected(old.version, input.expectedVersion);
      const times = await localInterval(db, input.startLocal, input.endLocal, input.timezone);
      const series=await seriesFor(db,input.id);
      if(input.occurrenceId&&series)await assertOccurrence(old,series,input.occurrenceId);
      const participants = await db.read13({value1:input.id,value2:actor.companyId});
      const warnings = await db.warnings(
        actor.companyId,
        participants.map((participant) => participant.person_id),
        times.starts_at,
        times.ends_at,
        input.timezone,
        input.id,
      );
      let futureSplitId:string|null=null;
      if(series&&input.occurrenceId&&input.scope==="single"){
        await db.write14({value1:actor.companyId,value2:input.id,value3:input.occurrenceId,value4:input.startLocal,value5:input.endLocal});
        await db.write15({value1:ports.clock.now(),value2:input.id,value3:actor.companyId});
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
        await db.write16({value1:before,value2:input.id});
        await db.write15({value1:ports.clock.now(),value2:input.id,value3:actor.companyId});
        await db.write18({value1:newId,value2:times.starts_at,value3:times.ends_at,value4:input.timezone,value5:ports.clock.now(),value6:old.all_day,value7:old.all_day?input.startLocal.slice(0,10):null,value8:old.all_day?input.endLocal.slice(0,10):null,value9:input.id,value10:actor.companyId});
        await db.write19({value1:newId,value2:actor.companyId,value3:series.frequency,value4:series.interval_count,value5:series.weekdays,value6:series.end_kind,value7:series.end_kind==="count"?remaining:null,value8:series.until_date});
        await db.write20({value1:newId,value2:input.id,value3:actor.companyId});
        await db.write21({value1:newId,value2:input.id,value3:actor.companyId});
      }else{
        await db.write22({value1:times.starts_at,value2:times.ends_at,value3:input.timezone,value4:old.all_day?input.startLocal.slice(0,10):null,value5:old.all_day?input.endLocal.slice(0,10):null,value6:ports.clock.now(),value7:input.id,value8:actor.companyId});
      }
      result = { id: input.id, version: old.version + 1, status: old.status, warnings };
      await calendarNotice(db,actor,input.id,result.version,"moved",`Meeting time changed: ${old.title}`,
        (await appointmentUsers(db,actor.companyId,input.id)).filter(user=>user!==actor.userId));
      if(futureSplitId)await calendarNotice(db,actor,futureSplitId,1,"invited",`Meeting invitation: ${old.title}`,
        (await appointmentUsers(db,actor.companyId,futureSplitId)).filter(user=>user!==actor.userId));
    } else if (input.action === "announcement.reschedule") {
      await db.needPermission( actor, "calendar.announcements.publish");
      const old = await db.read23({value1:input.id,value2:actor.companyId});
      if (!old) ports.fail("Announcement not found.", "NOT_FOUND", 404);
      ports.requirePlacement(a, old.branch_id, old.department_id);
      expected(old.version, input.expectedVersion);
      if (!["draft", "published"].includes(old.status))
        ports.fail("Only active announcements can be rescheduled.", "INVALID_TRANSITION", 409);
      await db.write24({value1:input.startDate,value2:input.endDate,value3:ports.clock.now(),value4:input.id,value5:actor.companyId});
      result = { id: input.id, version: old.version + 1, status: old.status, warnings: [] };
    } else if (input.action === "task.reschedule") {
      const task = await db.read25({value1:input.id,value2:actor.companyId});
      if (!task) ports.fail("Task not found.", "NOT_FOUND", 404);
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
      result = await db.reschedule(actor,input);
    } else if (input.action === "appointment.save") {
      const old = input.id
        ? await db.read26({value1:input.id,value2:actor.companyId})
        : null;
      if (input.id && !old) ports.fail("Appointment not found.", "NOT_FOUND", 404);
      if (old) {
        await manageAppointment(db, a, old);
        expected(old.version, input.expectedVersion);
      }
      const organizer = input.organizerPersonId?await eligiblePerson(db,a,input.organizerPersonId,true):null;
      if (input.organizerPersonId && input.organizerPersonId !== a.staff?.person_id)
        await db.needPermission( actor, "calendar.appointments.manage");
      const ids=[...new Set([...(input.organizerPersonId?[input.organizerPersonId]:[]),...input.participantIds])];
      const chosen=new Map<string,{userId:string;personId:string|null;attendance:"organizer"|"required"|"optional"}>();
      chosen.set(organizer?.user_id??actor.userId,{userId:organizer?.user_id??actor.userId,
        personId:input.organizerPersonId??a.staff?.person_id??null,attendance:"organizer"});
      for(const personId of ids){
        const person=await eligiblePerson(db,a,personId,false);
        chosen.set(person.user_id,{userId:person.user_id,personId,attendance:person.user_id===(organizer?.user_id??actor.userId)?"organizer":"required"});
      }
      for(const attendee of input.attendees??[]){
        const person=await db.read27({value1:actor.companyId,value2:attendee.userId});
        chosen.set(attendee.userId,{userId:attendee.userId,personId:person?.id??null,
          attendance:attendee.userId===(organizer?.user_id??actor.userId)?"organizer":attendee.attendance});
      }
      if(chosen.size>100)ports.fail("Choose up to 100 attendees.","TOO_MANY_ATTENDEES",422);
      const userIds=[...chosen.keys()],now=ports.clock.now();
      const active=(await db.read28({value1:actor.companyId,value2:userIds,value3:now})).results;
      if(active.length!==userIds.length)
        ports.fail("Choose active accounts in this Company.","CALENDAR_ATTENDEE_UNAVAILABLE",422);
      const startLocal=input.allDay?`${input.startDate}T00:00`:input.startLocal!;
      const endLocal=input.allDay?`${input.endDate}T00:00`:input.endLocal!;
      const times=await localInterval(db,startLocal,endLocal,input.timezone);
      const warnings = await db.warnings(
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
      await db.write29({value1:recordId,value2:actor.companyId,value3:organizer?.branch_id??actor.branchId,value4:organizer?.department_id??actor.departmentId,value5:input.organizerPersonId??null,value6:input.title,value7:input.description,value8:times.starts_at,value9:times.ends_at,value10:input.timezone,value11:input.location,value12:input.meetingUrl,value13:input.private,value14:JSON.stringify(input.guests),value15:result.version,value16:old?.created_by ?? actor.userId,value17:old?.created_at ?? now,value18:now,value19:input.allDay,value20:input.allDay?input.startDate??null:null,value21:input.allDay?input.endDate??null:null});
      const oldAttendees=(await db.read30({value1:recordId,value2:actor.companyId})).results;
      const oldByUser=new Map(oldAttendees.map(row=>[row.user_id,row]));
      await db.write31({value1:recordId,value2:actor.companyId});
      for(const entry of chosen.values()){
        const previous=oldByUser.get(entry.userId);
        await db.write32({value1:actor.companyId,value2:recordId,value3:entry.userId,value4:entry.personId,value5:entry.attendance,value6:entry.attendance==="organizer"?"accepted":previous?.response??"pending",value7:previous?.reminder_minutes===undefined?15:previous.reminder_minutes,value8:previous?.response_at??null});
      }
      await db.write33({value1:recordId,value2:actor.companyId});
      for (const person of [...new Set([...chosen.values()].flatMap(value=>value.personId?[value.personId]:[]))])
        await db.write34({value1:actor.companyId,value2:recordId,value3:person});
      if(input.recurrence===null){
        await db.write35({value1:recordId,value2:actor.companyId});
      }else if(input.recurrence){
        const rule=input.recurrence;
        if(rule.endKind==="until"&&rule.untilDate!<startLocal.slice(0,10))
          ports.fail("Recurrence must end on or after the first event.","CALENDAR_RECURRENCE_END",422);
        await db.write36({value1:recordId,value2:actor.companyId,value3:rule.frequency,value4:rule.intervalCount,value5:rule.weekdays.join(","),value6:rule.endKind,value7:rule.endKind==="count"?rule.occurrenceCount??null:null,value8:rule.endKind==="until"?rule.untilDate??null:null});
      }
      const added=userIds.filter(id=>!oldByUser.has(id)&&id!==actor.userId);
      const retained=userIds.filter(id=>oldByUser.has(id)&&id!==actor.userId);
      const removed=oldAttendees.map(row=>row.user_id).filter(id=>!chosen.has(id)&&id!==actor.userId);
      await calendarNotice(db,actor,recordId,result.version,"invited",`Meeting invitation: ${input.title}`,added);
      await calendarNotice(db,actor,recordId,result.version,"updated",`Meeting updated: ${input.title}`,retained);
      await calendarNotice(db,actor,recordId,result.version,"removed","A meeting invitation was withdrawn.",removed,"calendar_removed");
    } else if (input.action === "appointment.respond") {
      const r=await db.read37({value1:input.id,value2:actor.companyId});
      if(!r||r.status!=="scheduled")ports.fail("Meeting unavailable.","NOT_FOUND",404);
      const attendee=await db.read38({value1:actor.companyId,value2:input.id,value3:actor.userId});
      if(!attendee||attendee.attendance==="organizer")ports.fail("This invitation is unavailable.","INVITATION_UNAVAILABLE",403);
      const series=await seriesFor(db,input.id);
      if(input.occurrenceId&&input.scope==="single"){
        if(!series)ports.fail("This meeting does not repeat.","OCCURRENCE_NOT_FOUND",404);
        await assertOccurrence(r,series,input.occurrenceId);
        await db.write39({value1:actor.companyId,value2:input.id,value3:input.occurrenceId,value4:actor.userId,value5:input.response,value6:ports.clock.now()});
      }else await db.write40({value1:input.response,value2:ports.clock.now(),value3:actor.companyId,value4:input.id,value5:actor.userId});
      await db.write15({value1:ports.clock.now(),value2:input.id,value3:actor.companyId});
      result={id:input.id,version:r.version+1,status:r.status,warnings:[]};
      const organizer=await db.read42({value1:input.id,value2:actor.companyId});
      await calendarNotice(db,actor,input.id,result.version,`response:${actor.userId}:${key}`,`Meeting response: ${input.response}`,
        organizer?[organizer.user_id]:[]);
    } else if (input.action === "appointment.reminder.set") {
      const r=await db.read37({value1:input.id,value2:actor.companyId});
      if(!r||r.status!=="scheduled")ports.fail("Meeting unavailable.","NOT_FOUND",404);
      const changed=await db.read44({value1:input.minutes,value2:actor.companyId,value3:input.id,value4:actor.userId});
      if(!changed)ports.fail("This invitation is unavailable.","INVITATION_UNAVAILABLE",403);
      await db.write45({value1:actor.companyId,value2:input.id,value3:actor.userId});
      await db.write15({value1:ports.clock.now(),value2:input.id,value3:actor.companyId});
      result={id:input.id,version:r.version+1,status:r.status,warnings:[]};
    } else if (input.action === "appointment.status") {
      const r = await db.read12({value1:input.id,value2:actor.companyId});
      if (!r) ports.fail("Appointment not found.", "NOT_FOUND", 404);
      await manageAppointment(db, a, r);
      expected(r.version, input.expectedVersion);
      const series=await seriesFor(db,input.id);
      if(input.occurrenceId&&series)await assertOccurrence(r,series,input.occurrenceId);
      if (input.status !== "cancelled" && r.starts_at > ports.clock.now())
        ports.fail(
          "Record completion or a no-show after the appointment starts.",
          "INVALID_TRANSITION",
          409,
        );
      if(input.status==="cancelled"&&series&&input.occurrenceId&&input.scope==="single"){
        await db.write48({value1:actor.companyId,value2:input.id,value3:input.occurrenceId});
        await db.write15({value1:ports.clock.now(),value2:input.id,value3:actor.companyId});
      }else if(input.status==="cancelled"&&series&&input.occurrenceId&&input.scope==="future"){
        await db.write50({value1:addDays(input.occurrenceId.slice(0,10),-1),value2:input.id});
        await db.write15({value1:ports.clock.now(),value2:input.id,value3:actor.companyId});
      }else await db.write52({value1:input.status,value2:ports.clock.now(),value3:input.id,value4:actor.companyId});
      result = {
        id: input.id,
        version: r.version + 1,
        status: input.scope==="single"||input.scope==="future"?r.status:input.status,
        warnings: [],
      };
      await calendarNotice(db,actor,input.id,result.version,input.status,`Meeting ${input.status}: ${r.title}`,
        (await appointmentUsers(db,actor.companyId,input.id)).filter(user=>user!==actor.userId));
    } else if (input.action === "announcement.save") {
      await db.needPermission( actor, "calendar.announcements.publish");
      const old = input.id
        ? await db.read53({value1:input.id,value2:actor.companyId})
        : null;
      if (input.id && !old) ports.fail("Announcement not found.", "NOT_FOUND", 404);
      if (old) {
        ports.requirePlacement(a, old.branch_id, old.department_id);
        expected(old.version, input.expectedVersion);
      }
      const audience = a.nodes.find(
        (n) =>
          n.id === input.audienceId &&
          ["company", "branch", "department"].includes(n.kind),
      );
      if (!audience || !a.scopeIds.includes(audience.id))
        ports.fail("Choose an audience inside the selected workspace.");
      const branch =
          audience.kind === "branch" ? audience.id : audience.branchId,
        department = audience.kind === "department" ? audience.id : null;
      result = {
        id: recordId,
        version: (old?.version ?? 0) + 1,
        status: old?.status ?? "draft",
        warnings: [],
      };
      await db.write54({value1:recordId,value2:actor.companyId,value3:branch,value4:department,value5:input.title,value6:input.content,value7:input.startDate,value8:input.endDate,value9:result.status,value10:result.version,value11:old?.created_by ?? actor.userId,value12:old?.created_at ?? ports.clock.now(),value13:ports.clock.now()});
    } else if (input.action === "announcement.status") {
      await db.needPermission( actor, "calendar.announcements.publish");
      const r = await db.read23({value1:input.id,value2:actor.companyId});
      if (!r) ports.fail("Announcement not found.", "NOT_FOUND", 404);
      ports.requirePlacement(a, r.branch_id, r.department_id);
      expected(r.version, input.expectedVersion);
      if (
        r.status === input.status ||
        (input.status === "withdrawn" && r.status !== "published")
      )
        ports.fail(
          "This announcement cannot make that transition.",
          "INVALID_TRANSITION",
          409,
        );
      await db.write56({value1:input.status,value2:ports.clock.now(),value3:input.id,value4:actor.companyId});
      result = {
        id: input.id,
        version: r.version + 1,
        status: input.status,
        warnings: [],
      };
    } else result = await taskCommand(db, a, input, recordId, key);
    await db.write57({value1:ports.identifiers.next(),value2:actor.companyId,value3:input.action.split(".")[0],value4:recordId,value5:input.action + ("status" in input ? `.${input.status}` : ""),value6:actor.userId,value7:result.version,value8:ports.clock.now()});
    // Transactional command receipt, authority guards, audit and outbox are shared with the other workflows.
    return db.execute(
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

return {localInterval,calendarCommand};}
