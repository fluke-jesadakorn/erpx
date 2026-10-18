"use client";
import { FormDisclosure } from "@/components/ui/form-disclosure";
import { Button, Field, Select, TextArea, TextInput } from "@/components/ui/foundation";

import { ArrowLeft, CalendarPlus } from "lucide-react";
import { useCallback, useEffect, useLayoutEffect, useRef, useState, type FormEvent } from "react";
import { StandardCalendarForm as CalendarForm } from "@/components/work_projection/calendar/standard-calendar-forms";
import {Dialog} from "@/components/ui/dialog";
import {CalendarDraftBoundary} from "@/components/work_projection/calendar/calendar-draft-boundary";
import {createCalendarCommands,calendarCommandContext,type CalendarCommandAttempt} from "@/lib/work_projection/calendar-command";
import type {WorkCommandCompletion} from "@/lib/work_projection/work-fetch";
import {captureWorkCommandContext} from "@/lib/work_projection/command-context";
import { eventTime } from "@/components/work_projection/calendar/calendar-event";
import { itemOnDate, type CalendarData, type CalendarHierarchy } from "@/lib/work_projection/calendar";
import { addDays, rosterMonday, type RosterData } from "@/lib/people_payroll/people-roster";
import type {
  peopleMoveWorkImpact,
  peoplePositionWorkingTimeData,
  peopleWorkingTimeData,
} from "@/server/modules/people_payroll/public/people-hr-settings";
import { RosterOwnerWorkspace as RosterWorkspace } from "@/components/people_payroll/roster-owner-workspace";
import { usePeopleIntegrationCommand, usePeopleIntegrationData } from "@/components/people_payroll/people-integration-data";
import styles from "@/components/people_payroll/people-hr-settings.module.css";
import rosterStyles from "@/components/people_payroll/roster.module.css";
import {currentRosterOwnerDraft,rosterOwnerDraftHeld,type RosterOwnerDraftState} from "@/lib/people_payroll/roster-owner-command";

function form(event: FormEvent<HTMLFormElement>) {
  event.preventDefault();
  return new FormData(event.currentTarget);
}
const value = (fields: FormData, key: string) => String(fields.get(key) ?? "");
const weekdays = ["Monday", "Tuesday", "Wednesday", "Thursday", "Friday", "Saturday", "Sunday"];
function selectedMonday(date?: string) {
  if (
    !date ||
    !/^\d{4}-\d{2}-\d{2}$/.test(date) ||
    !Number.isFinite(Date.parse(`${date}T12:00:00Z`)) ||
    new Date(`${date}T12:00:00Z`).toISOString().slice(0, 10) !== date
  )
    return rosterMonday();
  return rosterMonday(date);
}

