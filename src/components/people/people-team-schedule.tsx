"use client";
import { ArrowLeft, CalendarPlus } from "lucide-react";
import { useCallback, useEffect, useState, type FormEvent } from "react";
import { CalendarForm } from "@/components/calendar/calendar-forms";
import { eventTime } from "@/components/calendar/calendar-event";
import { itemOnDate, type CalendarData, type CalendarHierarchy } from "@/lib/calendar";
import { addDays, rosterMonday, type RosterData } from "@/lib/people-roster";
import type { peopleMoveWorkImpact, peoplePositionWorkingTimeData, peopleWorkingTimeData } from "@/server/people-hr-settings";
import { RosterWorkspace } from "./roster-workspace";
import { usePeopleIntegrationCommand, usePeopleIntegrationData } from "./people-integration-data";
import styles from "./people-hr-settings.module.css";
import rosterStyles from "./roster.module.css";

function form(event:FormEvent<HTMLFormElement>) {event.preventDefault();return new FormData(event.currentTarget);}
const value = (fields:FormData,key:string) => String(fields.get(key)??"");
const weekdays = ["Monday","Tuesday","Wednesday","Thursday","Friday","Saturday","Sunday"];
function selectedMonday(date?:string) {
  if(!date || !/^\d{4}-\d{2}-\d{2}$/.test(date) || !Number.isFinite(Date.parse(`${date}T12:00:00Z`)) || new Date(`${date}T12:00:00Z`).toISOString().slice(0,10)!==date)return rosterMonday();
  return rosterMonday(date);
}

