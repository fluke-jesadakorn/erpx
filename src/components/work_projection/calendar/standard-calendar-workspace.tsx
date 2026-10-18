"use client";
import { canonicalUiHref } from "@/lib/work_projection/destination-registry";
import { FormDisclosure } from "@/components/ui/form-disclosure";
import { ActionLink as UiActionLink } from "@/components/ui/foundation";

import {
  Button,
  Field,
  Select,
  Table,
  TableFrame,
  TextArea,
  TextInput,
} from "@/components/ui/foundation";

import { RosterOwnerWorkspace as RosterWorkspace } from "@/components/people_payroll/roster-owner-workspace";
import {currentRosterOwnerDraft,rosterOwnerDraftHeld,type RosterOwnerDraftState} from "@/lib/people_payroll/roster-owner-command";
import {captureWorkCommandContext} from "@/lib/work_projection/command-context";
import { Dialog } from "@/components/ui/dialog";
import { PageHeader } from "@/components/ui/primitives";
import {
  calendarLayers,
  calendarQuerySchema,
  itemOnDate,
  today,
  type CalendarData,
  type CalendarHierarchy,
  type CalendarItem,
  type CalendarPerson,
  type CalendarQuery,
} from "@/lib/work_projection/calendar";
import { addDays, rosterMonday, type RosterData } from "@/lib/people_payroll/people-roster";
import { localDateTimeToInstant, localMinuteAt, moveInclusiveRange } from "@/lib/work_projection/calendar-timeline";
import {createCalendarCommands,calendarCommandContext,type CalendarCommandAttempt} from "@/lib/work_projection/calendar-command";
import {CalendarDraftBoundary} from "./calendar-draft-boundary";
import {workFetch,type WorkCommandCompletion} from "@/lib/work_projection/work-fetch";
import { formatUiDateTime, uiLocale } from "@/lib/shared/ui-format";
import {
  CalendarDays,
  AlertCircle,
  ChevronLeft,
  ChevronDown,
  ChevronRight,
  Globe2,
  Plus,
  RefreshCw,
  Search,
  Users,
} from "lucide-react";
import Link from "@/components/workplace/workspace-link";
import {
  useCallback,
  useEffect,
  useLayoutEffect,
  useRef,
  useState,
  type ReactNode,
  type PointerEvent as ReactPointerEvent,
} from "react";
import { CalendarEvent, eventStyle, eventTime, layerNames } from "@/components/work_projection/calendar/calendar-event";
import { StandardCalendarForm as CalendarForm, type CalendarEditor } from "@/components/work_projection/calendar/standard-calendar-forms";
import { CalendarDayTimeline } from "@/components/work_projection/calendar/calendar-day-timeline";
import { PersonalTimeGrid } from "@/components/work_projection/calendar/personal-time-grid";
import { MonthGrid } from "@/components/work_projection/calendar/month-grid";
import { MonthNavigation } from "@/components/work_projection/calendar/month-navigation";
import nativeStyles from "@/components/work_projection/calendar/calendar.module.css";
import standardStyles from "./standard-calendar-workspace.module.css";
// Native grid/event/scroll geometry remains owned by the specialized calendar widgets.
const styles: Record<string, string> = { ...nativeStyles, ...standardStyles, workspace: `${nativeStyles.workspace} ${standardStyles.workspace}` };
const label = (
  date: string,
  options: Intl.DateTimeFormatOptions = { month: "short", day: "numeric" },
) =>
  new Intl.DateTimeFormat(uiLocale, { ...options, timeZone: "UTC" }).format(
    new Date(`${date}T12:00:00Z`),
  );