export function PeopleTeamSchedule({
  base,
  departmentId,
  initialWeek,
  initialView,
  requestId,
  onBack,
}: {
  base: string;
  departmentId?: string;
  initialWeek?: string;
  initialView?: string;
  requestId?: string;
  onBack: () => void;
}) {
  const query = new URLSearchParams({
    week: selectedMonday(initialWeek),
    ...(departmentId ? { department: departmentId } : {}),
  });
  const roster = usePeopleIntegrationData<RosterData>(`/api/v1${base}/people/roster?${query}`);
  const contextKey = `${base}:${query}`;
  const [latest, setLatest] = useState<{ key: string; data: RosterData } | null>(null),
    [appointment, setAppointment] = useState(false);
  const [ownedDraft,setOwnedDraft]=useState<{state:RosterOwnerDraftState;key:string|null;data:RosterData|null}|null>(null),[ownerWarning,setOwnerWarning]=useState("");
  const freshData = latest?.key === contextKey ? latest.data : roster.data;
  const data = freshData??(rosterOwnerDraftHeld(ownedDraft?.state)?ownedDraft?.data:null);
  const defaultView = data?.selfRosterRequired ? "mine" : "team";
  const permittedView =
    initialView === "mine"
      ? data?.selfRosterRequired
      : initialView === "templates"
        ? data?.canManage
        : initialView === "team" || initialView === "requests";
  const view = permittedView ? initialView! : defaultView;
  const rosterHeld=rosterOwnerDraftHeld(ownedDraft?.state),rosterDraftLive=useRef<RosterOwnerDraftState|null>(null);
  const trackRosterDraft=useCallback((next:RosterOwnerDraftState)=>{rosterDraftLive.current=currentRosterOwnerDraft(rosterDraftLive.current,next);setOwnedDraft(previous=>{
    const current=currentRosterOwnerDraft(previous?.state,next);
    if(current===previous?.state)return previous;
    return {state:current,key:rosterOwnerDraftHeld(current)?previous?.key??`${contextKey}:${view}`:null,data:rosterOwnerDraftHeld(current)?data??previous?.data??null:null};
  });},[contextKey,view,data]);

  const selected = data
    ? {
        department: data.departmentId,
        week: data.weekStart,
        timezone: data.week?.timezone ?? "Asia/Bangkok",
      }
    : {
        department: departmentId ?? null,
        week: selectedMonday(initialWeek),
        timezone: "Asia/Bangkok",
      };
  const calendarQuery = new URLSearchParams({
    date: selected.week,
    view: "week",
    mode: view === "mine" ? "mine" : "team",
    layers: "leave,appointments",
    timezone: selected.timezone,
    limit: "200",
    ...(selected.department ? { node: selected.department } : {}),
  });
  const calendar = usePeopleIntegrationData<CalendarData>(
    `/api/v1${base}/calendar?${calendarQuery}`,
    "calendar",
  );
  const hierarchy = usePeopleIntegrationData<CalendarHierarchy>(
    `/api/v1${base}/calendar/hierarchy?limit=200`,
    "hierarchy",
  );
  const [calendarBusy,setCalendarBusy]=useState(false),[calendarPending,setCalendarPending]=useState<CalendarCommandAttempt|null>(null),[calendarDirty,setCalendarDirty]=useState(false),[calendarError,setCalendarError]=useState(''),[calendarMessage,setCalendarMessage]=useState(''),[calendarDiscard,setCalendarDiscard]=useState(false);
  const [appointmentSnapshot,setAppointmentSnapshot]=useState<{hierarchy:CalendarHierarchy;date:string;timezone:string;participantIds:string[]}|null>(null);
  const calendarCommands=useRef(createCalendarCommands()),calendarSource=useRef<string|null>(null),calendarTurn=useRef<object|null>(null),calendarLastContext=useRef<string|null>(null),calendarSourceKey=JSON.stringify([base,selected.department,selected.week,selected.timezone]);
  useLayoutEffect(()=>{calendarSource.current=calendarSourceKey;return()=>{calendarSource.current=null;};},[calendarSourceKey]);
  const calendarContext=useCallback(()=>JSON.stringify([calendarSource.current,calendarCommandContext()]),[]);
  const calendarHeld=calendarDirty||calendarBusy||calendarPending!==null;
  const closeAppointment=()=>{
    if(calendarBusy||calendarPending){setCalendarError('Recover the original team appointment before closing this form.');return;}
    if(calendarDirty){setCalendarDiscard(true);return;}
    setAppointment(false);
  };
  useEffect(()=>{
    const recover=()=>{const attempt=calendarCommands.current.pending();if(!attempt||attempt.context!==calendarContext()||calendarLastContext.current!==attempt.context)return;try{captureWorkCommandContext()();if(calendarSource.current===null)return;setCalendarPending(attempt);if(!calendarCommands.current.inflight()&&!calendarTurn.current)setCalendarBusy(false);}catch{}};
    recover();window.addEventListener('erpx:work-context-changed',recover);window.addEventListener('focus',recover);return()=>{window.removeEventListener('erpx:work-context-changed',recover);window.removeEventListener('focus',recover);};
  },[base,calendarSourceKey,calendarContext]);
  async function saveAppointment(input?:unknown,completion?:WorkCommandCompletion,captured?:CalendarCommandAttempt){
    if(calendarCommands.current.inflight()||calendarTurn.current)return;
    const source=calendarSource.current,assertWorkCurrent=captureWorkCommandContext(),turn={};
    const current=()=>{try{assertWorkCurrent();return source!==null&&calendarSource.current===source;}catch{return false;}};
    if(!current())return;calendarTurn.current=turn;setCalendarBusy(true);setCalendarError('');setCalendarMessage('');
    try{
      const attempt=captured??calendarCommands.current.prepare(input,completion,calendarContext());calendarLastContext.current=attempt.context;
      const result=await calendarCommands.current.execute(`/api/v1${base}/calendar/appointments`,attempt,calendarContext,'caller');
      if(!current())return;setCalendarPending(calendarCommands.current.pending());
      await calendar.reload();if(!current())return;result.assertCurrent();result.acknowledge();const unchanged=result.unchanged;setCalendarPending(calendarCommands.current.pending());
      if(unchanged){setAppointment(false);setCalendarDirty(false);if(attempt.completion)window.dispatchEvent(new CustomEvent('erpx:record-command-saved',{detail:attempt.completion}));}
      setCalendarMessage(unchanged?'Team appointment saved to Calendar.':'Submitted team appointment saved. Your newer draft is retained.');
    }catch(error){if(current()){setCalendarPending(calendarCommands.current.pending());setCalendarError(error instanceof Error?error.message:'The team appointment could not be saved.');}}
    finally{if(calendarTurn.current===turn){calendarTurn.current=null;if(current())setCalendarBusy(false);}}
  }
  useEffect(() => {
    const listener = (event: Event) => {
      const detail = (event as CustomEvent<{ base: string; data: RosterData }>).detail;
      if (detail.base === base) setLatest({ key: contextKey, data: detail.data });
    };
    window.addEventListener("erpx:roster-data", listener);
    return () => window.removeEventListener("erpx:roster-data", listener);
  }, [base, contextKey]);
  function updateContext(data: RosterData) {
    setLatest({ key: contextKey, data });
    const url = new URL(window.location.href);
    url.searchParams.set("panel", "schedule");
    url.searchParams.set("week", data.weekStart);
    if (data.departmentId) url.searchParams.set("department", data.departmentId);
    else url.searchParams.delete("department");
    window.history.replaceState(null, "", url);
  }
  const items = calendar.data?.items ?? [];
  const people =
    hierarchy.data?.people.filter(
      (person) => !selected.department || person.departmentId === selected.department,
    ) ?? [];
  const title =
    view === "mine"
      ? "My shifts"
      : view === "requests"
        ? "Roster requests"
        : view === "templates"
          ? "Shift templates"
          : "Team roster";
  return (
    <CalendarDraftBoundary dirty={calendarDirty} busy={calendarBusy} pending={calendarPending!==null}>
    <section className={styles.settings} aria-label="Team schedule">
      <header className={styles.heading}>
        <div>
          <Button type="submit" variant="plain" className={styles.back} disabled={rosterHeld||calendarHeld} onClick={()=>{if(calendarHeld){setCalendarError("Finish or recover the team appointment before leaving this schedule.");return;}if(rosterOwnerDraftHeld(rosterDraftLive.current)){setOwnerWarning("Finish or recover the retained Roster draft before leaving this schedule.");return;}onBack();}}>
            <ArrowLeft size={16} />
            Back to structure
          </Button>
          <h2>{title}</h2>
          <p>Shifts, leave, holidays and appointments together</p>
        </div>
        {calendar.data?.canSchedule && (
          <Button
            type="submit"
            variant="primary"
            className={styles.button}
            onClick={() => {if(calendarHeld||!hierarchy.data)return;setAppointmentSnapshot({hierarchy:hierarchy.data,date:selected.week,timezone:selected.timezone,participantIds:people.filter(person=>person.status!=="ended").map(person=>person.id)});setAppointment(true);}}
            disabled={calendarHeld||!hierarchy.data || hierarchy.loading}
          >
            <CalendarPlus size={16} />
            Create team appointment
          </Button>
        )}
      </header>
      {ownerWarning&&<p className={styles.error} role="alert">{ownerWarning}</p>}
      {roster.loading && !roster.data && (
        <p className={styles.loading} role="status">
          Loading team schedule…
        </p>
      )}
      {roster.error && (
        <p className={styles.error} role="alert">
          {roster.error}
          <Button
            type="submit"
            variant="primary"
            className={styles.button}
            onClick={() => void roster.reload()}
          >
            Retry roster
          </Button>
        </p>
      )}
      {calendar.error && (
        <p className={styles.error} role="alert">
          {calendar.error}
          <Button
            type="submit"
            variant="primary"
            className={styles.button}
            onClick={() => void calendar.reload()}
          >
            Retry calendar
          </Button>
        </p>
      )}
      {calendar.data?.sources
        .filter((source) => source.status === "error")
        .map((source) => (
          <p className={styles.error} key={source.source} role="alert">
            {source.message}
            <Button
              type="submit"
              variant="primary"
              className={styles.button}
              onClick={() => void calendar.reload()}
            >
              Refresh calendar
            </Button>
          </p>
        ))}
      <Dialog appearance="standard" open={appointment} title="Create team appointment" presentation="form-drawer" retainFormContents={calendarHeld} dismissDisabled={calendarBusy||calendarPending!==null} onClose={closeAppointment}>
        {appointmentSnapshot&&<>
          {calendarError&&<p className={styles.error} role="alert">{calendarError}</p>}
          {calendarPending&&<div role="alert"><p>The original team appointment has not been confirmed. Your newer draft is retained.</p><Button appearance="standard" disabled={calendarBusy} onClick={()=>void saveAppointment(undefined,undefined,calendarPending)}>Retry submitted team appointment</Button></div>}
          <CalendarForm appearance="legacy" editor={{kind:"appointment"}} hierarchy={appointmentSnapshot.hierarchy} date={appointmentSnapshot.date} selfPersonId={appointmentSnapshot.hierarchy.selfPersonId} busy={calendarBusy} api={`/api/v1${base}/calendar`} initialTimezone={appointmentSnapshot.timezone} initialParticipantIds={appointmentSnapshot.participantIds} send={(input,completion)=>void saveAppointment(input,completion)} edited={()=>setCalendarDirty(true)} close={closeAppointment}/>
        </>}
      </Dialog>
      <Dialog appearance="standard" open={calendarDiscard} title="Discard team appointment draft?" onClose={()=>setCalendarDiscard(false)}><p>Your team appointment has not been saved.</p><div className="ui-form-actions"><Button appearance="standard" onClick={()=>setCalendarDiscard(false)}>Keep editing</Button>{!calendarBusy&&!calendarPending&&<Button appearance="standard" variant="danger" onClick={()=>{setCalendarDiscard(false);setCalendarDirty(false);setAppointment(false);setAppointmentSnapshot(null);}}>Discard appointment draft</Button>}</div></Dialog>
      {calendarError&&!appointment&&<p className={styles.error} role="alert">{calendarError}</p>}
      {calendarMessage&&<p className={styles.message} role="status">{calendarMessage}</p>}
      {data && (
        <>
          <div className={styles.form}>
            <Field>
              Schedule view
              <Select
                value={view}
                disabled={rosterHeld||calendarHeld}
                onChange={(event) => {
                  if(calendarHeld){setCalendarError("Finish or recover the team appointment before changing this view.");return;}
                  if(rosterOwnerDraftHeld(rosterDraftLive.current)){setOwnerWarning("Finish or recover the retained Roster draft before changing its view.");return;}
                  const url = new URL(window.location.href);
                  url.searchParams.set("view", event.target.value);
                  window.history.pushState(null, "", url);
                }}
              >
                {data.selfRosterRequired && <option value="mine">My shifts</option>}
                <option value="team">Team roster</option>
                <option value="requests">Roster requests</option>
                {data.canManage && <option value="templates">Shift templates</option>}
              </Select>
            </Field>
          </div>
          <RosterWorkspace appearance="legacy"
            key={ownedDraft?.key??`${contextKey}:${view}`}
            initial={data}
            ownerAvailable={!!freshData}
            base={base}
            initialView={view}
            requestId={requestId}
            embedded
            calendarItems={items}
            onDataChange={updateContext}
            onDraftStateChange={trackRosterDraft}
          />
        </>
      )}
      {data && !data.week && items.length > 0 && (
        <div className={rosterStyles.calendar} aria-label="Team calendar">
          {Array.from({ length: 7 }, (_, index) => addDays(selected.week, index)).map((date) => (
            <section className={rosterStyles.day} key={date}>
              <header>
                <strong>
                  {new Intl.DateTimeFormat("en", {
                    timeZone: "UTC",
                    weekday: "short",
                    day: "numeric",
                    month: "short",
                  }).format(new Date(`${date}T12:00:00Z`))}
                </strong>
              </header>
              <div className={rosterStyles.dayBody}>
                {items
                  .filter((item) => itemOnDate(item, date, selected.timezone))
                  .map((item) => (
                    <article className={rosterStyles.calendarContext} key={item.id}>
                      <small>
                        {item.kind === "holiday"
                          ? "Holiday"
                          : item.kind === "leave"
                            ? "Leave"
                            : "Appointment"}{" "}
                        · {eventTime(item, selected.timezone)}
                      </small>
                      <h3>{item.title}</h3>
                      <p>{item.participants.map((person) => person.name).join(", ")}</p>
                    </article>
                  ))}
              </div>
            </section>
          ))}
        </div>
      )}
    </section>
    </CalendarDraftBoundary>
  );
}

