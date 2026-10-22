"use client";
import { canonicalUiHref } from "@erpx/feature-work_projection/lib/destination-registry";
import { Button, Field, Select, TextArea, TextInput } from "@erpx/ui/components/foundation";

import { Dialog } from "@erpx/ui/components/dialog";
import { DraftFormDrawer } from "@erpx/ui/components/draft-form-drawer";
import { TimeText } from "@erpx/feature-people_payroll/components/time/time-preference-context";
import { FeatureWidget, WidgetGrid } from "@erpx/feature-shared/components/features/feature-page";
import { StatusBadge } from "@erpx/ui/components/primitives";
import {
  addDays,
  rosterMonday,
  rosterPending,
  type RosterData,
  type RosterRequest,
  type RosterShift,
  type ShiftTemplate,
} from "@erpx/feature-people_payroll/lib/people-roster";
import { workFetch } from "@erpx/feature-work_projection/lib/work-fetch";
import { registerUnsavedWork } from "@erpx/feature-work_projection/lib/context-switch";
import { announceWorkChange } from "@erpx/feature-work_projection/lib/work-refresh";
import { currentRosterSnapshot } from "@erpx/feature-people_payroll/lib/roster-editor-draft";
import { itemOnDate, type CalendarItem } from "@erpx/feature-work_projection/lib/calendar";
import { eventTime } from "@erpx/feature-work_projection/components/calendar/calendar-event";
import {
  AlertCircle,
  ArrowRightLeft,
  CalendarDays,
  Check,
  ChevronLeft,
  ChevronRight,
  Clock3,
  Copy,
  Plus,
  Send,
  Users,
} from "lucide-react";
import Link from "@erpx/workplace/components/workspace-link";
import { useEffect, useLayoutEffect, useId, useRef, useState, type FormEvent, type SyntheticEvent } from "react";
import { RosterShiftForm } from "@erpx/feature-people_payroll/components/roster-shift-form";
import legacyStyles from "@erpx/feature-people_payroll/components/roster.module.css";
import standardStyles from "@erpx/feature-people_payroll/components/roster-owner-workspace.module.css";
import type { UiAppearance } from "@erpx/ui/components/appearance";
import {createRosterOwnerCommands,currentRosterOwnerDraft,rosterOwnerDraftHeld,rosterOwnerReadVersion,type RosterOwnerDraftState} from "@erpx/feature-people_payroll/lib/roster-owner-command";

type Editor =
  | { kind: "week"; copy?: boolean }
  | { kind: "shift"; shift?: RosterShift; date?: string }
  | { kind: "template"; template?: ShiftTemplate }
  | { kind: "request"; targetId?: string; swap?: boolean }
  | { kind: "decision"; request: RosterRequest; decision: string }
  | { kind: "cancel"; shift: RosterShift }
  | { kind: "publish" };
const dateLabel = (
  date: string,
  options: Intl.DateTimeFormatOptions = { month: "short", day: "numeric" },
) =>
  new Intl.DateTimeFormat("en", { ...options, timeZone: "UTC" }).format(
    new Date(`${date}T12:00:00Z`),
  );
const requestLabels = { extra: "Extra shift", move: "Move shift", swap: "Shift swap" };
const stateLabels: Record<string, string> = {
  pending_coworker: "Awaiting coworker",
  pending_manager: "Awaiting manager",
  approved: "Approved",
  rejected: "Rejected",
  withdrawn: "Withdrawn",
  invalidated: "Needs a new request",
};
function form(e: FormEvent<HTMLFormElement>) {
  e.preventDefault();
  return new FormData(e.currentTarget);
}
function field(f: FormData, name: string) {
  return String(f.get(name) ?? "");
}