export function PeopleTeamSchedule({base,departmentId,initialWeek,initialView,requestId,onBack}: {base:string;departmentId?:string;initialWeek?:string;initialView?:string;requestId?:string;onBack:()=>void}) {
  const query = new URLSearchParams({week:selectedMonday(initialWeek),...(departmentId?{department:departmentId}:{})});
  const roster = usePeopleIntegrationData<RosterData>(`/api/v1${base}/people/roster?${query}`);
  const contextKey = `${base}:${query}`;
  const [latest,setLatest] = useState<{key:string;data:RosterData}|null>(null), [appointment,setAppointment] = useState(false);
  const data = latest?.key===contextKey ? latest.data : roster.data;
  const defaultView = data?.selfRosterRequired ? "mine" : "team";
  const permittedView = initialView==="mine" ? data?.selfRosterRequired : initialView==="templates" ? data?.canManage : initialView==="team" || initialView==="requests";
  const view = permittedView ? initialView! : defaultView;
  const selected = data ? {department:data.departmentId,week:data.weekStart,timezone:data.week?.timezone??"Asia/Bangkok"} : {department:departmentId??null,week:selectedMonday(initialWeek),timezone:"Asia/Bangkok"};
  const calendarQuery = new URLSearchParams({date:selected.week,view:"week",mode:view==="mine"?"mine":"team",layers:"leave,appointments",timezone:selected.timezone,limit:"200",...(selected.department?{node:selected.department}:{})});
  const calendar = usePeopleIntegrationData<CalendarData>(`/api/v1${base}/calendar?${calendarQuery}`,"calendar");
  const hierarchy = usePeopleIntegrationData<CalendarHierarchy>(`/api/v1${base}/calendar/hierarchy?limit=200`,"hierarchy");
  const command = usePeopleIntegrationCommand(base,calendar.reload);
  useEffect(()=>{const listener=(event:Event)=>{const detail=(event as CustomEvent<{base:string;data:RosterData}>).detail;if(detail.base===base)setLatest({key:contextKey,data:detail.data});};window.addEventListener("erpx:roster-data",listener);return()=>window.removeEventListener("erpx:roster-data",listener);},[base,contextKey]);
  function updateContext(data:RosterData) {
    setLatest({key:contextKey,data});
    const url = new URL(window.location.href);
    url.searchParams.set("panel","schedule"); url.searchParams.set("week",data.weekStart);
    if(data.departmentId)url.searchParams.set("department",data.departmentId);else url.searchParams.delete("department");
    window.history.replaceState(null,"",url);
  }
  const items = calendar.data?.items??[];
  const people = hierarchy.data?.people.filter(person=>!selected.department || person.departmentId===selected.department)??[];
  const title = view==="mine"?"My shifts":view==="requests"?"Roster requests":view==="templates"?"Shift templates":"Team roster";
  return <section className={styles.settings} aria-label="Team schedule"><header className={styles.heading}><div><button className={styles.back} onClick={onBack}><ArrowLeft size={16}/>Back to structure</button><h2>{title}</h2><p>Shifts, leave, holidays and appointments together</p></div>{calendar.data?.canSchedule && <button className={styles.button} onClick={()=>setAppointment(true)} disabled={!hierarchy.data || hierarchy.loading}><CalendarPlus size={16}/>Create team appointment</button>}</header>
    {roster.loading && !roster.data && <p className={styles.loading} role="status">Loading team schedule…</p>}
    {roster.error && <p className={styles.error} role="alert">{roster.error}<button className={styles.button} onClick={()=>void roster.reload()}>Retry roster</button></p>}
    {calendar.error && <p className={styles.error} role="alert">{calendar.error}<button className={styles.button} onClick={()=>void calendar.reload()}>Retry calendar</button></p>}
    {calendar.data?.sources.filter(source=>source.status==="error").map(source=><p className={styles.error} key={source.source} role="alert">{source.message}<button className={styles.button} onClick={()=>void calendar.reload()}>Refresh calendar</button></p>)}
    {appointment && hierarchy.data && <section className={styles.stack} aria-label="Create team appointment"><h3>Create team appointment</h3>{command.error && <p className={styles.error} role="alert">{command.error}</p>}<CalendarForm editor={{kind:"appointment"}} hierarchy={hierarchy.data} date={selected.week} selfPersonId={hierarchy.data.selfPersonId} busy={command.busy} api={`/api/v1${base}/calendar`} initialTimezone={selected.timezone} initialParticipantIds={people.filter(person=>person.status!=="ended").map(person=>person.id)} send={async input=>{if(await command.send("calendar/appointments",input,"Team appointment saved to Calendar."))setAppointment(false);}} close={()=>setAppointment(false)}/></section>}
    {command.message && <p className={styles.message} role="status">{command.message}</p>}
    {data && <><div className={styles.form}><label>Schedule view<select value={view} onChange={event=>{const url=new URL(window.location.href);url.searchParams.set("view",event.target.value);window.history.pushState(null,"",url);}}>{data.selfRosterRequired && <option value="mine">My shifts</option>}<option value="team">Team roster</option><option value="requests">Roster requests</option>{data.canManage && <option value="templates">Shift templates</option>}</select></label></div><RosterWorkspace key={`${contextKey}:${view}`} initial={data} base={base} initialView={view} requestId={requestId} embedded calendarItems={items} onDataChange={updateContext}/></>}
    {data && !data.week && items.length>0 && <div className={rosterStyles.calendar} aria-label="Team calendar">{Array.from({length:7},(_,index)=>addDays(selected.week,index)).map(date=><section className={rosterStyles.day} key={date}><header><strong>{new Intl.DateTimeFormat("en",{timeZone:"UTC",weekday:"short",day:"numeric",month:"short"}).format(new Date(`${date}T12:00:00Z`))}</strong></header><div className={rosterStyles.dayBody}>{items.filter(item=>itemOnDate(item,date,selected.timezone)).map(item=><article className={rosterStyles.calendarContext} key={item.id}><small>{item.kind==="holiday"?"Holiday":item.kind==="leave"?"Leave":"Appointment"} · {eventTime(item,selected.timezone)}</small><h3>{item.title}</h3><p>{item.participants.map(person=>person.name).join(", ")}</p></article>)}</div></section>)}</div>}
  </section>;
}