export function PeopleWorkingTime({ base, engagementId }: { base: string; engagementId: string }) {
  const resource = usePeopleIntegrationData<Awaited<ReturnType<typeof peopleWorkingTimeData>>>(
    `/api/v1${base}/people/hr-settings?engagement=${encodeURIComponent(engagementId)}`,
  );
  const command = usePeopleIntegrationCommand(base, resource.reload);
  const data = resource.data,
    current = data?.assignments.find(
      (assignment) =>
        assignment.effective_from <= data.asOf &&
        (!assignment.effective_to || assignment.effective_to >= data.asOf),
    );
  const workweek = current
    ? Array.isArray(current.workweek_json)
      ? current.workweek_json
      : (JSON.parse(String(current.workweek_json)) as number[])
    : [];
  return (
    <section className={styles.stack} aria-label="Working time settings">
      {resource.loading && !data && (
        <p className={styles.loading} role="status">
          Loading working time…
        </p>
      )}
      {resource.error && (
        <p className={styles.error} role="alert">
          {resource.error}
          <Button
            type="submit"
            variant="primary"
            className={styles.button}
            onClick={() => void resource.reload()}
          >
            Retry
          </Button>
        </p>
      )}
      {data && (
        <>
          <dl className={styles.summary}>
            <div>
              <dt>Work policy</dt>
              <dd>{current?.policy ?? "No current work policy"}</dd>
            </div>
            <div>
              <dt>Working days</dt>
              <dd>
                {workweek.map((day) => weekdays[day - 1]).join(", ") ||
                  "Follow the assigned work policy"}
              </dd>
            </div>
            <div>
              <dt>Holiday calendar</dt>
              <dd>{current?.calendar ?? "No holiday calendar assigned"}</dd>
            </div>
            <div>
              <dt>Timezone</dt>
              <dd>{current?.timezone ?? "From the work policy"}</dd>
            </div>
          </dl>
          {data.canManage && (
            <>
              <FormDisclosure className={styles.row}>
                <summary>Assign work policy & holiday calendar</summary>
                <form
                  className={styles.form}
                  onSubmit={(event) => {
                    const f = form(event);
                    void command.send("leave/admin", {
                      resource: "assignment",
                      engagementId,
                      policyVersionId: value(f, "policy"),
                      calendarId: value(f, "calendar") || null,
                      effectiveFrom: value(f, "from"),
                      effectiveTo: value(f, "to") || null,
                    });
                  }}
                >
                  <Field>
                    Work policy
                    <Select name="policy" required>
                      <option value="">Choose a policy version</option>
                      {data.policies.map((policy) => (
                        <option key={policy.id} value={policy.id}>
                          {policy.name} · v{policy.version}
                        </option>
                      ))}
                    </Select>
                  </Field>
                  <Field>
                    Holiday calendar
                    <Select name="calendar">
                      <option value="">No calendar</option>
                      {data.calendars.map((calendar) => (
                        <option key={calendar.id} value={calendar.id}>
                          {calendar.name}
                        </option>
                      ))}
                    </Select>
                  </Field>
                  <div className={styles.fields}>
                    <Field>
                      Effective from
                      <TextInput type="date" name="from" required defaultValue={data.asOf} />
                    </Field>
                    <Field>
                      Effective to
                      <TextInput type="date" name="to" />
                    </Field>
                  </div>
                  <p className={styles.note}>
                    End an overlapping assignment first. Dates and previous policy versions remain
                    in the staff history.
                  </p>
                  <Button
                    clientManaged type="submit"
                    variant="primary"
                    className={styles.primary}
                    disabled={command.busy || !data.policies.length}
                  >
                    Assign working time
                  </Button>
                </form>
              </FormDisclosure>
              {data.assignments
                .filter((assignment) => !assignment.effective_to)
                .map((assignment) => (
                  <details className={styles.row} key={`${assignment.id}:${assignment.version}`}>
                    <summary>End {assignment.policy} assignment</summary>
                    <form
                      className={styles.form}
                      onSubmit={(event) => {
                        const f = form(event);
                        void command.send("leave/admin", {
                          resource: "assignment_end",
                          id: assignment.id,
                          expectedVersion: assignment.version,
                          effectiveTo: value(f, "to"),
                        });
                      }}
                    >
                      <Field>
                        Last day of this assignment
                        <TextInput type="date" name="to" min={assignment.effective_from} required />
                      </Field>
                      <Button
                        clientManaged type="submit"
                        variant="primary"
                        className={styles.button}
                        disabled={command.busy}
                      >
                        End policy assignment
                      </Button>
                    </form>
                  </details>
                ))}
            </>
          )}
          {data.assignments.length > 1 && (
            <details className={styles.row}>
              <summary>Assignment history</summary>
              {data.assignments.map((assignment) => (
                <p className={styles.note} key={assignment.id}>
                  {assignment.policy} · {assignment.calendar ?? "No holiday calendar"} ·{" "}
                  {assignment.effective_from} — {assignment.effective_to ?? "ongoing"}
                </p>
              ))}
            </details>
          )}
        </>
      )}
      {command.error && (
        <p className={styles.error} role="alert">
          {command.error}
        </p>
      )}
      {command.message && (
        <p className={styles.message} role="status">
          {command.message}
        </p>
      )}
    </section>
  );
}