function RosterOwnerView({
  appearance = "legacy",
  initial,
  base,
  initialView,
  requestId,
  initialDate,
  embedded = false,
  calendarItems = [],
  onDataChange,
  onDraftStateChange,
  ownerAvailable=true,
}: {
  appearance?: UiAppearance;
  initialDate?: string;
  embedded?: boolean;
  initial: RosterData;
  base: string;
  initialView?: string;
  requestId?: string;
  calendarItems?: CalendarItem[];
  onDataChange?: (data: RosterData) => void;
  onDraftStateChange?: (state: RosterOwnerDraftState) => void;
  ownerAvailable?: boolean;
}) {
  const styles = appearance === "standard" ? standardStyles : legacyStyles;
  const [data, setData] = useState(initial),[observedInitial,setObservedInitial]=useState(initial);
  // Reconcile a genuinely new owner SSR snapshot without remounting editor fields.
  if(observedInitial!==initial){setObservedInitial(initial);setData(initial);}
  const view =
    initialView === "team"
      ? "team"
      : initialView === "templates"
        ? "templates"
        : initialView === "requests"
          ? "requests"
          : initialView === "mine"
            ? "mine"
            : initial.canManage
              ? "team"
              : "mine";
  const [editor, setEditor] = useState<Editor | null>(null);
  const editorRoot = useRef<HTMLDivElement>(null);
  const [commands] = useState(()=>createRosterOwnerCommands()),draft=useRef(commands),ownerId=useId();
  const submittedEditor=useRef<Editor|null>(null);
  const [pending,setPending]=useState(false),[editorOpen,setEditorOpen]=useState(false);
  const [refreshRequired,setRefreshRequired]=useState<{department:string|null;start:string}|null>(null);
  const [editorKey, setEditorKey] = useState(0);
  const [dirty, setDirty] = useState(false), [replacement, setReplacement] = useState<Editor | null>(null);
  const [completedEditor, setCompletedEditor] = useState(false);
  const reads = useRef(0);
  const snapshot = useRef(initial);
  const [busy, setBusy] = useState(false),
    [error, setError] = useState(""),
    [message, setMessage] = useState("");
  useLayoutEffect(()=>{snapshot.current=data;commands.bind(()=>({base,data,initial,view:initialView,available:ownerAvailable}));return()=>{commands.bind(()=>null);reads.current++;};},[commands,base,data,initial,initialView,ownerAvailable]);
  function notifyDraft(controlBusy=busy){if(!commands.isMounted())return;const state=commands.status();onDraftStateChange?.({ownerId,...state,busy:controlBusy||state.busy});}
  useLayoutEffect(()=>{const state=commands.status();onDraftStateChange?.({ownerId,...state,busy:busy||state.busy});},[commands,ownerId,dirty,pending,busy,onDraftStateChange]);
  useEffect(() => {
    const unregister = registerUnsavedWork(() => draft.current.isDirty());
    const unload = (event: BeforeUnloadEvent) => {if (draft.current.isDirty()) event.preventDefault();};
    window.addEventListener("beforeunload", unload);
    return () => {unregister(); window.removeEventListener("beforeunload", unload);};
  }, []);
  useEffect(() => {
    const update = (event: Event) => {
      const d = (event as CustomEvent<{ base: string; data: RosterData }>).detail;
      if (d.base === base && currentRosterSnapshot(snapshot.current, d.data) === d.data) {
        reads.current++; snapshot.current = d.data; setData(d.data);
      }
    };
    window.addEventListener("erpx:roster-data", update);
    return () => window.removeEventListener("erpx:roster-data", update);
  }, [base]);
  const endpoint = `/api/v1${base}/people/roster`;
  const week = data.week;
  const shifts = data.shifts.filter(
    (s) =>
      s.week_id === week?.id &&
      s.state === "active" &&
      (view !== "mine" || week?.state === "published"),
  );
  const assignments = data.assignments.filter(
    (a) => a.state === "assigned" && shifts.some((s) => s.id === a.shift_id),
  );
  const capacity = shifts.reduce((n, s) => n + s.capacity, 0);
  const pendingRequests = data.requests.filter((r) => rosterPending(r.state));
  const own = data.assignments.filter(
    (a) =>
      a.person_id === data.selfPersonId &&
      a.state === "assigned" &&
      data.shifts.some(
        (s) =>
          s.id === a.shift_id &&
          s.state === "active" &&
          data.weeks.some((w) => w.id === s.week_id && w.state === "published"),
      ),
  );
  const currentDepartment = data.departments.find((d) => d.id === data.departmentId);
  async function load(department = data.departmentId, start = data.weekStart, selectedView = view) {
    const read = ++reads.current;
    const q = new URLSearchParams({ week: start, ...(department ? { department } : {}) });
    const guard=commands.capture(),response = await workFetch(`${endpoint}?${q}`, { cache: "no-store" });
    const json = await response.json();guard.assertCurrent();
    if (!response.ok) throw new Error(json.error?.message ?? "Could not load the roster.");
    if (read !== reads.current || currentRosterSnapshot(snapshot.current, json.data) !== json.data) return;
    snapshot.current = json.data; setData(json.data);
    // Only a completed current native GET, with no unknown command, may rebase
    // version metadata. The existing keyed form values remain untouched.
    if(!commands.hasPending())setEditor(current=>{
      if(current?.kind==="template"&&current.template){const version=rosterOwnerReadVersion(json.data,"templates",current.template.id,department);return version?{...current,template:{...current.template,version}}:current;}
      if((current?.kind==="shift"||current?.kind==="cancel")&&current.shift){const version=rosterOwnerReadVersion(json.data,"shifts",current.shift.id,department,current.shift.week_id);return version?{...current,shift:{...current.shift,version}}:current;}
      if(current?.kind==="decision"){const version=rosterOwnerReadVersion(json.data,"requests",current.request.id,department);return version?{...current,request:{...current.request,version}}:current;}
      return current;
    });
    onDataChange?.(json.data);
    window.dispatchEvent(
      new CustomEvent("erpx:roster-data", { detail: { base, data: json.data } }),
    );
    if (!embedded)
      window.history.replaceState(null, "", canonicalUiHref(`${base}/people/roster?${q}&view=${selectedView}`));
  }
  async function navigate(department: string | null, start: string, refreshOnly=false) {
    if(commands.hasPending()||commands.pending()||!refreshOnly&&commands.isDirty()){setError("Recover or finish the current Roster draft before changing its owner or filters.");return;}
    setBusy(true);
    setError("");
    setMessage("");
    try {
      await load(department, start);setRefreshRequired(null);
    } catch (e) {
      setError(e instanceof Error ? e.message : "Could not load the roster.");
    } finally {
      setBusy(false);
    }
  }
  async function apply(run:()=>ReturnType<typeof commands.retry>) {
    setBusy(true);setPending(true);setError("");setMessage("");
    try{
      const operation=run();notifyDraft(true);const result=await operation;result.assertCurrent();const completion=result.completion();
      result.assertCurrent();if(!result.acknowledge())throw Error("The original Roster acknowledgement was already handled.");
      const input=result.input,saved=result.data,original=submittedEditor.current;
      result.assertCurrent();announceWorkChange();result.assertCurrent();setDirty(commands.isDirty());setPending(false);notifyDraft(true);
      let unchanged=result.unchanged();
      if(unchanged&&completion){result.assertCurrent();window.dispatchEvent(new CustomEvent("erpx:record-command-saved",{detail:completion}));result.assertCurrent();unchanged=result.unchanged();}
      if(unchanged){result.assertCurrent();setEditor(null);setEditorOpen(false);}
      else{
        // Receipt identity/version rebases independently of newer local fields.
        // Keep editorKey, so ShiftForm's actual controlled input state survives.
        result.assertCurrent();
        if(original?.kind==="template"&&input.action==="template.save")setEditor({...original,template:{id:saved.id,version:saved.version,department_id:input.departmentId,name:input.name,start_time:input.startTime,end_time:input.endTime,capacity:input.capacity,active:input.active}});
        else if(original?.kind==="shift"&&input.action==="shift.save")setEditor({...original,shift:{...original.shift,id:saved.id,version:saved.version,week_id:input.weekId,name:input.name,local_date:input.date,start_time:input.startTime,end_time:input.endTime,capacity:input.capacity,state:"active",template_id:input.templateId??null,starts_at:original.shift?.starts_at??0,ends_at:original.shift?.ends_at??0}});
        else if(["shift.cancel","request.decide","week.publish","week.create","request.create"].includes(input.action))setCompletedEditor(true);
      }
      result.assertCurrent();setMessage(unchanged?"Roster updated.":"Submitted change saved. Your newer draft is retained.");
      const refresh={department:data.departmentId,start:result.start};setRefreshRequired(refresh);
      try{await load(refresh.department,refresh.start);setRefreshRequired(null);}
      catch{setError("Your change was saved. Refresh the current roster before making another change; your draft is retained.");}
    }catch(cause){setError(cause instanceof Error?cause.message:"Could not save your change. Recover the original request.");}
    finally{setBusy(false);setPending(commands.hasPending());setDirty(commands.isDirty());notifyDraft(false);}
  }
  async function send(input:unknown,start=data.weekStart){
    if(busy||commands.hasPending()||commands.pending()||completedEditor||refreshRequired){setError("Recover the original request or refresh the saved roster before another change. Your draft is retained.");return;}
    submittedEditor.current=editor;
    const form=editorRoot.current?.querySelector("form"),completion=form?{form,formEditVersion:form.getAttribute("data-erpx-edit-version")}:undefined;
    await apply(()=>commands.send(input,start,completion));
  }
  const recover=()=>busy?Promise.resolve():apply(commands.retry);
  function recovery(){return pending?<div role="status"><p>The original Roster result is not confirmed. Your request and newer draft are retained.</p><Button appearance={appearance} type="button" variant="secondary" disabled={busy} onClick={()=>void recover()}>Recover original Roster request</Button></div>:null;}
  function activateEditor(value: Editor | null) {
    setEditorKey(draft.current.open());
    setDirty(false); setPending(false); setCompletedEditor(false); setReplacement(null);setEditorOpen(value!==null);
    setEditor(value);notifyDraft(false);
  }
  function changedEditor(event: SyntheticEvent<HTMLDivElement>) {
    const form = (event.target as HTMLElement).closest("form");
    if (!form || !editorRoot.current?.contains(form)) return;
    form.setAttribute("data-erpx-edit-version", crypto.randomUUID());
    draft.current.changed(); setDirty(true);notifyDraft();
  }
  function open(value: Editor) {
    if(busy||commands.hasPending()||commands.pending()){setError("Recover the original Roster request before replacing this editor.");return;}
    if (value.kind === "shift" && !value.date && !value.shift && initialDate)
      value = { ...value, date: initialDate };
    setError("");
    setMessage("");
    if (draft.current.isDirty()) setReplacement(value);
    else activateEditor(value);
  }
  const shiftLabel = (id?: string | null) => {
    const s = data.shifts.find((s) => s.id === id);
    return s
      ? `${dateLabel(s.local_date)} · ${s.name} · ${s.start_time}–${s.end_time}${s.end_time < s.start_time ? " (+1 day)" : ""}`
      : "Shift no longer available";
  };
  function shiftCard(s: RosterShift) {
    const aa = assignments.filter((a) => a.shift_id === s.id),
      mine = aa.some((a) => a.person_id === data.selfPersonId);
    const openSlots = Math.max(0, s.capacity - aa.length),
      isFuture = s.starts_at > data.asOf,
      canRequest = data.purchase?.newWork !== false && !!data.selfPersonId && week?.state === "published" && isFuture;
    return (
      <article
        key={s.id}
        className={`${styles.shift} ${mine ? styles.myShift : ""}`}
        data-testid="roster-shift"
      >
        <div className={styles.shiftTop}>
          <span className={styles.shiftTime}>
            <Clock3 size={13} />
            {s.start_time}–{s.end_time}
          </span>
          {s.end_time < s.start_time && <span className={styles.overnight}>+1 day</span>}
        </div>
        <h3>{s.name}</h3>
        <div className={styles.names}>
          {aa.map((a) => (
            <span key={a.id} className={a.person_id === data.selfPersonId ? styles.me : undefined}>
              {a.person_name}
              {a.person_id === data.selfPersonId && " · you"}
            </span>
          ))}
        </div>
        <div className={styles.coverage}>
          <Users size={13} />
          <span>
            {aa.length} / {s.capacity} assigned
          </span>
          {openSlots > 0 && <b>{openSlots} open</b>}
        </div>
        {data.conflicts.some((c) => c.shiftId === s.id) && (
          <span className={styles.conflictLabel}>
            <AlertCircle size={13} />
            Needs attention
          </span>
        )}
        <div className={styles.cardActions}>
          {mine && week?.state === "published" && data.purchase?.newWork !== false && (
            <Link href={canonicalUiHref(`${base}/overtime/new?shift=${encodeURIComponent(s.id)}`)}>
              Request overtime
            </Link>
          )}
          {data.canManage && view !== "mine" && isFuture && data.purchase?.newWork !== false && (
            <Button appearance={appearance} type="submit" variant="plain" onClick={() => open({ kind: "shift", shift: s })}>
              Edit shift
            </Button>
          )}
          {data.canManage && view !== "mine" && isFuture && data.purchase?.newWork === false && data.purchase.settleExisting && (
            <Button appearance={appearance} type="button" variant="plain" onClick={() => open({ kind: "cancel", shift: s })}>
              Cancel shift
            </Button>
          )}
          {canRequest && !mine && openSlots > 0 && (
            <Button appearance={appearance}
              type="submit"
              variant="plain"
              onClick={() => open({ kind: "request", targetId: s.id })}
            >
              Request slot
            </Button>
          )}
          {canRequest && !mine && aa.length > 0 && own.length > 0 && (
            <Button appearance={appearance}
              type="submit"
              variant="plain"
              onClick={() => open({ kind: "request", targetId: s.id, swap: true })}
            >
              Request swap
            </Button>
          )}
        </div>
      </article>
    );
  }
  const modalTitle = !editor
    ? ""
    : editor.kind === "week"
      ? editor.copy
        ? "Copy roster week"
        : "Create roster week"
      : editor.kind === "shift"
        ? editor.shift
          ? "Edit shift"
          : "Add shift"
        : editor.kind === "template"
          ? editor.template
            ? "Edit shift template"
            : "New shift template"
          : editor.kind === "request"
            ? "Request a roster change"
            : editor.kind === "cancel"
              ? "Cancel shift"
              : editor.kind === "publish"
                ? "Publish roster"
                : `${editor.decision[0].toUpperCase()}${editor.decision.slice(1)} request`;
  return (
    <div data-ui-appearance={appearance === "standard" ? "standard" : undefined} className={`${styles.workspace} ${embedded ? styles.embedded : ""}`} aria-busy={busy} data-roster-owner-pending={pending?"true":undefined} data-roster-owner-dirty={dirty?"true":undefined}>
      {recovery()}
      {editor&&!editorOpen&&<Button appearance={appearance} type="button" variant="secondary" disabled={busy} onClick={()=>setEditorOpen(true)}>Reopen retained Roster form</Button>}
      {(refreshRequired||error&&!pending)&&<Button appearance={appearance} type="button" variant="secondary" disabled={busy||pending} onClick={()=>void navigate(refreshRequired?.department??data.departmentId,refreshRequired?.start??data.weekStart,true)}>Refresh current roster</Button>}
      <section className={styles.toolbar} aria-label="Roster filters">
        <Field appearance={appearance} className={styles.department}>
          Department
          <Select appearance={appearance}
            value={data.departmentId ?? ""}
            disabled={busy || !data.departments.length}
            onChange={(e) => void navigate(e.target.value, data.weekStart)}
          >
            {!data.departments.length && <option value="">No eligible department</option>}
            {data.departments.map((d) => (
              <option key={d.id} value={d.id}>
                {d.branch_name ? `${d.branch_name} · ` : ""}
                {d.name}
              </option>
            ))}
          </Select>
        </Field>
        <div className={styles.weekPicker}>
          <Button appearance={appearance}
            type="submit"
            variant="ghost"
            className={styles.iconButton}
            disabled={busy}
            aria-label="Previous week"
            onClick={() => void navigate(data.departmentId, addDays(data.weekStart, -7))}
          >
            <ChevronLeft size={18} />
          </Button>
          <Field appearance={appearance}>
            Week of
            <TextInput appearance={appearance}
              aria-label="Week of"
              type="date"
              value={data.weekStart}
              disabled={busy}
              onChange={(e) =>
                e.target.value && void navigate(data.departmentId, rosterMonday(e.target.value))
              }
            />
          </Field>
          <Button appearance={appearance}
            type="submit"
            variant="ghost"
            className={styles.iconButton}
            disabled={busy}
            aria-label="Next week"
            onClick={() => void navigate(data.departmentId, addDays(data.weekStart, 7))}
          >
            <ChevronRight size={18} />
          </Button>
          <Button appearance={appearance}
            type="submit"
            variant="secondary"
            className={styles.secondary}
            disabled={busy}
            onClick={() => void navigate(data.departmentId, rosterMonday())}
          >
            Today
          </Button>
        </div>
      </section>
      {error && !editor && (
        <div role="alert" className={styles.error}>
          {error}
          <Button appearance={appearance}
            type="submit"
            variant="plain"
            onClick={() => void navigate(data.departmentId, data.weekStart)}
          >
            Refresh roster
          </Button>
        </div>
      )}
      {message && (
        <p className={styles.success} role="status">
          <Check size={16} />
          {message}
        </p>
      )}
      <section className={styles.weekHeader}>
        <div>
          <div className={styles.eyebrow}>
            <CalendarDays size={15} />
            WEEKLY ROSTER
          </div>
          <h2>
            {dateLabel(data.weekStart)} –{" "}
            {dateLabel(addDays(data.weekStart, 6), {
              month: "short",
              day: "numeric",
              year: "numeric",
            })}
          </h2>
          <p>
            {currentDepartment?.name ?? "Your schedule"} · {week?.timezone ?? "Asia/Bangkok"}
            {week?.state === "draft" && " · Visible to roster managers"}
          </p>
        </div>
        <div className={styles.actions}>
          {week && <StatusBadge appearance={appearance} status={week.state} />}
          {view !== "mine" && data.canManage && data.departmentId && data.purchase?.newWork !== false && (
            <>
              {week ? (
                <>
                  <Button appearance={appearance}
                    type="submit"
                    variant="secondary"
                    className={styles.secondary}
                    onClick={() => open({ kind: "week", copy: true })}
                    disabled={busy}
                  >
                    <Copy size={15} />
                    Copy week
                  </Button>
                  <Button appearance={appearance}
                    type="submit"
                    variant="primary"
                    className={styles.primary}
                    onClick={() => open({ kind: "shift" })}
                    disabled={busy}
                  >
                    <Plus size={16} />
                    Add shift
                  </Button>
                  {week.state === "draft" && (
                    <Button appearance={appearance}
                      type="submit"
                      variant="primary"
                      className={styles.primary}
                      onClick={() => open({ kind: "publish" })}
                      disabled={busy || !shifts.length || !!data.conflicts.length}
                    >
                      <Send size={15} />
                      Publish week
                    </Button>
                  )}
                </>
              ) : (
                <Button appearance={appearance}
                  type="submit"
                  variant="primary"
                  className={styles.primary}
                  onClick={() => open({ kind: "week" })}
                  disabled={busy}
                >
                  <Plus size={16} />
                  Create week
                </Button>
              )}
            </>
          )}
          {data.selfPersonId && data.purchase?.newWork !== false && (
            <Button appearance={appearance}
              type="submit"
              variant="secondary"
              className={styles.secondary}
              disabled={
                busy ||
                (!own.length &&
                  !data.shifts.some((s) => s.state === "active" && s.starts_at > data.asOf))
              }
              onClick={() => open({ kind: "request" })}
            >
              <ArrowRightLeft size={15} />
              Request a change
            </Button>
          )}
        </div>
      </section>
      {data.purchase?.newWork === false && (
        <p className={styles.note} role="status">
          {data.purchase.settleExisting
            ? "Roster planning is paused for this package. You can review history and complete existing changes."
            : "This roster is available for history."}
        </p>
      )}
      {view === "mine" && !data.selfPersonId ? (
        <p className={styles.note} role="status">
          HR setup required: link your account to an active Engagement and department in People
          settings to see your personal schedule.
        </p>
      ) : view === "mine" ? (
        <div className={styles.metrics}>
          <div>
            <strong>{assignments.filter((a) => a.person_id === data.selfPersonId).length}</strong>
            <span>My published shifts</span>
          </div>
          <div>
            <strong>
              {shifts
                .filter((s) =>
                  assignments.some((a) => a.shift_id === s.id && a.person_id === data.selfPersonId),
                )
                .reduce((n, s) => n + (s.ends_at - s.starts_at) / 3600000, 0)
                .toLocaleString(undefined, { maximumFractionDigits: 1 })}
            </strong>
            <span>Scheduled hours</span>
          </div>
          <div>
            <strong>
              {pendingRequests.filter((r) => r.requester_person_id === data.selfPersonId).length}
            </strong>
            <span>My pending changes</span>
          </div>
        </div>
      ) : (
        <div className={styles.metrics}>
          <div>
            <strong>{shifts.length}</strong>
            <span>Shifts this week</span>
          </div>
          <div>
            <strong>
              {assignments.length}
              <small> / {capacity}</small>
            </strong>
            <span>Assigned slots</span>
          </div>
          <div>
            <strong>{Math.max(0, capacity - assignments.length)}</strong>
            <span>Open slots</span>
          </div>
          <div>
            <strong>{pendingRequests.length}</strong>
            <span>Pending requests</span>
          </div>
        </div>
      )}

      {data.conflicts.length > 0 && (
        <section className={styles.warning} aria-label="Roster conflicts">
          <strong>
            <AlertCircle size={16} />
            Coverage needs attention
          </strong>
          {data.conflicts.map((c, i) => (
            <p key={`${c.shiftId}-${i}`}>
              {shiftLabel(c.shiftId)} — {c.message}
            </p>
          ))}
        </section>
      )}
      {data.canManage && data.coverageGaps.length > 0 && (
        <section className={styles.warning} aria-label="Weekly roster gaps">
          <strong>
            <AlertCircle size={16} />
            Staff needing a weekly shift
          </strong>
          {data.coverageGaps.map((g) => (
            <p key={`${g.week_start}-${g.engagement_id}`}>
              Week of {g.week_start}: {g.legal_name}
            </p>
          ))}
        </section>
      )}
      {(view === "team" || view === "mine") &&
        (view === "mine" && !data.selfPersonId ? (
          <div className={styles.empty}>
            <Users size={30} />
            <h3>Your personal schedule needs staff setup</h3>
            <p>
              Ask HR to link your account to an active Engagement and department in People settings.
            </p>
          </div>
        ) : !data.departmentId ? (
          <div className={styles.empty}>
            <Users size={30} />
            <h3>No roster department available</h3>
            <p>
              An active staff profile and department placement are required to see your schedule.
            </p>
          </div>
        ) : !week ? (
          <div className={styles.empty}>
            <CalendarDays size={30} />
            <h3>
              {data.canManage && view !== "mine"
                ? "A fresh week, ready to plan"
                : "Your roster is not published yet"}
            </h3>
            <p>
              {data.canManage && view !== "mine"
                ? "Create a week, add shift times, and assign your team before publishing."
                : "Your manager will publish the schedule here when it is ready."}
            </p>
          </div>
        ) : (
          <>
            {view === "mine" && !assignments.some((a) => a.person_id === data.selfPersonId) && (
              <p className={styles.note}>
                You have no assigned shifts this week. Open Team roster to see available slots.
              </p>
            )}
            <div
              className={styles.calendar}
              aria-label={view === "mine" ? "My weekly shifts" : "Team weekly roster"}
            >
              {Array.from({ length: 7 }, (_, i) => addDays(data.weekStart, i)).map((date) => {
                const daily = shifts.filter(
                  (s) =>
                    s.local_date === date &&
                    (view === "team" ||
                      assignments.some(
                        (a) => a.shift_id === s.id && a.person_id === data.selfPersonId,
                      )),
                );
                const events = calendarItems.filter((item) =>
                  itemOnDate(item, date, week.timezone),
                );
                return (
                  <section key={date} className={styles.day}>
                    <header>
                      <span>{dateLabel(date, { weekday: "short" })}</span>
                      <strong>{dateLabel(date, { day: "numeric" })}</strong>
                      {data.canManage && view === "team" && data.purchase?.newWork !== false && (
                        <Button appearance={appearance}
                          type="submit"
                          variant="plain"
                          aria-label={`Add shift on ${date}`}
                          onClick={() => open({ kind: "shift", date })}
                        >
                          <Plus size={16} />
                        </Button>
                      )}
                    </header>
                    <div className={styles.dayBody}>
                      {daily.map(shiftCard)}
                      {events.map((item) => (
                        <article
                          className={`${styles.calendarContext} ${item.kind === "holiday" ? styles.holiday : item.kind === "leave" ? styles.absence : styles.appointment}`}
                          key={item.id}
                        >
                          <small>
                            {item.kind === "holiday"
                              ? "Holiday"
                              : item.kind === "leave"
                                ? "Leave"
                                : "Appointment"}{" "}
                            · {eventTime(item, week.timezone)}
                          </small>
                          <h3>{item.title}</h3>
                          {item.description && <p>{item.description}</p>}
                          {item.participants.length > 0 && (
                            <p>{item.participants.map((person) => person.name).join(", ")}</p>
                          )}
                        </article>
                      ))}
                      {!daily.length && !events.length && (
                        <p className={styles.off}>No scheduled work</p>
                      )}
                    </div>
                  </section>
                );
              })}
            </div>
            <p className={styles.note}>
              An extra shift does not authorize overtime. Use “Request overtime” on your shift to
              start a separate request.
            </p>
            <p className={styles.note}>
              Times are shown in {week.timezone}. “+1 day” marks shifts ending the next day.
            </p>
          </>
        ))}
      {view === "requests" && (
        <section className={styles.requests} aria-label="Roster requests">
          {!data.requests.length && (
            <div className={styles.empty}>
              <ArrowRightLeft size={30} />
              <h3>No requests yet</h3>
              <p>Request an open slot, move your shift, or arrange a swap with a coworker.</p>
            </div>
          )}
          {data.requests.map((r) => {
            const source = data.assignments.find((a) => a.id === r.source_assignment_id);
            return (
              <article
                key={r.id}
                id={`request-${r.id}`}
                className={`${styles.requestCard} ${r.id === requestId ? styles.highlight : ""}`}
              >
                <div className={styles.requestHead}>
                  <div>
                    <span className={styles.eyebrow}>{requestLabels[r.kind]}</span>
                    <h3>
                      {r.requester_name}
                      {r.coworker_name && (
                        <>
                          <ArrowRightLeft size={15} />
                          {r.coworker_name}
                        </>
                      )}
                    </h3>
                  </div>
                  <StatusBadge appearance={appearance} status={stateLabels[r.state]} />
                </div>
                <div className={styles.requestDetails}>
                  {source && (
                    <p>
                      <span>From</span>
                      {shiftLabel(source.shift_id)}
                    </p>
                  )}
                  <p>
                    <span>{r.kind === "extra" ? "Shift" : "To"}</span>
                    {shiftLabel(r.target_shift_id)}
                  </p>
                </div>
                <p className={styles.reason}>{r.reason}</p>
                {r.decision_note && <p className={styles.note}>{r.decision_note}</p>}
                <div className={styles.actions}>
                  {r.permittedActions.map((action) => (
                    <Button appearance={appearance}
                      type="submit"
                      variant="secondary"
                      key={action}
                      className={
                        action === "approve" || action === "accept"
                          ? styles.primary
                          : styles.secondary
                      }
                      disabled={busy}
                      onClick={() => open({ kind: "decision", request: r, decision: action })}
                    >
                      {action === "accept"
                        ? "Accept swap"
                        : action === "approve"
                          ? "Approve change"
                          : action === "reject"
                            ? "Reject"
                            : "Withdraw"}
                    </Button>
                  ))}
                </div>
              </article>
            );
          })}
        </section>
      )}
      {view === "templates" && data.canManage && (
        <section>
          <div className={styles.sectionHead}>
            <div>
              <h3>Reusable shift times</h3>
              <p>Templates speed up planning. Published shifts keep their original times.</p>
            </div>
            {data.purchase?.newWork !== false && <Button appearance={appearance}
              type="submit"
              variant="primary"
              className={styles.primary}
              disabled={!data.departmentId}
              onClick={() => open({ kind: "template" })}
            >
              <Plus size={16} />
              New template
            </Button>}
          </div>
          <div className={styles.templateGrid}>
            {data.templates.map((t) => (
              <article key={t.id} className={styles.template}>
                <Clock3 size={22} />
                <h3>{t.name}</h3>
                <p>
                  {t.start_time}–{t.end_time}
                  {t.end_time < t.start_time && " (+1 day)"}
                </p>
                <p>
                  {t.capacity} staff · {t.active ? "Active" : "Archived"}
                </p>
                {data.purchase?.newWork !== false && <Button appearance={appearance}
                  type="submit"
                  variant="secondary"
                  className={styles.secondary}
                  onClick={() => open({ kind: "template", template: t })}
                >
                  Edit template
                </Button>}
              </article>
            ))}
          </div>
          {!data.templates.length && (
            <p className={styles.note}>
              Create morning, afternoon, or overnight shift templates for this department.
            </p>
          )}
        </section>
      )}
      {data.history.length > 0 && (
        <details className={styles.history}>
          <summary>Change history</summary>
          {data.history.map((h) => (
            <div key={h.id}>
              <strong>{h.action.replaceAll(".", " ")}</strong>
              <span>
                {h.actor_name} · <TimeText value={h.created_at} />
              </span>
              {h.reason && <p>{h.reason}</p>}
            </div>
          ))}
        </details>
      )}
      {editor && (
        <DraftFormDrawer appearance={appearance} title={modalTitle} open={editorOpen} retainFormContents dirty={dirty} dismissDisabled={busy||pending}
          onClose={() => {if(!busy&&!commands.hasPending())activateEditor(null);}} onDiscard={() => {if(!busy&&!commands.hasPending())activateEditor(null);}}
          confirmationDescription="Your draft has unsaved changes.">
          <div ref={editorRoot} data-work-dirty={dirty ? "true" : undefined} onInputCapture={changedEditor} onChangeCapture={changedEditor}>
          {error && (
            <p className={styles.error} role="alert">
              {error}
            </p>
          )}
          {message && <p className={styles.note} role="status">{message}</p>}
          {recovery()}
          {pending&&<Button appearance={appearance} type="button" variant="secondary" disabled={busy} onClick={()=>{if(!busy)setEditorOpen(false);}}>Close and retain request</Button>}
          {(refreshRequired||error&&!pending)&&<Button appearance={appearance} type="button" variant="secondary" disabled={busy||pending} onClick={()=>void navigate(refreshRequired?.department??data.departmentId,refreshRequired?.start??data.weekStart,true)}>Refresh current roster</Button>}
          {completedEditor && <p className={styles.note} role="status">This change was saved. Close this editor to review the latest roster.</p>}
          {editor.kind === "week" && (
            <form
              className={styles.form}
              onSubmit={(e) => {
                const f = form(e),
                  date = field(f, "week");
                void send(
                  {
                    action: "week.create",
                    departmentId: data.departmentId,
                    weekStart: date,
                    timezone: field(f, "timezone"),
                    ...(field(f, "copy") ? { copyWeekId: field(f, "copy") } : {}),
                  },
                  date,
                );
              }}
            >
              <Field appearance={appearance}>
                Week starting Monday
                <TextInput appearance={appearance}
                  type="date"
                  name="week"
                  required
                  defaultValue={editor.copy ? addDays(data.weekStart, 7) : data.weekStart}
                />
              </Field>
              <Field appearance={appearance}>
                Timezone
                <TextInput appearance={appearance}
                  name="timezone"
                  required
                  defaultValue={week?.timezone ?? "Asia/Bangkok"}
                  list="roster-timezones"
                />
                <datalist id="roster-timezones">
                  <option>Asia/Bangkok</option>
                  <option>America/New_York</option>
                  <option>UTC</option>
                </datalist>
              </Field>
              <Field appearance={appearance}>
                Copy from
                <Select appearance={appearance} name="copy" defaultValue={editor.copy ? week?.id : ""}>
                  <option value="">Start empty</option>
                  {data.weeks.map((w) => (
                    <option key={w.id} value={w.id}>
                      {w.week_start} · {w.state}
                    </option>
                  ))}
                </Select>
              </Field>
              <p className={styles.note}>
                Assignments are checked again for the new dates. Requests and decisions are not
                copied.
              </p>
              <Button appearance={appearance} clientManaged variant="primary" className={styles.primary} disabled={busy || completedEditor} type="submit">
                {editor.copy ? "Copy into draft" : "Create draft"}
              </Button>
            </form>
          )}
          {editor.kind === "publish" && (
            <form
              className={styles.form}
              onSubmit={(e) => {
                e.preventDefault();
                void send({ action: "week.publish", id: week!.id, expectedVersion: week!.version });
              }}
            >
              <p>
                Publish {shifts.length} shifts with {assignments.length} assignments and{" "}
                {capacity - assignments.length} open slots. Your department will be notified.
              </p>
              <p className={styles.note}>
                Future changes need a reason and will notify affected staff.
              </p>
              <Button appearance={appearance} clientManaged variant="primary" type="submit" className={styles.primary} disabled={busy || completedEditor}>
                Publish roster
              </Button>
            </form>
          )}
          {editor.kind === "shift" && week && (
            <RosterShiftForm appearance={appearance}
              key={editorKey}
              editor={editor}
              data={data}
              busy={busy || completedEditor}
              submit={send}
              cancelShift={(s) => open({ kind: "cancel", shift: s })}
            />
          )}
          {editor.kind === "template" && (
            <form
              className={styles.form}
              onSubmit={(e) => {
                const f = form(e);
                void send({
                  action: "template.save",
                  departmentId: data.departmentId,
                  ...(editor.template
                    ? { id: editor.template.id, expectedVersion: editor.template.version }
                    : {}),
                  name: field(f, "name"),
                  startTime: field(f, "start"),
                  endTime: field(f, "end"),
                  capacity: Number(f.get("capacity")),
                  active: f.get("active") === "on",
                });
              }}
            >
              <Field appearance={appearance}>
                Template name
                <TextInput appearance={appearance}
                  name="name"
                  maxLength={80}
                  required
                  defaultValue={editor.template?.name}
                  placeholder="Morning"
                />
              </Field>
              <div className={styles.twoFields}>
                <Field appearance={appearance}>
                  Start time
                  <TextInput appearance={appearance}
                    type="time"
                    name="start"
                    required
                    defaultValue={editor.template?.start_time ?? "06:00"}
                  />
                </Field>
                <Field appearance={appearance}>
                  End time
                  <TextInput appearance={appearance}
                    type="time"
                    name="end"
                    required
                    defaultValue={editor.template?.end_time ?? "14:00"}
                  />
                </Field>
              </div>
              <Field appearance={appearance}>
                Required staff
                <TextInput appearance={appearance}
                  type="number"
                  name="capacity"
                  min="1"
                  max="100"
                  required
                  defaultValue={editor.template?.capacity ?? 2}
                />
              </Field>
              <Field appearance={appearance} orientation="inline" className={styles.check}>
                <TextInput appearance={appearance}
                  type="checkbox"
                  name="active"
                  defaultChecked={editor.template?.active ?? true}
                />
                Active template
              </Field>
              <p className={styles.note}>
                An end time earlier than the start time means the next day.
              </p>
              <Button appearance={appearance} clientManaged type="submit" variant="primary" className={styles.primary} disabled={busy || completedEditor}>
                Save template
              </Button>
            </form>
          )}
          {editor.kind === "request" && (
            <RequestForm appearance={appearance}
              data={data}
              targetId={editor.targetId}
              swap={editor.swap}
              busy={busy || completedEditor}
              submit={send}
            />
          )}
          {editor.kind === "decision" && (
            <form
              className={styles.form}
              onSubmit={(e) => {
                const f = form(e);
                void send({
                  action: "request.decide",
                  id: editor.request.id,
                  expectedVersion: editor.request.version,
                  decision: editor.decision,
                  reason: field(f, "reason"),
                });
              }}
            >
              <p>
                {editor.request.requester_name} · {requestLabels[editor.request.kind]}
                <br />
                {shiftLabel(editor.request.target_shift_id)}
              </p>
              {editor.decision === "accept" && (
                <p>Your assignments stay as they are until an independent manager approves.</p>
              )}
              <Field appearance={appearance}>
                {editor.decision === "reject" ? "Reason for rejection" : "Note (optional)"}
                <TextArea appearance={appearance}
                  name="reason"
                  maxLength={1000}
                  required={editor.decision === "reject"}
                  minLength={editor.decision === "reject" ? 3 : undefined}
                />
              </Field>
              <Button appearance={appearance} clientManaged variant="primary" className={styles.primary} type="submit" disabled={busy || completedEditor}>
                Confirm {editor.decision}
              </Button>
            </form>
          )}
          {editor.kind === "cancel" && (
            <form
              className={styles.form}
              onSubmit={(e) => {
                const f = form(e);
                void send({
                  action: "shift.cancel",
                  id: editor.shift.id,
                  expectedVersion: editor.shift.version,
                  reason: field(f, "reason"),
                });
              }}
            >
              <p>
                {shiftLabel(editor.shift.id)}
                <br />
                Assigned staff will be notified if this roster is published.
              </p>
              <Field appearance={appearance}>
                Reason
                <TextArea appearance={appearance} name="reason" required minLength={3} maxLength={1000} />
              </Field>
              <Button appearance={appearance} clientManaged type="submit" variant="danger" className={styles.danger} disabled={busy || completedEditor}>
                Cancel this shift
              </Button>
            </form>
          )}
          </div>
        </DraftFormDrawer>
      )}
      <Dialog appearance={appearance} open={replacement !== null} title="Discard unsaved roster changes?" onClose={() => setReplacement(null)}>
        <p>Your draft has unsaved changes.</p><div className={styles.actions}>
          <Button appearance={appearance} type="button" variant="secondary" onClick={() => setReplacement(null)}>Keep editing</Button>
          <Button appearance={appearance} type="button" variant="danger" disabled={busy||pending} onClick={() => {if(replacement&&!busy&&!commands.hasPending())activateEditor(replacement);}}>Discard changes</Button>
        </div>
      </Dialog>
    </div>
  );
}