export function PeopleWorkingTime({base,engagementId}: {base:string;engagementId:string}) {
  const resource = usePeopleIntegrationData<Awaited<ReturnType<typeof peopleWorkingTimeData>>>(`/api/v1${base}/people/hr-settings?engagement=${encodeURIComponent(engagementId)}`);
  const command = usePeopleIntegrationCommand(base,resource.reload);
  const data=resource.data, current=data?.assignments.find(assignment=>assignment.effective_from<=data.asOf && (!assignment.effective_to || assignment.effective_to>=data.asOf));
  const workweek=current ? (Array.isArray(current.workweek_json)?current.workweek_json:JSON.parse(String(current.workweek_json)) as number[]) : [];
  return <section className={styles.stack} aria-label="Working time settings">
    {resource.loading && !data && <p className={styles.loading} role="status">Loading working time…</p>}{resource.error && <p className={styles.error} role="alert">{resource.error}<button className={styles.button} onClick={()=>void resource.reload()}>Retry</button></p>}
    {data && <><dl className={styles.summary}><div><dt>Work policy</dt><dd>{current?.policy??"No current work policy"}</dd></div><div><dt>Working days</dt><dd>{workweek.map(day=>weekdays[day-1]).join(", ")||"Follow the assigned work policy"}</dd></div><div><dt>Holiday calendar</dt><dd>{current?.calendar??"No holiday calendar assigned"}</dd></div><div><dt>Timezone</dt><dd>{current?.timezone??"From the work policy"}</dd></div></dl>
      {data.canManage && <><details className={styles.row}><summary>Assign work policy & holiday calendar</summary><form className={styles.form} onSubmit={event=>{const f=form(event);void command.send("leave/admin",{resource:"assignment",engagementId,policyVersionId:value(f,"policy"),calendarId:value(f,"calendar")||null,effectiveFrom:value(f,"from"),effectiveTo:value(f,"to")||null});}}><label>Work policy<select name="policy" required><option value="">Choose a policy version</option>{data.policies.map(policy=><option key={policy.id} value={policy.id}>{policy.name} · v{policy.version}</option>)}</select></label><label>Holiday calendar<select name="calendar"><option value="">No calendar</option>{data.calendars.map(calendar=><option key={calendar.id} value={calendar.id}>{calendar.name}</option>)}</select></label><div className={styles.fields}><label>Effective from<input type="date" name="from" required defaultValue={data.asOf}/></label><label>Effective to<input type="date" name="to"/></label></div><p className={styles.note}>End an overlapping assignment first. Dates and previous policy versions remain in the staff history.</p><button className={styles.primary} disabled={command.busy || !data.policies.length}>Assign working time</button></form></details>
        {data.assignments.filter(assignment=>!assignment.effective_to).map(assignment=><details className={styles.row} key={`${assignment.id}:${assignment.version}`}><summary>End {assignment.policy} assignment</summary><form className={styles.form} onSubmit={event=>{const f=form(event);void command.send("leave/admin",{resource:"assignment_end",id:assignment.id,expectedVersion:assignment.version,effectiveTo:value(f,"to")});}}><label>Last day of this assignment<input type="date" name="to" min={assignment.effective_from} required/></label><button className={styles.button} disabled={command.busy}>End policy assignment</button></form></details>)}
      </>}
      {data.assignments.length>1 && <details className={styles.row}><summary>Assignment history</summary>{data.assignments.map(assignment=><p className={styles.note} key={assignment.id}>{assignment.policy} · {assignment.calendar??"No holiday calendar"} · {assignment.effective_from} — {assignment.effective_to??"ongoing"}</p>)}</details>}
    </>}
    {command.error && <p className={styles.error} role="alert">{command.error}</p>}{command.message && <p className={styles.message} role="status">{command.message}</p>}
  </section>;
}