export function PeoplePositionWorkingTime({
  base,
  positionId,
}: {
  base: string;
  positionId: string;
}) {
  const resource = usePeopleIntegrationData<
      Awaited<ReturnType<typeof peoplePositionWorkingTimeData>>
    >(`/api/v1${base}/people/hr-settings?position=${encodeURIComponent(positionId)}`),
    command = usePeopleIntegrationCommand(base, resource.reload);
  const position = resource.data?.position;
  return (
    <section className={styles.stack} aria-label="Position working time">
      {resource.loading && !position && (
        <p className={styles.loading} role="status">
          Loading roster requirement…
        </p>
      )}
      {resource.error && (
        <p className={styles.error} role="alert">
          {resource.error}
        </p>
      )}
      {position && (
        <form
          key={`${position.id}:${position.version}`}
          className={styles.form}
          onSubmit={(event) => {
            const f = form(event);
            void command.send(`people/organization/positions/${position.id}`, {
              title: position.title,
              branchId: position.branch_id,
              departmentId: position.department_id,
              levelId: position.level_id,
              reportsToPositionId: position.reports_to_position_id,
              isCeo: position.is_ceo === 1,
              rosterRequired: f.has("roster"),
              active: position.active === 1,
              expectedVersion: position.version,
              requirements: position.requirements ?? "",
              reason: value(f, "reason"),
            });
          }}
        >
          <Field orientation="inline" className={styles.check}>
            <TextInput
              type="checkbox"
              name="roster"
              defaultChecked={position.roster_required === 1}
            />
            Staff in this position require a weekly roster
          </Field>
          <p className={styles.note}>
            Staff in a roster position need published shifts to request roster-based leave. Changes
            follow the position’s existing roster workflow.
          </p>
          <Field>
            Reason
            <TextArea name="reason" required minLength={3} maxLength={1000} />
          </Field>
          <Button
            clientManaged type="submit"
            variant="primary"
            className={styles.primary}
            disabled={command.busy}
          >
            Save roster requirement
          </Button>
        </form>
      )}
      {command.error && (
        <p className={styles.error} role="alert">
          {command.error}
        </p>
      )}
      {command.message && (
        <p className={styles.message} role="status">
          {command.message}
        </p>
      )}
    </section>
  );
}