function RequestForm({
  appearance,
  data,
  targetId,
  swap,
  busy,
  submit,
}: {
  appearance: UiAppearance;
  data: RosterData;
  targetId?: string;
  swap?: boolean;
  busy: boolean;
  submit: (input: unknown) => Promise<void>;
}) {
  const styles = appearance === "standard" ? standardStyles : legacyStyles;
  const [kind, setKind] = useState(swap ? "swap" : "extra"),
    [target, setTarget] = useState(targetId ?? "");
  const available = data.shifts.filter(
    (s) =>
      s.state === "active" &&
      s.starts_at > data.asOf &&
      data.weeks.some((w) => w.id === s.week_id && w.state === "published"),
  );
  const own = data.assignments.filter(
    (a) =>
      a.person_id === data.selfPersonId &&
      a.state === "assigned" &&
      available.some((s) => s.id === a.shift_id),
  );
  const label = (s: RosterShift) =>
    `${dateLabel(s.local_date)} · ${s.name} · ${s.start_time}–${s.end_time}${s.end_time < s.start_time ? " (+1 day)" : ""}`;
  const targets = available.filter(
    (s) =>
      !own.some((a) => a.shift_id === s.id) &&
      (kind === "swap"
        ? data.assignments.some((a) => a.shift_id === s.id && a.state === "assigned")
        : data.assignments.filter((a) => a.shift_id === s.id && a.state === "assigned").length <
          s.capacity),
  );
  return (
    <form
      className={styles.form}
      onSubmit={(e) => {
        const f = form(e);
        void submit({
          action: "request.create",
          kind,
          ...(kind !== "extra" ? { sourceAssignmentId: field(f, "source") } : {}),
          targetShiftId: target,
          ...(kind === "swap" ? { targetAssignmentId: field(f, "coworker") } : {}),
          reason: field(f, "reason"),
        });
      }}
    >
      <Field appearance={appearance}>
        Request type
        <Select appearance={appearance}
          value={kind}
          onChange={(e) => {
            setKind(e.target.value);
            setTarget("");
          }}
        >
          <option value="extra">Take an extra open slot</option>
          <option value="move" disabled={!own.length}>
            Move my shift into an open slot
          </option>
          <option value="swap" disabled={!own.length}>
            Swap with a coworker
          </option>
        </Select>
      </Field>
      {kind !== "extra" && (
        <Field appearance={appearance}>
          My shift to exchange
          <Select appearance={appearance} name="source" required defaultValue="">
            <option value="">Select your shift</option>
            {own.map((a) => (
              <option key={a.id} value={a.id}>
                {label(available.find((s) => s.id === a.shift_id)!)}
              </option>
            ))}
          </Select>
        </Field>
      )}
      <Field appearance={appearance}>
        Requested shift
        <Select appearance={appearance} value={target} onChange={(e) => setTarget(e.target.value)} required>
          <option value="">Select a shift</option>
          {targets.map((s) => (
            <option key={s.id} value={s.id}>
              {label(s)}
            </option>
          ))}
        </Select>
      </Field>
      {kind === "swap" && (
        <Field appearance={appearance}>
          Coworker to swap with
          <Select appearance={appearance} key={target} name="coworker" required defaultValue="">
            <option value="">Select a coworker</option>
            {data.assignments
              .filter(
                (a) =>
                  a.shift_id === target &&
                  a.person_id !== data.selfPersonId &&
                  a.state === "assigned",
              )
              .map((a) => (
                <option key={a.id} value={a.id}>
                  {a.person_name}
                </option>
              ))}
          </Select>
        </Field>
      )}
      <Field appearance={appearance}>
        Reason
        <TextArea appearance={appearance}
          name="reason"
          required
          minLength={3}
          maxLength={1000}
          placeholder="Tell your manager why you need this change."
        />
      </Field>
      <p className={styles.note}>
        {kind === "swap"
          ? "Your coworker accepts first, then a manager approves. Both assignments stay unchanged until final approval."
          : "Your manager must approve before the roster changes."}
      </p>
      {!targets.length && (
        <p className={styles.warning}>No eligible shifts are available in the surrounding weeks.</p>
      )}
      <Button appearance={appearance}
        clientManaged type="submit"
        variant="primary"
        className={styles.primary}
        disabled={busy || !target || !targets.some((s) => s.id === target)}
      >
        Send request
      </Button>
    </form>
  );
}

export function RosterOwnerWorkspace(props: Parameters<typeof RosterOwnerView>[0]) {
  const appearance=props.appearance??"legacy",[drafts,setDrafts]=useState<Record<string,RosterOwnerDraftState>>({});
  if(props.embedded)return <RosterOwnerView {...props}/>;
  const views:[string,string,boolean][]=[["mine","My shifts",props.initial.selfRosterRequired],["team","Team roster",true],["requests","Requests",true],["templates","Shift templates",props.initial.canManage]];
  return <WidgetGrid pageKey="people-roster">{views.filter(([view,,available])=>available||rosterOwnerDraftHeld(drafts[view])).map(([view,title])=><FeatureWidget appearance={appearance} key={view} id={`roster-${view}`} title={title}><RosterOwnerView {...props} initialView={view} embedded onDraftStateChange={next=>setDrafts(previous=>{const current=currentRosterOwnerDraft(previous[view],next);return current===previous[view]?previous:{...previous,[view]:current};})}/></FeatureWidget>)}</WidgetGrid>;
}