function queryString(q: CalendarQuery) {
  return new URLSearchParams(
    Object.entries(q)
      .filter(([, v]) => v !== undefined && v !== "")
      .map(([k, v]) => [k, String(v)]),
  ).toString();
}
function mergeItems(a: CalendarItem[], b: CalendarItem[]) {
  return [...new Map([...a, ...b].map((i) => [i.id, i])).values()];
}
function CalendarView({
  base,
  initial,
  initialHierarchy,
  initialEvent,
  initialAccounts,
  contextKey="legacy-calendar",
}: {
  base: string;
  initial: CalendarData;
  initialHierarchy: CalendarHierarchy;
  initialEvent?: string;
  initialAccounts?: {id:string;name:string;email:string}[];
  contextKey?:string;
}) {
  const [data, setData] = useState(initial),
    [hierarchy, setHierarchy] = useState(initialHierarchy),
    [busy, setBusy] = useState(false),
    [error, setError] = useState(""),
    [message, setMessage] = useState("");
  const [accounts, setAccounts] = useState<{ id: string; name: string; email: string }[]>(initialAccounts??[]);
  const [commandBusy,setCommandBusy]=useState(false);
  const [editorDirty,setEditorDirty]=useState(false),[discardEditor,setDiscardEditor]=useState(false),[unresolved,setUnresolved]=useState<CalendarCommandAttempt|null>(null);
  const [rosterVisible,setRosterVisible]=useState(false),[rosterDismiss,setRosterDismiss]=useState(false),[rosterDraft,setRosterDraft]=useState<RosterOwnerDraftState|null>(null);
  const rosterDraftLive=useRef<RosterOwnerDraftState|null>(null),rosterParentLive=useRef<{base:string;contextKey:string}|null>(null);
  const trackRosterDraft=useCallback((next:RosterOwnerDraftState)=>{const current=currentRosterOwnerDraft(rosterDraftLive.current,next);rosterDraftLive.current=current;setRosterDraft(current);},[]);
  useLayoutEffect(()=>{rosterParentLive.current={base,contextKey};return()=>{rosterParentLive.current=null;};},[base,contextKey]);
  const commands=useRef(createCalendarCommands());
  const commandContext=useCallback(()=>JSON.stringify([rosterParentLive.current?.base,rosterParentLive.current?.contextKey,calendarCommandContext()]),[]);
  const closeEditor=()=>{if(editorDirty||commandBusy||unresolved)setDiscardEditor(true);else setEditor(null);};
  const [selected, setSelected] = useState<CalendarItem | null>(
      initial.items.find((i) => i.id === initialEvent || i.sourceId === initialEvent) ?? null,
    ),
    [editor, setEditor] = useState<CalendarEditor | null>(null),
    [roster, setRoster] = useState<RosterData | null>(null),
    [rosterDate, setRosterDate] = useState(initial.query.date),
    [dateDrop, setDateDrop] = useState<string | null>(null),
    [dateDragging, setDateDragging] = useState<string | null>(null),
    [pendingShift, setPendingShift] = useState<{
      item: CalendarItem;
      change: { startsAt: number; endsAt: number };
    } | null>(null);
  const [editScope, setEditScope] = useState<"single" | "future" | "series">("single");
  const qRef = useRef(initial.query),
    requestSeq = useRef(0),activeRead=useRef<number|null>(null),lastCommandContext=useRef<string|null>(null),commandTurn=useRef<object|null>(null),
    api = `/api/v1${base}/calendar`;
  function captureView(){
    const owner=rosterParentLive.current,assertWorkCurrent=captureWorkCommandContext(),sequence=requestSeq.current,query=queryString(qRef.current);
    const ownerCurrent=()=>{try{assertWorkCurrent();return owner!==null&&rosterParentLive.current===owner;}catch{return false;}};
    return {ownerCurrent,assertOwner:()=>{assertWorkCurrent();if(!owner||rosterParentLive.current!==owner)throw Error("The original Calendar source changed.");},projectionCurrent:()=>ownerCurrent()&&requestSeq.current===sequence&&queryString(qRef.current)===query};
  }
  const load = useCallback(
    async (q: CalendarQuery, append = false, writeUrl: false | "push" | "replace" = false) => {
      const seq = ++requestSeq.current,owner=rosterParentLive.current,assertWorkCurrent=captureWorkCommandContext();
      let visibleQuery=queryString(qRef.current);
      const current=()=>{try{assertWorkCurrent();return owner!==null&&rosterParentLive.current===owner&&seq===requestSeq.current&&queryString(qRef.current)===visibleQuery;}catch{return false;}};
      if(!current())return null;activeRead.current=seq;
      setBusy(true);
      setError("");
      try {
        const r = await workFetch(`${api}?${queryString(q)}`, {
            cache: "no-store",
          }),
          j = await r.json();
        assertWorkCurrent();
        if(!owner||rosterParentLive.current!==owner)throw Error("The original Calendar source changed. Refresh in its account and role.");
        if (!r.ok) throw new Error(j.error?.message ?? "Calendar could not be loaded.");
        if (!current()) return null;
        const next = j.data as CalendarData;
        qRef.current = { ...next.query, cursor: 0 };visibleQuery=queryString(qRef.current);
        setData((old) =>
          append
            ? {
                ...next,
                query: qRef.current,
                items: mergeItems(old.items, next.items),
                coverage: {
                  ...next.coverage,
                  shifts: old.coverage.shifts + next.coverage.shifts,
                  assigned: old.coverage.assigned + next.coverage.assigned,
                  open: old.coverage.open + next.coverage.open,
                  warnings: old.coverage.warnings + next.coverage.warnings,
                },
              }
            : next,
        );
        const eventId = new URL(window.location.href).searchParams.get("event");
        setSelected(
          writeUrl === "push"
            ? null
            : (next.items.find((i) => i.id === eventId || i.sourceId === eventId) ?? null),
        );
        if (writeUrl)
          window.history[`${writeUrl}State`](
            null,
            "",
            canonicalUiHref(`${base}/calendar?${queryString(qRef.current)}${writeUrl === "replace" && eventId ? `&event=${encodeURIComponent(eventId)}` : ""}`),
          );
        return ()=>{if(!current())throw Error("The refreshed Calendar view changed. Retry the original action in its owner.");};
      } catch (e) {
        if (current()) setError(e instanceof Error ? e.message : "Calendar could not be loaded.");
        return null;
      } finally {
        if(activeRead.current===seq)activeRead.current=null;
        if (current()) setBusy(false);
      }
    },
    [api, base],
  );
  useEffect(() => {
    if(initialAccounts!==undefined)return;
    void workFetch(`/api/v1${base}/inbox/people`)
      .then((r) => r.json())
      .then((j) => setAccounts(Array.isArray(j.data) ? j.data : []))
      .catch(() => {});
  }, [base,initialAccounts]);
  useEffect(() => {
    const refresh = () => {
      if (document.visibilityState === "visible") void load({ ...qRef.current, cursor: 0 });
    };
    const pop = () => {
      const params = new URL(window.location.href).searchParams;
      const raw = Object.fromEntries(params);
      if (!params.has("view")) raw.view = params.get("preset") === "coverage" ? "week" : "month";
      if (!params.has("layers") && params.get("preset") === "coverage") raw.layers = "roster,leave";
      const parsed = calendarQuerySchema.safeParse(raw);
      if (parsed.success) void load(parsed.data, false, false);
    };
    window.addEventListener("focus", refresh);
    document.addEventListener("visibilitychange", refresh);
    window.addEventListener("popstate", pop);
    return () => {
      window.removeEventListener("focus", refresh);
      document.removeEventListener("visibilitychange", refresh);
      window.removeEventListener("popstate", pop);
    };
  }, [load]);
  useEffect(()=>{
    const recover=()=>{
      const attempt=commands.current.pending();
      if(!attempt||attempt.context!==commandContext()||lastCommandContext.current!==attempt.context)return;
      try{captureWorkCommandContext()();if(!rosterParentLive.current)return;setUnresolved(attempt);if(!commands.current.inflight()&&!commandTurn.current){setCommandBusy(false);if(activeRead.current===null)setBusy(false);}}catch{}
    };
    recover();window.addEventListener("erpx:work-context-changed",recover);window.addEventListener("focus",recover);
    return()=>{window.removeEventListener("erpx:work-context-changed",recover);window.removeEventListener("focus",recover);};
  },[base,contextKey,commandContext]);
  const q = data.query,
    activePreset = q.preset ?? (q.mode === "mine" ? "mine" : "team"),
    days = Array.from(
      {
        length: Math.round((Date.parse(data.to) - Date.parse(data.from)) / 86400000),
      },
      (_, i) => addDays(data.from, i),
    );
  function change(patch: Partial<CalendarQuery>) {
    const query = { ...qRef.current, ...patch, cursor: 0 };
    qRef.current = query;
    void load(query, false, "push");
  }
  function changePreset(preset: "mine" | "team" | "coverage") {
    const query: CalendarQuery = {
      ...qRef.current,
      preset,
      mode: preset === "mine" ? "mine" : "team",
      view: preset === "coverage" ? "week" : "month",
      layers: preset === "coverage" ? "roster,leave" : calendarLayers.join(","),
      person: undefined,
      search: "",
      cursor: 0,
    };
    qRef.current = query;
    void load(query, false, "push");
  }
  function open(item: CalendarItem) {
    setSelected(item);
    setEditScope(item.occurrenceId ? "single" : "series");
    setError("");
    setMessage("");
    const url = new URL(window.location.href);
    url.searchParams.set("event", item.id);
    url.searchParams.set("view", qRef.current.view);
    window.history.pushState(null, "", url);
  }
  function closeDetail() {
    if(commandBusy||commands.current.pending()){setError("Recover the submitted Calendar action before closing its details.");return;}
    setSelected(null);
    const url = new URL(window.location.href);
    url.searchParams.delete("event");
    window.history.replaceState(null, "", url);
  }
  async function send(input: unknown,completion?:WorkCommandCompletion,captured?:CalendarCommandAttempt) {
    if(commands.current.inflight()||commandTurn.current)return;
    const view=captureView(),turn={};if(!view.ownerCurrent())return;commandTurn.current=turn;
    setCommandBusy(true);setBusy(true);setError("");setMessage("");
    try {
      const attempt=captured??commands.current.prepare(input,completion,commandContext());lastCommandContext.current=attempt.context;
      const ack=await commands.current.execute(api,attempt,commandContext,"caller");
      view.assertOwner();if(!view.projectionCurrent())throw Error("The Calendar view changed. Recover the original submitted action.");
      setUnresolved(commands.current.pending());
      const refreshed=await load({...qRef.current,cursor:0});
      if(!refreshed)return;view.assertOwner();refreshed();ack.assertCurrent();ack.acknowledge();const unchanged=ack.unchanged;
      setUnresolved(commands.current.pending());
      if(unchanged){setEditor(null);setEditorDirty(false);if(attempt.completion)window.dispatchEvent(new CustomEvent('erpx:record-command-saved',{detail:attempt.completion}));}
      setMessage(unchanged?(ack.result.warnings.length?`Saved. ${ack.result.warnings.join(" ")}`:"Calendar updated."):"Submitted Calendar change saved. Your newer draft is retained.");
    } catch(e){if(view.ownerCurrent()){setUnresolved(commands.current.pending());if(view.projectionCurrent())setError(e instanceof Error?e.message:"Could not save this change.");}}
    finally{if(commandTurn.current===turn){commandTurn.current=null;if(view.ownerCurrent()){setCommandBusy(false);if(view.projectionCurrent()&&activeRead.current===null)setBusy(false);}}}
  }
  const replaceItem = (replacement: CalendarItem) =>
    setData((current) => ({
      ...current,
      items: current.items.map((item) => (item.id === replacement.id ? replacement : item)),
    }));
  async function sendReschedule(item: CalendarItem, optimistic: CalendarItem, input: unknown) {
    if(commands.current.inflight()||commandTurn.current)return;
    const view=captureView(),turn={};if(!view.ownerCurrent())return;commandTurn.current=turn;
    replaceItem(optimistic);setCommandBusy(true);setBusy(true);setError("");setMessage("");
    try{
      const attempt=commands.current.prepare(input,undefined,commandContext());lastCommandContext.current=attempt.context;
      const ack=await commands.current.execute(api,attempt,commandContext,"caller");view.assertOwner();if(!view.projectionCurrent())throw Error("The Calendar view changed. Recover the original submitted move.");
      setUnresolved(commands.current.pending());const refreshed=await load({...qRef.current,cursor:0});
      if(!refreshed)return;view.assertOwner();refreshed();ack.assertCurrent();ack.acknowledge();setUnresolved(commands.current.pending());
      // The owned fresh read already contains any later native history. Never
      // overwrite it with this attempt's old optimistic version or old item.
      setMessage(ack.result.warnings.length?`Saved. ${ack.result.warnings.join(" ")}`:"Calendar updated.");
    }catch(cause){if(view.ownerCurrent()){setUnresolved(commands.current.pending());if(view.projectionCurrent()){replaceItem(item);setError(cause instanceof Error?cause.message:"Could not reschedule this item.");}}}
    finally{if(commandTurn.current===turn){commandTurn.current=null;if(view.ownerCurrent()){setCommandBusy(false);if(view.projectionCurrent()&&activeRead.current===null)setBusy(false);}}}
  }
  function rescheduleTimed(
    item: CalendarItem,
    change: { startsAt: number; endsAt: number },
    reason = "",
  ) {
    setError("");
    const optimistic = { ...item, startsAt: change.startsAt, endsAt: change.endsAt };
    if (item.kind === "shift" && item.details?.requiresReason && !reason) {
      replaceItem(optimistic);
      setPendingShift({ item, change });
      return;
    }
    if (item.kind === "appointment") {
      const zone = item.allDay ? item.timezone : q.timezone;
      const start = localMinuteAt(change.startsAt, zone),
        end = localMinuteAt(change.endsAt, zone);
      void sendReschedule(item, optimistic, {
        action: "appointment.reschedule",
        id: item.sourceId,
        expectedVersion: item.version,
        startLocal: `${start.date}T${start.time}`,
        endLocal: `${end.date}T${end.time}`,
        timezone: zone,
        ...(item.occurrenceId ? { occurrenceId: item.occurrenceId, scope: editScope } : {}),
      });
    } else if (item.kind === "shift") {
      const start = localMinuteAt(change.startsAt, item.timezone),
        end = localMinuteAt(change.endsAt, item.timezone);
      void sendReschedule(item, optimistic, {
        action: "shift.reschedule",
        id: item.sourceId,
        expectedVersion: item.version,
        date: start.date,
        startTime: start.time,
        endTime: end.time,
        reason,
      });
    }
  }
  function rescheduleDate(item: CalendarItem, target: string, edge?: "start" | "end") {
    if (item.kind === "task" && item.permittedActions.includes("task.reschedule")) {
      void sendReschedule(
        item,
        { ...item, startDate: target, endDate: addDays(target, 1) },
        {
          action: "task.reschedule",
          id: item.sourceId,
          expectedVersion: item.version,
          dueDate: target,
        },
      );
      return;
    }
    if (
      item.kind === "announcement" &&
      item.startDate &&
      item.endDate &&
      item.permittedActions.includes("announcement.reschedule")
    ) {
      const inclusiveEnd = addDays(item.endDate, -1);
      const range =
        edge === "start"
          ? { startDate: target <= inclusiveEnd ? target : inclusiveEnd, endDate: inclusiveEnd }
          : edge === "end"
            ? {
                startDate: item.startDate,
                endDate: target >= item.startDate ? target : item.startDate,
              }
            : moveInclusiveRange(item.startDate, inclusiveEnd, target);
      void sendReschedule(
        item,
        { ...item, startDate: range.startDate, endDate: addDays(range.endDate, 1) },
        {
          action: "announcement.reschedule",
          id: item.sourceId,
          expectedVersion: item.version,
          ...range,
        },
      );
      return;
    }
    if (
      (item.kind === "appointment" || item.kind === "shift") &&
      item.startsAt !== null &&
      item.endsAt !== null &&
      item.permittedActions.includes(`${item.kind}.reschedule`)
    ) {
      const displayed = localMinuteAt(item.startsAt, q.timezone);
      const startsAt = localDateTimeToInstant(`${target}T${displayed.time}`, q.timezone);
      rescheduleTimed(item, { startsAt, endsAt: startsAt + (item.endsAt - item.startsAt) });
    }
  }
  function beginDateDrag(item: CalendarItem, event: ReactPointerEvent, edge?: "start" | "end") {
    const movable = item.permittedActions.includes(`${item.kind}.reschedule`);
    if (!movable) return;
    event.stopPropagation();
    const originX = event.clientX,
      originY = event.clientY;
    let moved = false,
      target: string | null = null;
    const onMove = (pointer: PointerEvent) => {
      moved ||= Math.hypot(pointer.clientX - originX, pointer.clientY - originY) >= 5;
      if (!moved) return;
      setDateDragging(item.id);
      pointer.preventDefault();
      target =
        (
          document
            .elementFromPoint(pointer.clientX, pointer.clientY)
            ?.closest("[data-calendar-date]") as HTMLElement | null
        )?.dataset.calendarDate ?? null;
      setDateDrop(target);
    };
    const finish = () => {
      window.removeEventListener("pointermove", onMove);
      window.removeEventListener("pointerup", finish);
      window.removeEventListener("pointercancel", cancel);
      window.removeEventListener("keydown", escape);
      setDateDrop(null);
      setDateDragging(null);
      if (moved && target) rescheduleDate(item, target, edge);
    };
    const cancel = () => {
      window.removeEventListener("pointermove", onMove);
      window.removeEventListener("pointerup", finish);
      window.removeEventListener("pointercancel", cancel);
      window.removeEventListener("keydown", escape);
      setDateDrop(null);
      setDateDragging(null);
    };
    window.addEventListener("pointermove", onMove);
    window.addEventListener("pointerup", finish, { once: true });
    window.addEventListener("pointercancel", cancel, { once: true });
    const escape = (keyboard: KeyboardEvent) => {
      if (keyboard.key === "Escape") cancel();
    };
    window.addEventListener("keydown", escape);
  }
  async function openRoster(item?: CalendarItem) {
    if(rosterOwnerDraftHeld(rosterDraftLive.current)){setRosterVisible(true);setError("Finish or recover the retained Roster draft before opening a different owner.");return;}
    setError("");
    setBusy(true);
    try {
      const assertCurrent=captureWorkCommandContext(),parent=rosterParentLive.current,queryStamp=JSON.stringify(qRef.current);
      const department =
        item?.departmentId ??
        hierarchy.people.find((p) => p.id === q.person)?.departmentId ??
        (q.node
          ? hierarchy.nodes.find((n) => n.kind === "department" && withinNode(n.id, q.node))?.id
          : undefined);
      const params = new URLSearchParams({
        week: rosterMonday(item?.startDate ?? q.date),
        ...(department ? { department } : {}),
      });
      const r = await workFetch(`/api/v1${base}/people/roster?${params}`, {
          cache: "no-store",
        }),
        j = await r.json();
      assertCurrent();if(!parent||rosterParentLive.current!==parent||JSON.stringify(qRef.current)!==queryStamp)throw Error("The Calendar owner changed before Roster loaded.");
      if(rosterOwnerDraftHeld(rosterDraftLive.current)){setRosterVisible(true);setError("The retained Roster draft takes precedence over a late read.");return;}
      if (!r.ok) throw new Error(j.error?.message ?? "Could not load roster planning.");
      setRosterDate(item?.startDate ?? q.date);
      closeDetail();
      rosterDraftLive.current=null;setRosterDraft(null);setRosterVisible(true);setRoster(j.data);
    } catch (e) {
      setError(e instanceof Error ? e.message : "Could not load the roster.");
    } finally {
      setBusy(false);
    }
  }
  async function morePeople() {
    setBusy(true);
    try {
      const r = await workFetch(`${api}/hierarchy?cursor=${hierarchy.nextCursor}`),
        j = await r.json();
      if (!r.ok) throw new Error(j.error?.message ?? "People could not be loaded.");
      setHierarchy((old) => ({
        ...j.data,
        people: [
          ...new Map(
            [...old.people, ...j.data.people].map((p: CalendarPerson) => [p.id, p]),
          ).values(),
        ],
      }));
    } catch (e) {
      setError(e instanceof Error ? e.message : "People could not be loaded.");
    } finally {
      setBusy(false);
    }
  }
  const personButton = (p: CalendarPerson) => (
    <Button appearance="standard"
      type="submit"
      variant="plain"
      key={p.id}
      className={styles.person}
      aria-pressed={q.person === p.id}
      onClick={() => change({ person: q.person === p.id ? undefined : p.id, mode: "team" })}
    >
      <span className={styles.avatar}>
        {p.name
          .split(" ")
          .map((x) => x[0])
          .slice(0, 2)
          .join("")}
      </span>
      <span>
        {p.name}
        {p.id === data.selfPersonId && " · You"}
      </span>
    </Button>
  );
  function orgTree(parent: string | null, seen = new Set<string>()): ReactNode {
    return hierarchy.nodes
      .filter(
        (n) =>
          n.parentId === parent ||
          (parent === null && !hierarchy.nodes.some((x) => x.id === n.parentId)),
      )
      .filter((n) => !seen.has(n.id))
      .map((n) => {
        const next = new Set([...seen, n.id]),
          people = hierarchy.people.filter(
            (p) =>
              (p.departmentId ??
                p.branchId ??
                hierarchy.nodes.find((x) => x.kind === "company")?.id) === n.id,
          );
        return (
          <details key={n.id}>
            <summary>{n.name}</summary>
            {n.inScope !== false && (
              <Button appearance="standard"
                type="submit"
                variant="plain"
                className={styles.person}
                aria-pressed={q.node === n.id}
                onClick={() =>
                  change({
                    node: q.node === n.id ? undefined : n.id,
                    person: undefined,
                    mode: "team",
                  })
                }
              >
                View {n.name}
              </Button>
            )}
            <div className={styles.treeChildren}>
              {people.map(personButton)}
              {orgTree(n.id, next)}
            </div>
          </details>
        );
      });
  }
  function reportTree(manager: string | null, seen = new Set<string>()): ReactNode {
    return hierarchy.people
      .filter((p) =>
        manager === null
          ? !p.managerId || !hierarchy.people.some((x) => x.id === p.managerId)
          : p.managerId === manager,
      )
      .filter((p) => !seen.has(p.id))
      .map((p) => (
        <div key={p.id}>
          {personButton(p)}
          <div className={styles.treeChildren}>{reportTree(p.id, new Set([...seen, p.id]))}</div>
        </div>
      ));
  }
  function withinNode(id: string | null, node: string | undefined): boolean {
    if (!node) return true;
    const seen = new Set<string>();
    while (id && !seen.has(id)) {
      if (id === node) return true;
      seen.add(id);
      id = hierarchy.nodes.find((n) => n.id === id)?.parentId ?? null;
    }
    return false;
  }
  const eventPeople: CalendarPerson[] = data.items.flatMap((item) =>
    item.participants.map((p) => ({
      id: p.id,
      name: p.name,
      userId: null,
      departmentId: item.departmentId,
      branchId: item.branchId,
      managerId: null,
      status: "event",
    })),
  );
  const people = [
    ...new Map([...eventPeople, ...hierarchy.people].map((p) => [p.id, p])).values(),
  ].filter(
    (p) =>
      (!q.person || p.id === q.person) &&
      (q.mode !== "mine" || p.id === data.selfPersonId) &&
      (q.preset === "coverage" || eventPeople.some((event) => event.id === p.id)) &&
      (!q.node || withinNode(p.departmentId, q.node) || withinNode(p.branchId, q.node)) &&
      (!q.search ||
        p.name.toLocaleLowerCase().includes(q.search.toLocaleLowerCase()) ||
        data.items.some(
          (i) =>
            i.participants.some((x) => x.id === p.id) ||
            (i.kind === "task" && !!p.userId && i.details?.assigneeId === p.userId),
        )),
  );
  const event = (item: CalendarItem, showPeople = false) => (
    <CalendarEvent
      key={item.id}
      item={item}
      zone={q.timezone}
      onOpen={open}
      showPeople={showPeople}
      onPointerDown={
        q.view === "week" || q.view === "month"
          ? (pointer) => beginDateDrag(item, pointer)
          : undefined
      }
      resizeHandles={
        (q.view === "week" || q.view === "month") &&
        item.kind === "announcement" &&
        item.permittedActions.includes("announcement.reschedule")
      }
      onResizePointerDown={(edge, pointer) => beginDateDrag(item, pointer, edge)}
      dragging={dateDragging === item.id}
      compact={q.view === "week" || q.view === "month"}
    />
  );
  const itemsForDate = (date: string) =>
    data.items
      .filter((i) => itemOnDate(i, date, q.timezone))
      .sort((a, b) => (a.startsAt ?? 0) - (b.startsAt ?? 0) || a.title.localeCompare(b.title));
  const byDate = new Map(days.map((day) => [day, itemsForDate(day)]));
  const daily = (date: string) => byDate.get(date) ?? [];
  const attentionDateKey = (item: CalendarItem) =>
    item.startDate ??
    (item.startsAt === null ? "\uffff" : localMinuteAt(item.startsAt, q.timezone).date);
  const attention = data.items
      .filter((i) => i.warnings.length)
      .sort((a, b) => {
        const aDate = attentionDateKey(a),
          bDate = attentionDateKey(b);
        if (aDate !== bDate) return aDate.localeCompare(bDate);
        return (
          (a.startsAt ?? 0) - (b.startsAt ?? 0) ||
          a.title.localeCompare(b.title) ||
          a.id.localeCompare(b.id)
        );
      }),
    unscheduled = data.items.filter((i) => !i.startDate && i.startsAt === null);
  const move = (direction: number) => {
    if (q.view === "month") {
      const d = new Date(`${q.date.slice(0, 7)}-01T12:00:00Z`);
      d.setUTCMonth(d.getUTCMonth() + direction);
      change({ date: d.toISOString().slice(0, 10) });
    } else
      change({
        date: addDays(q.date, direction * (q.view === "week" ? 7 : q.view === "day" ? 1 : 30)),
      });
  };
  const failedSources = data.sources.filter((source) => source.status === "error"),
    hasMoreResults = data.nextCursor !== null,
    attentionIsPartial = hasMoreResults || failedSources.length > 0,
    filterSummary = [
      `${q.layers.split(",").length} ${q.layers.split(",").length === 1 ? "layer" : "layers"}`,
      q.timezone,
      q.person ? "1 person selected" : "",
      q.node ? (hierarchy.nodes.find((node) => node.id === q.node)?.name ?? "Unit selected") : "",
      q.search ? `Search: ${q.search}` : "",
    ].filter(Boolean);
  const recovery=unresolved&&<div role="alert" className={styles.error}><p>The submitted Calendar action has not been confirmed. Your newer draft remains here.</p><Button appearance="standard" disabled={busy} onClick={()=>void send(undefined,undefined,unresolved)}>Retry submitted change</Button></div>;
  return (
    <CalendarDraftBoundary appearance="standard" dirty={editorDirty||pendingShift!==null} busy={commandBusy} pending={unresolved!==null}>
    <div className={styles.workspace} data-calendar-workspace data-ui-appearance="standard" aria-busy={busy}>
      <PageHeader appearance="standard"
        title="Calendar"
        description="See your schedule, team plans and coverage in one place."
        action={
          <div className={styles.headerActions}>
            <div className={styles.presetSwitch} role="group" aria-label="Calendar view">
              {(
                [
                  ["mine", "My"],
                  ["team", "Team"],
                  ["coverage", "Coverage"],
                ] as const
              ).map(([preset, title]) => (
                <Button appearance="standard"
                  variant="plain"
                  key={preset}
                  type="button"
                  aria-pressed={activePreset === preset}
                  onClick={() => changePreset(preset)}
                >
                  {title}
                </Button>
              ))}
            </div>
            {(data.canSchedule ||
              data.canAnnounce ||
              data.canManageRoster ||
              data.selfPersonId) && (
              <details className={styles.createMenu}>
                <summary className={styles.primary}>
                  <Plus size={15} /> Create <ChevronDown size={14} />
                </summary>
                <div className={styles.createMenuItems}>
                  {data.canSchedule && (
                    <>
                      <Button appearance="standard"
                        variant="plain"
                        type="button"
                        onClick={() => setEditor({ kind: "appointment" })}
                      >
                        New appointment
                      </Button>
                      {data.selfPersonId && (
                        <Link
                          href={canonicalUiHref(`${base}/staff-tasks/new?dueDate=${encodeURIComponent(q.date)}`)}
                        >
                          New task
                        </Link>
                      )}
                    </>
                  )}
                  {data.canAnnounce && (
                    <Button appearance="standard"
                      variant="plain"
                      type="button"
                      onClick={() => setEditor({ kind: "announcement" })}
                    >
                      New announcement
                    </Button>
                  )}
                  {data.canManageRoster && (
                    <Button appearance="standard" variant="plain" type="button" onClick={() => void openRoster()}>
                      Plan roster
                    </Button>
                  )}
                  {data.selfPersonId && <Link href={canonicalUiHref(`${base}/leave/new`)}>Request leave</Link>}
                </div>
              </details>
            )}
          </div>
        }
      />
      <section className={styles.toolbar} aria-label="Calendar controls">
        <div className={styles.toolbarCluster}>
          <div className={styles.views} role="group" aria-label="Calendar view">
            {(["day", "week", "month", "agenda"] as const).map((view) => (
              <Button appearance="standard"
                variant="plain"
                key={view}
                type="button"
                aria-pressed={q.view === view}
                onClick={() => change({ view })}
              >
                {view[0].toUpperCase() + view.slice(1)}
              </Button>
            ))}
          </div>
        </div>
        {q.view === "month" ? (
          <MonthNavigation appearance="standard"
            className={styles.calendarMonthNavigation}
            month={q.date.slice(0, 7)}
            date={q.date}
            todayDate={today(q.timezone)}
            onMonthChange={(month) => change({ date: `${month}-01` })}
            onDateChange={(date) => change({ date })}
            onToday={() => change({ date: today(q.timezone) })}
          />
        ) : (
          <div className={`${styles.toolbarCluster} ${styles.datePicker}`}>
            <Button appearance="standard"
              type="submit"
              variant="ghost"
              className={styles.iconButton}
              aria-label="Previous period"
              onClick={() => move(-1)}
            >
              <ChevronLeft size={17} />
            </Button>
            <TextInput appearance="standard"
              aria-label="Calendar date"
              type="date"
              value={q.date}
              onChange={(e) => e.target.value && change({ date: e.target.value })}
            />
            <Button appearance="standard"
              type="submit"
              variant="ghost"
              className={styles.iconButton}
              aria-label="Next period"
              onClick={() => move(1)}
            >
              <ChevronRight size={17} />
            </Button>
            <Button appearance="standard"
              type="submit"
              variant="primary"
              className={styles.button}
              onClick={() => change({ date: today(q.timezone) })}
            >
              Today
            </Button>
          </div>
        )}
        <Button appearance="standard"
          type="submit"
          variant="ghost"
          className={`${styles.iconButton} ${styles.refreshButton}`}
          aria-label="Refresh calendar"
          disabled={busy}
          onClick={() => void load({ ...qRef.current, cursor: 0 })}
        >
          <RefreshCw size={16} />
        </Button>
      </section>
      <div className={styles.filterChips} aria-label="Active calendar filters">
        <strong>
          {activePreset === "coverage"
            ? "Coverage"
            : activePreset === "mine"
              ? "My calendar"
              : "Team calendar"}
        </strong>
        {filterSummary.map((item) => (
          <span key={item}>{item}</span>
        ))}
      </div>
      <FormDisclosure appearance="standard" className={styles.filterPanel} data-calendar-filters>
        <summary>
          Filters <span>{filterSummary.join(" · ")}</span>
        </summary>
        <div className={styles.filterControls}>
          <section className={styles.filterSection} aria-label="Calendar layers">
            <h2>Layers</h2>
            <div className={styles.layerBar}>
              {calendarLayers.map((layer) => (
                <Field appearance="standard" orientation="inline" key={layer}>
                  <TextInput appearance="standard"
                    type="checkbox"
                    checked={q.layers.split(",").includes(layer)}
                    onChange={(e) => {
                      const next = e.target.checked
                        ? [...q.layers.split(","), layer]
                        : q.layers.split(",").filter((x) => x !== layer);
                      if (next.length) change({ layers: next.join(",") });
                    }}
                  />
                  <span className={styles.dot} style={eventStyle(layer)} />
                  {layerNames[layer]}
                </Field>
              ))}
            </div>
            <Field appearance="standard" className={styles.zone}>
              <Globe2 size={13} />
              Display timezone
              <Select appearance="standard"
                aria-label="Display timezone"
                value={q.timezone}
                onChange={(e) => change({ timezone: e.target.value })}
              >
                {[
                  ...new Set([
                    q.timezone,
                    "Asia/Bangkok",
                    "UTC",
                    "Asia/Singapore",
                    "Europe/London",
                    "America/New_York",
                  ]),
                ].map((z) => (
                  <option key={z}>{z}</option>
                ))}
              </Select>
            </Field>
          </section>
          <section className={styles.peopleFilters} aria-label="People filters">
            <h2>
              <Users size={15} /> People
            </h2>
            <form
              className={styles.search}
              onSubmit={(e) => {
                e.preventDefault();
                change({ search: String(new FormData(e.currentTarget).get("search") ?? "") });
              }}
            >
              <Search size={14} />
              <TextInput appearance="standard"
                aria-label="Search people or events"
                name="search"
                placeholder="Search people or events"
                defaultValue={q.search}
                key={q.search}
              />
              <Button appearance="standard"
                clientManaged variant="ghost"
                type="submit"
                className={styles.iconButton}
                aria-label="Search"
              >
                ↵
              </Button>
            </form>
            <Field appearance="standard" className={styles.hierarchyPicker}>
              People hierarchy
              <Select appearance="standard"
                value={q.hierarchy}
                onChange={(e) =>
                  change({ hierarchy: e.target.value as "organization" | "reporting" })
                }
              >
                <option value="organization">Organization</option>
                <option value="reporting">Reporting</option>
              </Select>
            </Field>
            <Button appearance="standard"
              type="submit"
              variant="plain"
              className={styles.person}
              aria-pressed={!q.person && !q.node}
              onClick={() =>
                change({ person: undefined, node: undefined, search: "", mode: "team" })
              }
            >
              All authorized people
            </Button>
            <div className={styles.tree}>
              {q.hierarchy === "organization" ? orgTree(null) : reportTree(null)}
            </div>
            {hierarchy.nextCursor !== null && (
              <Button appearance="standard"
                type="submit"
                variant="primary"
                className={styles.button}
                onClick={() => void morePeople()}
              >
                Load more people
              </Button>
            )}
            <p className={styles.muted}>
              Your selected Company and role determine what appears here.
            </p>
          </section>
        </div>
      </FormDisclosure>
      {error && !editor && !selected && (
        <div className={styles.error} role="alert">
          {error}
        </div>
      )}
      {message && (
        <p className={styles.success} role="status">
          {message}
        </p>
      )}
      {failedSources.length > 0 && (
        <div className={styles.error} role="alert">
          <span>
            Some calendar sources could not be loaded:{" "}
            {failedSources.map((source) => layerNames[source.source]).join(", ")}. Results are
            partial.
          </span>{" "}
          <Button appearance="standard"
            type="submit"
            variant="primary"
            className={styles.button}
            onClick={() => void load({ ...qRef.current, cursor: 0 })}
          >
            Retry calendar sources
          </Button>
        </div>
      )}
      {activePreset === "coverage" && (
        <div className={styles.summary} aria-label="Coverage counts">
          <span>
            <strong>{data.coverage.shifts}</strong> shifts
          </span>
          <span>
            <strong>{data.coverage.assigned}</strong> assigned slots
          </span>
          <span>
            <strong>{data.coverage.open}</strong> open slots
          </span>
          <span>
            <strong>{attention.length}</strong> need attention
          </span>
        </div>
      )}
      {hasMoreResults && (
        <p className={styles.partialStatus} role="status">
          Showing {data.items.length} loaded events. More results are available below.
        </p>
      )}
      <section className={styles.main} aria-label="Calendar events">
        <div className={styles.period}>
          <h2>
            {q.view === "month"
              ? label(q.date, { month: "long", year: "numeric" })
              : q.view === "day"
                ? label(q.date, { weekday: "long", month: "long", day: "numeric", year: "numeric" })
                : `${label(data.from)} – ${label(addDays(data.to, -1), { month: "short", day: "numeric", year: "numeric" })}`}
          </h2>
          {q.person && (
            <Button appearance="standard"
              type="submit"
              variant="primary"
              className={styles.button}
              onClick={() => change({ person: undefined })}
            >
              Clear person
            </Button>
          )}
        </div>
        {attention.length > 0 && (
          <details className={styles.attentionPanel} data-calendar-attention>
            <summary>
              <span className={styles.attentionIcon}>
                <AlertCircle size={18} aria-hidden="true" />
              </span>
              <span className={styles.attentionHeading}>
                <span className={styles.attentionTitle}>
                  Needs attention{" "}
                  <span className={styles.attentionCount}>
                    {attention.length}
                    {attentionIsPartial ? " loaded" : ""}
                  </span>
                </span>
                <span className={styles.attentionPreview}>
                  {attention[0].title}
                  {attention.length > 1 ? ` · +${attention.length - 1} more` : ""}
                </span>
              </span>
              <span className={styles.attentionToggle}>
                <span className={styles.attentionViewAll}>View all</span>
                <span className={styles.attentionHide}>Hide list</span>
                <ChevronDown size={16} aria-hidden="true" />
              </span>
            </summary>
            <ul className={styles.attentionList}>
              {attention.map((item) => {
                const date =
                  item.startDate ??
                  (item.startsAt === null ? null : localMinuteAt(item.startsAt, q.timezone).date);
                const people = item.participants.map((person) => person.name).filter(Boolean);
                const context = [
                  ...people,
                  ...(item.owner && !people.includes(item.owner) ? [item.owner] : []),
                ].join(" · ");
                return (
                  <li key={item.id}>
                    <Button appearance="standard" variant="plain" type="button" onClick={() => open(item)}>
                      <span className={styles.attentionRowMain}>
                        <strong>{item.title}</strong>
                        <span className={styles.attentionReason}>{item.warnings.join(" · ")}</span>
                        {context && <span className={styles.attentionContext}>{context}</span>}
                      </span>
                      <span className={styles.attentionDate}>
                        {date
                          ? label(date, { month: "short", day: "numeric", year: "numeric" })
                          : "Unscheduled"}
                      </span>
                    </Button>
                  </li>
                );
              })}
            </ul>
            {attentionIsPartial && (
              <p className={styles.attentionPartial}>
                Only loaded events are counted; more items may be available.
              </p>
            )}
          </details>
        )}
        {q.view === "week" && activePreset === "mine" && (
          <PersonalTimeGrid
            days={days}
            zone={q.timezone}
            items={data.items}
            onOpen={open}
            onCreate={(date, startLocal, endLocal) =>
              setEditor({ kind: "appointment", startLocal, endLocal })
            }
            onReschedule={rescheduleTimed}
          />
        )}
        {q.view === "week" && activePreset !== "mine" && (
          <TableFrame appearance="standard"
            label="Records"
            className={`${styles.scroll} ${styles.weekScroll}`}
            data-calendar-view="week"
            tabIndex={0}
            role="region"
            aria-label="Weekly people grid"
          >
            <Table className={styles.week}>
              <thead>
                <tr>
                  <th>People & coverage</th>
                  {days.map((day) => (
                    <th key={day} className={day === today(q.timezone) ? styles.today : undefined}>
                      {label(day, { weekday: "short" })}
                      <strong>{label(day, { day: "numeric" })}</strong>
                    </th>
                  ))}
                </tr>
              </thead>
              <tbody>
                <tr>
                  <th scope="row">Team events & open slots</th>
                  {days.map((day) => (
                    <td
                      key={day}
                      data-calendar-date={day}
                      className={dateDrop === day ? styles.dateDropTarget : undefined}
                    >
                      <div className={styles.weekCellEvents}>
                        {daily(day)
                          .filter(
                            (i) =>
                              i.kind === "announcement" ||
                              !i.participants.length ||
                              !people.some((p) => i.participants.some((x) => x.id === p.id)) ||
                              (i.capacity ?? 0) > (i.assigned ?? 0),
                          )
                          .map((i) => event(i))}
                      </div>
                    </td>
                  ))}
                </tr>
                {people.map((p) => (
                  <tr key={p.id}>
                    <th scope="row">
                      <span className={styles.avatar}>{p.name[0]}</span> {p.name}
                    </th>
                    {days.map((day) => (
                      <td
                        key={day}
                        data-calendar-date={day}
                        className={dateDrop === day ? styles.dateDropTarget : undefined}
                      >
                        <div className={styles.weekCellEvents}>
                          {daily(day)
                            .filter(
                              (i) =>
                                i.participants.some((x) => x.id === p.id) ||
                                (i.kind === "task" &&
                                  !!p.userId &&
                                  i.details?.assigneeId === p.userId),
                            )
                            .map((i) => event(i))}
                        </div>
                      </td>
                    ))}
                  </tr>
                ))}
              </tbody>
            </Table>
          </TableFrame>
        )}
        {q.view === "day" && activePreset === "mine" && (
          <PersonalTimeGrid
            days={[q.date]}
            zone={q.timezone}
            items={data.items}
            onOpen={open}
            onCreate={(date, startLocal, endLocal) =>
              setEditor({ kind: "appointment", startLocal, endLocal })
            }
            onReschedule={rescheduleTimed}
          />
        )}
        {q.view === "day" && activePreset !== "mine" && (
          <CalendarDayTimeline
            date={q.date}
            zone={q.timezone}
            items={data.items}
            people={people}
            onOpen={open}
            onReschedule={rescheduleTimed}
          />
        )}
        {q.view === "month" && (
          <div className={styles.monthLayout}>
            <MonthGrid
              month={data.from.slice(0, 7)}
              currentDate={today(q.timezone)}
              selectedDate={q.date}
              dropTargetDate={dateDrop}
              onSelectDate={(date) => change({ date })}
              eventCount={(date) => daily(date).length}
              renderDay={(day) =>
                daily(day)
                  .slice(0, 2)
                  .map((i) => event(i))
              }
              renderMore={(day, count) => (
                <Button appearance="standard"
                  variant="plain"
                  type="button"
                  className={styles.morePreview}
                  aria-label={`Show ${count} more loaded events for ${label(day, { month: "long", day: "numeric" })}`}
                  onClick={() => change({ date: day })}
                >
                  +{count} more
                </Button>
              )}
            />
            <section
              className={styles.monthSelectedDay}
              data-selected-calendar-day
              aria-labelledby="selected-calendar-day"
            >
              <header className={styles.monthSelectedHeader}>
                <div>
                  <span>Selected day</span>
                  <h3 id="selected-calendar-day">
                    {label(q.date, {
                      weekday: "long",
                      month: "long",
                      day: "numeric",
                      year: "numeric",
                    })}
                  </h3>
                </div>
                <span>
                  {daily(q.date).length} {daily(q.date).length === 1 ? "event" : "events"}
                </span>
              </header>
              {daily(q.date).length ? (
                <div className={styles.monthSelectedEvents}>
                  {daily(q.date).map((item) => event(item))}
                </div>
              ) : (
                <p className={styles.monthNoEvents}>No events scheduled for this day.</p>
              )}
            </section>
          </div>
        )}
        {q.view === "agenda" && (
          <div className={styles.agenda} data-calendar-view="agenda">
            {days
              .filter((day) => daily(day).length)
              .map((day) => (
                <section key={day} className={styles.agendaDay}>
                  <h3>
                    {label(day, {
                      weekday: "long",
                      month: "long",
                      day: "numeric",
                    })}
                  </h3>
                  <div className={styles.agendaItems}>{daily(day).map((i) => event(i, true))}</div>
                </section>
              ))}
          </div>
        )}
        {q.view !== "month" &&
          !data.items.length &&
          data.sources.every((s) => s.status === "ready") && (
            <div className={styles.empty}>
              <CalendarDays size={30} />
              <h3>Room for what’s next</h3>
              <p>No events match these dates and filters.</p>
            </div>
          )}
        {!!unscheduled.length && (
          <details className={`${styles.agendaDay} ${styles.unscheduled}`}>
            <summary>Unscheduled preparation · {unscheduled.length}</summary>
            <div className={styles.agendaItems}>{unscheduled.map((i) => event(i, true))}</div>
          </details>
        )}
        <div className={styles.footer}>
          {hasMoreResults && (
            <Button appearance="standard"
              type="submit"
              variant="primary"
              className={styles.button}
              disabled={busy}
              onClick={() => void load({ ...qRef.current, cursor: data.nextCursor! }, true)}
            >
              Load more events
            </Button>
          )}
          <span>
            {busy ? "Refreshing…" : "Source records stay up to date when you return to this page."}
          </span>
        </div>
      </section>
      <Dialog appearance="standard"
        open={!!selected && !editor}
        onClose={closeDetail}
        title={selected?.title ?? "Event"}
        presentation="form-drawer"
      >
        {selected && (
          <div className={styles.detail}>
            {recovery}
            {error && (
              <div className={styles.error} role="alert">
                {error}
              </div>
            )}
            <div className={styles.eyebrow}>
              {layerNames[selected.source]} · {selected.status}
            </div>
            {selected.description && <p>{selected.description}</p>}
            <dl>
              <dt>When</dt>
              <dd>
                {selected.startDate
                  ? label(selected.startDate, {
                      weekday: "short",
                      year: "numeric",
                      month: "short",
                      day: "numeric",
                    })
                  : selected.startsAt
                    ? new Intl.DateTimeFormat(uiLocale, {
                        timeZone: selected.timezone,
                        dateStyle: "medium",
                      }).format(new Date(selected.startsAt))
                    : "Unscheduled"}{" "}
                · {eventTime(selected, selected.timezone)}
                {!selected.allDay && ` · ${selected.timezone}`}
              </dd>
              {selected.owner && (
                <>
                  <dt>Owner</dt>
                  <dd>{selected.owner}</dd>
                </>
              )}
              {!!selected.participants.length && (
                <>
                  <dt>People</dt>
                  <dd>{selected.participants.map((p) => p.name).join(", ")}</dd>
                </>
              )}
              {!!selected.details?.attendees?.length && (
                <>
                  <dt>Invitations</dt>
                  <dd>
                    {selected.details.attendees.map((attendee) => (
                      <p key={attendee.userId}>
                        {attendee.name} · {attendee.attendance} ·{" "}
                        {attendee.response === "pending"
                          ? "Tentative (awaiting response)"
                          : attendee.response}
                      </p>
                    ))}
                  </dd>
                </>
              )}
              {selected.details?.location && (
                <>
                  <dt>Location</dt>
                  <dd>{selected.details.location}</dd>
                </>
              )}
              {selected.details?.meetingUrl && (
                <>
                  <dt>Meeting</dt>
                  <dd>
                    <a href={selected.details.meetingUrl} target="_blank" rel="noreferrer">
                      Join meeting
                    </a>
                  </dd>
                </>
              )}
              {selected.details?.guests?.length ? (
                <>
                  <dt>External guests</dt>
                  <dd>
                    {selected.details.guests.map((g, i) => (
                      <p key={i}>
                        {g.name} · {g.email} · {g.phone}
                      </p>
                    ))}
                  </dd>
                </>
              ) : null}
              {selected.details?.responsibility && (
                <>
                  <dt>Responsibility</dt>
                  <dd>{selected.details.responsibility}</dd>
                </>
              )}
            </dl>
            {selected.warnings.map((w) => (
              <p key={w} className={styles.attention}>
                {w}
              </p>
            ))}
            <div className={styles.actions}>
              {selected.kind === "appointment" &&
                selected.occurrenceId &&
                selected.permittedActions.includes("appointment.reschedule") && (
                  <Field appearance="standard">
                    Edit scope
                    <Select appearance="standard"
                      value={editScope}
                      onChange={(e) =>
                        setEditScope(e.target.value as "single" | "future" | "series")
                      }
                    >
                      <option value="single">This occurrence</option>
                      <option value="future">This and future occurrences</option>
                      <option value="series">The series</option>
                    </Select>
                  </Field>
                )}
              {selected.kind === "appointment" &&
                selected.permittedActions.includes("appointment.reschedule") &&
                selected.startsAt !== null &&
                selected.endsAt !== null && (
                  <form
                    onSubmit={(e) => {
                      e.preventDefault();
                      const form = new FormData(e.currentTarget);
                      void send({
                        action: "appointment.reschedule",
                        id: selected.sourceId,
                        expectedVersion: selected.version,
                        startLocal: String(form.get("start")),
                        endLocal: String(form.get("end")),
                        timezone: selected.timezone,
                        ...(selected.occurrenceId
                          ? { occurrenceId: selected.occurrenceId, scope: editScope }
                          : {}),
                      });
                    }}
                  >
                    <Field appearance="standard">
                      Starts
                      <TextInput appearance="standard"
                        type="datetime-local"
                        name="start"
                        step="900"
                        defaultValue={`${localMinuteAt(selected.startsAt, selected.timezone).date}T${localMinuteAt(selected.startsAt, selected.timezone).time}`}
                      />
                    </Field>
                    <Field appearance="standard">
                      Ends
                      <TextInput appearance="standard"
                        type="datetime-local"
                        name="end"
                        step="900"
                        defaultValue={`${localMinuteAt(selected.endsAt, selected.timezone).date}T${localMinuteAt(selected.endsAt, selected.timezone).time}`}
                      />
                    </Field>
                    <Button appearance="standard"
                      clientManaged type="submit"
                      variant="primary"
                      className={styles.button}
                      disabled={busy}
                    >
                      Save time
                    </Button>
                  </form>
                )}
              {selected.permittedActions.includes("appointment.respond") && (
                <div role="group" aria-label="Respond to invitation">
                  {(["accepted", "tentative", "declined"] as const).map((response) => (
                    <Button appearance="standard"
                      type="submit"
                      variant="primary"
                      key={response}
                      className={styles.button}
                      disabled={busy}
                      onClick={() =>
                        void send({
                          action: "appointment.respond",
                          id: selected.sourceId,
                          expectedVersion: selected.version,
                          occurrenceId: selected.occurrenceId,
                          scope: selected.occurrenceId ? "single" : "series",
                          response,
                        })
                      }
                    >
                      {response === "accepted"
                        ? "Accept"
                        : response === "tentative"
                          ? "Tentative"
                          : "Decline"}
                    </Button>
                  ))}
                </div>
              )}
              {selected.permittedActions.includes("appointment.reminder.set") && (
                <Field appearance="standard">
                  My reminder
                  <Select appearance="standard"
                    defaultValue="15"
                    onChange={(e) =>
                      void send({
                        action: "appointment.reminder.set",
                        id: selected.sourceId,
                        expectedVersion: selected.version,
                        minutes: e.target.value === "off" ? null : Number(e.target.value),
                      })
                    }
                  >
                    <option value="off">Off</option>
                    <option disabled={data.purchase?.newWork===false} value="0">At start</option>
                    <option disabled={data.purchase?.newWork===false} value="5">5 minutes</option>
                    <option disabled={data.purchase?.newWork===false} value="15">15 minutes</option>
                    <option disabled={data.purchase?.newWork===false} value="30">30 minutes</option>
                    <option disabled={data.purchase?.newWork===false} value="60">1 hour</option>
                  </Select>
                </Field>
              )}
              {selected.permittedActions.includes("roster.open") && (
                <Button appearance="standard"
                  type="submit"
                  variant="primary"
                  className={styles.primary}
                  onClick={() => void openRoster(selected)}
                >
                  Open roster actions
                </Button>
              )}
              {selected.href && selected.source !== "roster" && (
                <UiActionLink appearance="standard"
                  variant="primary"
                  className={styles.button}
                  href={`${base}${selected.href}`}
                >
                  {selected.source === "support"
                    ? "Open support case"
                    : selected.source === "crm"
                      ? "Open CRM record"
                      : selected.source === "customer_followups"
                        ? "Open customer follow-up"
                        : selected.source === "supplier_bills"
                          ? "Open supplier invoice review"
                          : selected.source === "procurement_rfqs"
                            ? "Open supplier quotation review"
                            : selected.source === "supplier_promises"
                              ? "Open supplier confirmation"
                              : selected.source === "stock_transit"
                                ? "Open transport record"
                                : selected.source === "quality_holds"
                                  ? "Open quality hold"
                                  : selected.source === "supplier_batches"
                                    ? "Open supplier payment batch"
                                    : selected.source === "refunds"
                                      ? "Open refund obligation"
                                      : selected.source === "leave"
                                        ? "Open leave request"
                                        : "Open lifecycle case"}
                </UiActionLink>
              )}
              {selected.permittedActions.map((action) =>
                action === "appointment.save" ||
                action === "announcement.save" ||
                action === "task.plan" ? (
                  <Button appearance="standard"
                    type="submit"
                    variant="primary"
                    key={action}
                    className={styles.primary}
                    onClick={() => {
                      setError("");
                      setEditor({
                        kind:
                          selected.kind === "task"
                            ? "task"
                            : selected.kind === "announcement"
                              ? "announcement"
                              : "appointment",
                        item: selected,
                      });
                    }}
                  >
                    {action === "task.plan" ? "Assign & schedule" : "Edit"}
                  </Button>
                ) : action === "appointment.respond" ||
                  action === "appointment.reminder.set" ? null : action.startsWith(
                    "appointment.",
                  ) ||
                  action.startsWith("announcement.") ||
                  action === "task.complete" ? (
                  <Button appearance="standard"
                    type="submit"
                    variant="primary"
                    key={action}
                    className={styles.button}
                    disabled={busy}
                    onClick={() =>
                      void send({
                        action:
                          action === "task.complete" ? action : `${action.split(".")[0]}.status`,
                        id: selected.sourceId,
                        expectedVersion: selected.version,
                        ...(selected.occurrenceId && action === "appointment.cancelled"
                          ? { occurrenceId: selected.occurrenceId, scope: editScope }
                          : {}),
                        ...(action !== "task.complete" ? { status: action.split(".")[1] } : {}),
                      })
                    }
                  >
                    {
                      {
                        cancelled: "Cancel appointment",
                        completed: "Complete",
                        no_show: "Record no-show",
                        published: "Publish",
                        withdrawn: "Withdraw",
                        complete: "Complete task",
                      }[action.split(".")[1]]
                    }
                  </Button>
                ) : null,
              )}
            </div>
            {!!selected.history.length && (
              <>
                <h3>History</h3>
                <ul className={styles.history}>
                  {selected.history.map((h, i) => (
                    <li key={i}>
                      {h.action.replaceAll(".", " ")} · {h.actor}
                      <time dateTime={new Date(h.at).toISOString()}>
                        {formatUiDateTime(h.at, q.timezone)}
                      </time>
                    </li>
                  ))}
                </ul>
              </>
            )}
          </div>
        )}
      </Dialog>
      {recovery}
      <Dialog appearance="standard" open={discardEditor} title="Discard Calendar draft?" onClose={()=>setDiscardEditor(false)}><p>{unresolved?"The submitted action is still unresolved. Retry it in the original account and role before discarding this editor.":"Your Calendar draft has not been saved."}</p><div className="ui-form-actions"><Button appearance="standard" data-autofocus onClick={()=>setDiscardEditor(false)}>Keep editing</Button>{!unresolved&&!commandBusy&&<Button appearance="standard" variant="danger" onClick={()=>{setDiscardEditor(false);setEditorDirty(false);setEditor(null);}}>Discard draft</Button>}</div></Dialog>
      <Dialog appearance="standard"
        open={!!editor}
        onClose={closeEditor}
        retainFormContents={editorDirty||commandBusy||unresolved!==null}
        dismissDisabled={commandBusy||unresolved!==null}
        title={
          editor?.kind === "appointment"
            ? editor.item
              ? "Edit appointment"
              : "New appointment"
            : editor?.kind === "announcement"
              ? "Announcement"
              : "Assign & schedule task"
        }
       presentation="form-drawer">
        {editor && (
          <>
            {error && (
              <div className={styles.error} role="alert">
                {error}
              </div>
            )}
            {recovery}
            <CalendarForm
              key={`${editor.kind}-${editor.item?.id ?? "new"}`}
              editor={editor}
              hierarchy={hierarchy}
              date={q.date}
              selfPersonId={data.selfPersonId}
              busy={busy}
              send={(value,completion) => void send(value,completion)}
              edited={()=>setEditorDirty(true)}
              close={closeEditor}
              accounts={accounts}
              api={api}
            />
          </>
        )}
      </Dialog>
      <Dialog appearance="standard"
        open={!!pendingShift}
        onClose={() => {
          if (pendingShift) replaceItem(pendingShift.item);
          setPendingShift(null);
        }}
        title="Explain published roster change"
        presentation="form-drawer"
      >
        {pendingShift && (
          <form
            className={styles.form}
            onSubmit={(formEvent) => {
              formEvent.preventDefault();
              const reason = String(
                new FormData(formEvent.currentTarget).get("reason") ?? "",
              ).trim();
              if (reason.length < 3) {
                setError("Explain the change to this published roster.");
                return;
              }
              const pending = pendingShift;
              setPendingShift(null);
              rescheduleTimed(pending.item, pending.change, reason);
            }}
          >
            {error && (
              <div className={styles.error} role="alert">
                {error}
              </div>
            )}
            <p>The shift stays at the proposed time while you provide the required audit reason.</p>
            <Field appearance="standard">
              Reason
              <TextArea appearance="standard" name="reason" required minLength={3} maxLength={1000} autoFocus />
            </Field>
            <div className={styles.actions}>
              <Button appearance="standard"
                variant="primary"
                type="button"
                className={styles.button}
                onClick={() => {
                  replaceItem(pendingShift.item);
                  setPendingShift(null);
                }}
              >
                Cancel
              </Button>
              <Button appearance="standard" clientManaged variant="primary" type="submit" className={styles.primary}>
                Save change
              </Button>
            </div>
          </form>
        )}
      </Dialog>
      {roster&&!rosterVisible&&<Button appearance="standard" type="button" variant="secondary" onClick={()=>setRosterVisible(true)}>Reopen retained Roster planning</Button>}
      <Dialog appearance="standard"
        open={!!roster&&rosterVisible}
        retainFormContents
        dismissDisabled={rosterDraft?.busy}
        onClose={() => {
          if(rosterDraftLive.current?.busy)return;
          if(rosterOwnerDraftHeld(rosterDraftLive.current)){setRosterDismiss(true);return;}
          setRosterVisible(false);setRoster(null);rosterDraftLive.current=null;setRosterDraft(null);
          void load({ ...qRef.current, cursor: 0 });
        }}
        title="Roster planning"
        presentation="fullscreen"
      >
        {roster && (
          <RosterWorkspace appearance="standard" base={base} initial={roster} initialDate={rosterDate} embedded onDraftStateChange={trackRosterDraft}/>
        )}
      </Dialog>
      <Dialog appearance="standard" open={rosterDismiss} title="Retain Roster work?" onClose={()=>setRosterDismiss(false)}>
        <p>{rosterDraft?.pending?"The original Roster result is not confirmed. Keep the request and form for exact recovery.":"Your Roster draft has unsaved changes."}</p>
        <div className={styles.actions}>
          <Button appearance="standard" type="button" disabled={rosterDraft?.busy} onClick={()=>setRosterDismiss(false)}>Keep editing Roster</Button>
          <Button appearance="standard" type="button" disabled={rosterDraft?.busy} onClick={()=>{if(rosterDraftLive.current?.busy)return;setRosterDismiss(false);setRosterVisible(false);}}>Close and retain Roster</Button>
          {!rosterDraft?.pending&&<Button appearance="standard" type="button" variant="danger" disabled={rosterDraft?.busy} onClick={()=>{if(rosterDraftLive.current?.busy||rosterDraftLive.current?.pending)return;setRosterDismiss(false);setRosterVisible(false);setRoster(null);rosterDraftLive.current=null;setRosterDraft(null);}}>Discard Roster draft</Button>}
        </div>
      </Dialog>
    </div>
    </CalendarDraftBoundary>
  );
}

export function StandardCalendarWorkspace(props: Parameters<typeof CalendarView>[0]) {
  return <CalendarView key={props.contextKey??props.base} {...props} />;
}