export function PeopleMoveWorkImpact({
  base,
  engagementIds,
  onReadyChange,
}: {
  base: string;
  engagementIds: string[];
  onReadyChange?: (ready: boolean) => void;
}) {
  const ids = [...new Set(engagementIds)].sort();
  const [batches, setBatches] = useState<Record<string, { signature: string; ready: boolean }>>({});
  const report = useCallback(
    (key: string, signature: string, ready: boolean) =>
      setBatches((previous) =>
        previous[key]?.signature === signature && previous[key]?.ready === ready
          ? previous
          : { ...previous, [key]: { signature, ready } },
      ),
    [],
  );
  const chunks = Array.from({ length: Math.ceil(ids.length / 40) }, (_, index) =>
    ids.slice(index * 40, index * 40 + 40),
  );
  const ready = chunks.every(
    (chunk) => batches[chunk[0]]?.signature === chunk.join(",") && batches[chunk[0]]?.ready,
  );
  useEffect(() => {
    onReadyChange?.(ready);
  }, [ready, onReadyChange]);
  useEffect(
    () => () => {
      onReadyChange?.(false);
    },
    [onReadyChange],
  );
  return ids.length ? (
    <section className={styles.stack} aria-label="Schedule and policy impact">
      <h3>Schedule, leave & compensation</h3>
      {chunks.map((chunk) => (
        <MoveImpact key={chunk[0]} base={base} ids={chunk} report={report} />
      ))}
      <p className={styles.note}>
        Published shifts and approved leave need review when changing departments. Compensation
        changes use their own effective date and approval workflow.
      </p>
    </section>
  ) : null;
}
function MoveImpact({
  base,
  ids,
  report,
}: {
  base: string;
  ids: string[];
  report: (key: string, signature: string, ready: boolean) => void;
}) {
  const resource = usePeopleIntegrationData<Awaited<ReturnType<typeof peopleMoveWorkImpact>>>(
    `/api/v1${base}/people/hr-settings?impact=${encodeURIComponent(ids.join(","))}`,
  );
  const batchKey = ids[0],
    signature = ids.join(","),
    ready = !!resource.data && !resource.loading && !resource.error;
  useEffect(() => {
    report(batchKey, signature, ready);
  }, [batchKey, signature, ready, report]);
  return (
    <>
      {resource.loading && !resource.data && (
        <p className={styles.loading} role="status">
          Checking affected staff…
        </p>
      )}
      {resource.error && (
        <p className={styles.error} role="alert">
          {resource.error}
          <Button
            type="submit"
            variant="primary"
            className={styles.button}
            onClick={() => void resource.reload()}
          >
            Retry impact check
          </Button>
        </p>
      )}
      {resource.data?.people.map((person) => (
        <div className={styles.row} key={person.engagementId}>
          <strong>{person.name}</strong>
          <small>
            {person.upcomingShifts === null
              ? "Roster details require roster read permission"
              : `${person.upcomingShifts} upcoming assigned shifts`}{" "}
            · {person.upcomingLeave} approved absences ahead
          </small>
          <small>
            Work policy: {person.policy ?? "Unassigned"} · Holidays:{" "}
            {person.calendar ?? "Unassigned"}
          </small>
          {person.payGroup !== undefined && (
            <small>
              Pay group: {person.payGroup ?? "Unassigned"} · Salary remains unchanged by this move
            </small>
          )}
        </div>
      ))}
    </>
  );
}