export function PeoplePositionWorkingTime({base,positionId}: {base:string;positionId:string}) {
  const resource=usePeopleIntegrationData<Awaited<ReturnType<typeof peoplePositionWorkingTimeData>>>(`/api/v1${base}/people/hr-settings?position=${encodeURIComponent(positionId)}`), command=usePeopleIntegrationCommand(base,resource.reload);
  const position=resource.data?.position;
  return <section className={styles.stack} aria-label="Position working time">{resource.loading && !position && <p className={styles.loading} role="status">Loading roster requirement…</p>}{resource.error && <p className={styles.error} role="alert">{resource.error}</p>}{position && <form key={`${position.id}:${position.version}`} className={styles.form} onSubmit={event=>{const f=form(event);void command.send(`people/organization/positions/${position.id}`,{title:position.title,branchId:position.branch_id,departmentId:position.department_id,levelId:position.level_id,reportsToPositionId:position.reports_to_position_id,isCeo:position.is_ceo===1,rosterRequired:f.has("roster"),active:position.active===1,expectedVersion:position.version,requirements:position.requirements??"",reason:value(f,"reason")});}}><label className={styles.check}><input type="checkbox" name="roster" defaultChecked={position.roster_required===1}/>Staff in this position require a weekly roster</label><p className={styles.note}>Staff in a roster position need published shifts to request roster-based leave. Changes follow the position’s existing roster workflow.</p><label>Reason<textarea name="reason" required minLength={3} maxLength={1000}/></label><button className={styles.primary} disabled={command.busy}>Save roster requirement</button></form>}{command.error && <p className={styles.error} role="alert">{command.error}</p>}{command.message && <p className={styles.message} role="status">{command.message}</p>}</section>;
}

export function PeopleMoveWorkImpact({base,engagementIds,onReadyChange}: {base:string;engagementIds:string[];onReadyChange?:(ready:boolean)=>void}) {
  const ids=[...new Set(engagementIds)].sort();
  const [batches,setBatches]=useState<Record<string,{signature:string;ready:boolean}>>({});
  const report=useCallback((key:string,signature:string,ready:boolean)=>setBatches(previous=>previous[key]?.signature===signature && previous[key]?.ready===ready ? previous : {...previous,[key]:{signature,ready}}),[]);
  const chunks=Array.from({length:Math.ceil(ids.length/40)},(_,index)=>ids.slice(index*40,index*40+40));
  const ready=chunks.every(chunk=>batches[chunk[0]]?.signature===chunk.join(",") && batches[chunk[0]]?.ready);
  useEffect(()=>{onReadyChange?.(ready);},[ready,onReadyChange]);
  useEffect(()=>()=>{onReadyChange?.(false);},[onReadyChange]);
  return ids.length ? <section className={styles.stack} aria-label="Schedule and policy impact"><h3>Schedule, leave & compensation</h3>{chunks.map(chunk=><MoveImpact key={chunk[0]} base={base} ids={chunk} report={report}/>)}<p className={styles.note}>Published shifts and approved leave need review when changing departments. Compensation changes use their own effective date and approval workflow.</p></section> : null;
}
function MoveImpact({base,ids,report}: {base:string;ids:string[];report:(key:string,signature:string,ready:boolean)=>void}) {
  const resource=usePeopleIntegrationData<Awaited<ReturnType<typeof peopleMoveWorkImpact>>>(`/api/v1${base}/people/hr-settings?impact=${encodeURIComponent(ids.join(","))}`);
  const batchKey=ids[0], signature=ids.join(","), ready=!!resource.data && !resource.loading && !resource.error;
  useEffect(()=>{report(batchKey,signature,ready);},[batchKey,signature,ready,report]);
  return <>{resource.loading && !resource.data && <p className={styles.loading} role="status">Checking affected staff…</p>}{resource.error && <p className={styles.error} role="alert">{resource.error}<button className={styles.button} onClick={()=>void resource.reload()}>Retry impact check</button></p>}{resource.data?.people.map(person=><div className={styles.row} key={person.engagementId}><strong>{person.name}</strong><small>{person.upcomingShifts===null?"Roster details require roster read permission":`${person.upcomingShifts} upcoming assigned shifts`} · {person.upcomingLeave} approved absences ahead</small><small>Work policy: {person.policy??"Unassigned"} · Holidays: {person.calendar??"Unassigned"}</small>{person.payGroup!==undefined && <small>Pay group: {person.payGroup??"Unassigned"} · Salary remains unchanged by this move</small>}</div>)}</>;
}
