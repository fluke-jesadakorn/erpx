"use client";
import { FeatureTabs } from "@/components/features/feature-page";
import { RosterWorkspace } from "@/components/people/roster-workspace";
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
} from "@/lib/calendar";
import { addDays,rosterMonday,type RosterData } from "@/lib/people-roster";
import { localDateTimeToInstant, localMinuteAt, moveInclusiveRange } from "@/lib/calendar-timeline";
import { workFetch } from "@/lib/work-fetch";
import {
CalendarDays,
ChevronLeft,
ChevronRight,
Globe2,
RefreshCw,
Search,
Users
} from "lucide-react";
import Link from "next/link";
import {
useCallback,
useEffect,
useRef,
useState,
type ReactNode,
type PointerEvent as ReactPointerEvent,
} from "react";
import {
CalendarEvent,
eventStyle,
eventTime,
layerNames,
} from "./calendar-event";
import { CalendarForm,type CalendarEditor } from "./calendar-forms";
import { CalendarDayTimeline } from "./calendar-day-timeline";
import styles from "./calendar.module.css";
const label = (
  date: string,
  options: Intl.DateTimeFormatOptions = { month: "short", day: "numeric" },
) =>
  new Intl.DateTimeFormat("en", { ...options, timeZone: "UTC" }).format(
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
  explicitView,
  initialEvent,
}: {
  base: string;
  initial: CalendarData;
  initialHierarchy: CalendarHierarchy;
  explicitView: boolean;
  initialEvent?: string;
}) {
  const [data, setData] = useState(initial),
    [hierarchy, setHierarchy] = useState(initialHierarchy),
    [busy, setBusy] = useState(false),
    [error, setError] = useState(""),
    [message, setMessage] = useState("");
  const [selected, setSelected] = useState<CalendarItem | null>(
      initial.items.find(
        (i) => i.id === initialEvent || i.sourceId === initialEvent,
      ) ?? null,
    ),
    [editor, setEditor] = useState<CalendarEditor | null>(null),
    [roster, setRoster] = useState<RosterData | null>(null),
    [rosterDate, setRosterDate] = useState(initial.query.date),
    [dateDrop, setDateDrop] = useState<string | null>(null),
    [dateDragging, setDateDragging] = useState<string | null>(null),
    [pendingShift, setPendingShift] = useState<{ item: CalendarItem; change: { startsAt: number; endsAt: number } } | null>(null);
  const qRef = useRef(initial.query),
    receipt = useRef<{ body: string; key: string } | null>(null),
    requestSeq = useRef(0),
    mobileDefaultApplied = useRef(false),
    api = `/api/v1${base}/calendar`;
  const load = useCallback(
    async (
      q: CalendarQuery,
      append = false,
      writeUrl: false | "push" | "replace" = false,
    ) => {
      const seq = ++requestSeq.current;
      setBusy(true);
      setError("");
      try {
        const r = await workFetch(`${api}?${queryString(q)}`, {
            cache: "no-store",
          }),
          j = await r.json();
        if (!r.ok)
          throw new Error(j.error?.message ?? "Calendar could not be loaded.");
        if (seq !== requestSeq.current) return;
        const next = j.data as CalendarData;
        qRef.current = { ...next.query, cursor: 0 };
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
            : next.items.find(
                (i) => i.id === eventId || i.sourceId === eventId,
              ) ?? null,
        );
        if (writeUrl)
          window.history[`${writeUrl}State`](
            null,
            "",
            `${base}/calendar?${queryString(qRef.current)}${writeUrl === "replace" && eventId ? `&event=${encodeURIComponent(eventId)}` : ""}`,
          );
      } catch (e) {
        if (seq === requestSeq.current)
          setError(
            e instanceof Error ? e.message : "Calendar could not be loaded.",
          );
      } finally {
        if (seq === requestSeq.current) setBusy(false);
      }
    },
    [api, base],
  );
  useEffect(() => {
    if (
      !explicitView &&
      !mobileDefaultApplied.current &&
      window.matchMedia("(max-width:760px)").matches
    ) {
      mobileDefaultApplied.current = true;
      void load({ ...qRef.current, view: "agenda" }, false, "replace");
    }
    const refresh = () => {
      if (document.visibilityState === "visible")
        void load({ ...qRef.current, cursor: 0 });
    };
    const pop = () => {
      const parsed = calendarQuerySchema.safeParse(
        Object.fromEntries(new URL(window.location.href).searchParams),
      );
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
  }, [load, explicitView]);
  const q = data.query,
    days = Array.from(
      {
        length: Math.round(
          (Date.parse(data.to) - Date.parse(data.from)) / 86400000,
        ),
      },
      (_, i) => addDays(data.from, i),
    );
  function change(patch: Partial<CalendarQuery>) {
    const query={ ...qRef.current, ...patch, cursor: 0 };
    void load(query,false,"push");
  }
  function open(item: CalendarItem) {
    setSelected(item);
    setError("");
    setMessage("");
    const url = new URL(window.location.href);
    url.searchParams.set("event", item.id);
    url.searchParams.set("view", qRef.current.view);
    window.history.pushState(null, "", url);
  }
  function closeDetail() {
    setSelected(null);
    const url = new URL(window.location.href);
    url.searchParams.delete("event");
    window.history.replaceState(null, "", url);
  }
  async function send(input: unknown) {
    setBusy(true);
    setError("");
    setMessage("");
    try {
      const body = JSON.stringify(input);
      if (receipt.current?.body !== body)
        receipt.current = { body, key: crypto.randomUUID() };
      const r = await workFetch(api, {
          method: "POST",
          headers: {
            "Content-Type": "application/json",
            "Idempotency-Key": receipt.current.key,
          },
          body,
        }),
        j = await r.json();
      if (!r.ok)
        throw new Error(j.error?.message ?? "Could not save this change.");
      receipt.current = null;
      setEditor(null);
      await load({ ...qRef.current, cursor: 0 });
      setMessage(
        j.data.warnings?.length
          ? `Saved. ${j.data.warnings.join(" ")}`
          : "Calendar updated.",
      );
    } catch (e) {
      setError(e instanceof Error ? e.message : "Could not save this change.");
    } finally {
      setBusy(false);
    }
  }
  const replaceItem = (replacement: CalendarItem) =>
    setData((current) => ({
      ...current,
      items: current.items.map((item) => item.id === replacement.id ? replacement : item),
    }));
  async function sendReschedule(item: CalendarItem, optimistic: CalendarItem, input: unknown) {
    replaceItem(optimistic);
    setBusy(true);
    setError("");
    setMessage("");
    try {
      const body = JSON.stringify(input);
      if (receipt.current?.body !== body) receipt.current = { body, key: crypto.randomUUID() };
      const response = await workFetch(api, {
        method: "POST",
        headers: { "Content-Type": "application/json", "Idempotency-Key": receipt.current.key },
        body,
      });
      const json = await response.json();
      if (!response.ok) throw new Error(json.error?.message ?? "Could not reschedule this item.");
      receipt.current = null;
      replaceItem({ ...optimistic, version: json.data.version, warnings: json.data.warnings ?? optimistic.warnings });
      setMessage(json.data.warnings?.length ? `Saved. ${json.data.warnings.join(" ")}` : "Calendar updated.");
      await load({ ...qRef.current, cursor: 0 });
    } catch (cause) {
      replaceItem(item);
      setError(cause instanceof Error ? cause.message : "Could not reschedule this item.");
    } finally {
      setBusy(false);
    }
  }
  function rescheduleTimed(item: CalendarItem, change: { startsAt: number; endsAt: number }, reason = "") {
    setError("");
    const optimistic = { ...item, startsAt: change.startsAt, endsAt: change.endsAt };
    if (item.kind === "shift" && item.details?.requiresReason && !reason) {
      replaceItem(optimistic);
      setPendingShift({ item, change });
      return;
    }
    if (item.kind === "appointment") {
      const start = localMinuteAt(change.startsAt, q.timezone), end = localMinuteAt(change.endsAt, q.timezone);
      void sendReschedule(item, optimistic, {
        action: "appointment.reschedule",
        id: item.sourceId,
        expectedVersion: item.version,
        startLocal: `${start.date}T${start.time}`,
        endLocal: `${end.date}T${end.time}`,
        timezone: q.timezone,
      });
    } else if (item.kind === "shift") {
      const start = localMinuteAt(change.startsAt, item.timezone), end = localMinuteAt(change.endsAt, item.timezone);
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
      void sendReschedule(item, { ...item, startDate: target, endDate: addDays(target, 1) }, {
        action: "task.reschedule", id: item.sourceId, expectedVersion: item.version, dueDate: target,
      });
      return;
    }
    if (item.kind === "announcement" && item.startDate && item.endDate && item.permittedActions.includes("announcement.reschedule")) {
      const inclusiveEnd = addDays(item.endDate, -1);
      const range = edge === "start"
        ? { startDate: target <= inclusiveEnd ? target : inclusiveEnd, endDate: inclusiveEnd }
        : edge === "end"
          ? { startDate: item.startDate, endDate: target >= item.startDate ? target : item.startDate }
          : moveInclusiveRange(item.startDate, inclusiveEnd, target);
      void sendReschedule(item, { ...item, startDate: range.startDate, endDate: addDays(range.endDate, 1) }, {
        action: "announcement.reschedule", id: item.sourceId, expectedVersion: item.version, ...range,
      });
      return;
    }
    if ((item.kind === "appointment" || item.kind === "shift") && item.startsAt !== null && item.endsAt !== null && item.permittedActions.includes(`${item.kind}.reschedule`)) {
      const displayed = localMinuteAt(item.startsAt, q.timezone);
      const startsAt = localDateTimeToInstant(`${target}T${displayed.time}`, q.timezone);
      rescheduleTimed(item, { startsAt, endsAt: startsAt + (item.endsAt - item.startsAt) });
    }
  }
  function beginDateDrag(item: CalendarItem, event: ReactPointerEvent, edge?: "start" | "end") {
    const movable = item.permittedActions.includes(`${item.kind}.reschedule`);
    if (!movable) return;
    event.stopPropagation();
    const originX = event.clientX, originY = event.clientY;
    let moved = false, target: string | null = null;
    const onMove = (pointer: PointerEvent) => {
      moved ||= Math.hypot(pointer.clientX - originX, pointer.clientY - originY) >= 5;
      if (!moved) return;
      setDateDragging(item.id);
      pointer.preventDefault();
      target = (document.elementFromPoint(pointer.clientX, pointer.clientY)?.closest("[data-calendar-date]") as HTMLElement | null)?.dataset.calendarDate ?? null;
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
    const escape = (keyboard: KeyboardEvent) => { if (keyboard.key === "Escape") cancel(); };
    window.addEventListener("keydown", escape);
  }
  async function openRoster(item?: CalendarItem) {
    setError("");
    setBusy(true);
    try {
      const department = item?.departmentId ?? hierarchy.people.find(p=>p.id===q.person)?.departmentId ?? (q.node ? hierarchy.nodes.find(n=>n.kind==="department" && withinNode(n.id,q.node))?.id : undefined);
      const params = new URLSearchParams({week:rosterMonday(item?.startDate ?? q.date),...(department ? {department} : {})});
      const r = await workFetch(`/api/v1${base}/people/roster?${params}`, {
          cache: "no-store",
        }),
        j = await r.json();
      if (!r.ok)
        throw new Error(j.error?.message ?? "Could not load roster planning.");
      setRosterDate(item?.startDate ?? q.date);
      closeDetail();
      setRoster(j.data);
    } catch (e) {
      setError(e instanceof Error ? e.message : "Could not load the roster.");
    } finally {
      setBusy(false);
    }
  }
  async function morePeople() {
    setBusy(true);
    try {
      const r = await workFetch(
          `${api}/hierarchy?cursor=${hierarchy.nextCursor}`,
        ),
        j = await r.json();
      if (!r.ok)
        throw new Error(j.error?.message ?? "People could not be loaded.");
      setHierarchy((old) => ({
        ...j.data,
        people: [
          ...new Map(
            [...old.people, ...j.data.people].map((p: CalendarPerson) => [
              p.id,
              p,
            ]),
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
    <button
      key={p.id}
      className={styles.person}
      aria-pressed={q.person === p.id}
      onClick={() =>
        change({ person: q.person === p.id ? undefined : p.id, mode: "team" })
      }
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
    </button>
  );
  function orgTree(parent: string | null, seen = new Set<string>()): ReactNode {
    return hierarchy.nodes
      .filter(
        (n) =>
          n.parentId === parent ||
          (parent === null &&
            !hierarchy.nodes.some((x) => x.id === n.parentId)),
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
              <button
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
              </button>
            )}
            <div className={styles.treeChildren}>
              {people.map(personButton)}
              {orgTree(n.id, next)}
            </div>
          </details>
        );
      });
  }
  function reportTree(
    manager: string | null,
    seen = new Set<string>(),
  ): ReactNode {
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
          <div className={styles.treeChildren}>
            {reportTree(p.id, new Set([...seen, p.id]))}
          </div>
        </div>
      ));
  }
  function withinNode(id:string|null,node:string|undefined):boolean {
    if (!node) return true;
    const seen=new Set<string>();
    while(id && !seen.has(id)) { if(id===node)return true; seen.add(id); id=hierarchy.nodes.find(n=>n.id===id)?.parentId??null; }
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
    ...new Map(
      [...eventPeople, ...hierarchy.people].map((p) => [p.id, p]),
    ).values(),
  ].filter(
    (p) =>
      (!q.person || p.id === q.person) &&
      (q.mode !== "mine" || p.id === data.selfPersonId) &&
      (q.preset === "coverage" || eventPeople.some(event => event.id === p.id)) &&
      (!q.node || withinNode(p.departmentId,q.node) || withinNode(p.branchId,q.node)) &&
      (!q.search ||
        p.name.toLocaleLowerCase().includes(q.search.toLocaleLowerCase()) ||
        data.items.some(
          (i) =>
            i.participants.some((x) => x.id === p.id) ||
            (i.kind === "task" &&
              !!p.userId &&
              i.details?.assigneeId === p.userId),
        )),
  );
  const event = (item: CalendarItem, showPeople = false) => (
    <CalendarEvent
      key={item.id}
      item={item}
      zone={q.timezone}
      onOpen={open}
      showPeople={showPeople}
      onPointerDown={q.view === "week" || q.view === "month" ? (pointer) => beginDateDrag(item, pointer) : undefined}
      resizeHandles={(q.view === "week" || q.view === "month") && item.kind === "announcement" && item.permittedActions.includes("announcement.reschedule")}
      onResizePointerDown={(edge, pointer) => beginDateDrag(item, pointer, edge)}
      dragging={dateDragging === item.id}
      compact={q.view === "week" || q.view === "month"}
    />
  );
  const itemsForDate = (date: string) =>
    data.items
      .filter((i) => itemOnDate(i, date, q.timezone))
      .sort(
        (a, b) =>
          (a.startsAt ?? 0) - (b.startsAt ?? 0) ||
          a.title.localeCompare(b.title),
      );
  const byDate = new Map(days.map((day) => [day, itemsForDate(day)]));
  const daily = (date: string) => byDate.get(date) ?? [];
  const attention = data.items.filter((i) => i.warnings.length),
    unscheduled = data.items.filter((i) => !i.startDate && i.startsAt === null);
  const move = (direction: number) => {
    if (q.view === "month") {
      const d = new Date(`${q.date.slice(0, 7)}-01T12:00:00Z`);
      d.setUTCMonth(d.getUTCMonth() + direction);
      change({ date: d.toISOString().slice(0, 10) });
    } else
      change({ date: addDays(q.date, direction * (q.view === "week" ? 7 : q.view === "day" ? 1 : 30)) });
  };
  return (
    <div className={styles.workspace} data-calendar-workspace aria-busy={busy}>

      <section className={styles.toolbar} aria-label="Calendar filters">
        <div className={`${styles.toolbarCluster} ${styles.creationActions}`} aria-label="Create and plan">
          {data.canSchedule && <><button onClick={() => setEditor({kind:"appointment"})}>New appointment</button><button onClick={() => setEditor({kind:"task"})}>New task</button></>}
          {data.canAnnounce && <button onClick={() => setEditor({kind:"announcement"})}>New announcement</button>}
          {data.canManageRoster && <button onClick={() => void openRoster()}>Plan roster</button>}
          {data.selfPersonId && <Link href={`${base}/leave/new`}>Request leave</Link>}
        </div>
        <div className={styles.toolbarCluster}>
          <div className={styles.views} role="group" aria-label="Calendar view">
            {(["day", "week", "month", "agenda"] as const).map((view) => (
              <button
                key={view}
                type="button"
                aria-pressed={q.view === view}
                onClick={() => change({ view })}
              >
                {view[0].toUpperCase() + view.slice(1)}
              </button>
            ))}
          </div>
        </div>
        <div className={`${styles.toolbarCluster} ${styles.datePicker}`}>
          <button
            className={styles.iconButton}
            aria-label="Previous period"
            onClick={() => move(-1)}
          >
            <ChevronLeft size={17} />
          </button>
          <input
            aria-label="Calendar date"
            type="date"
            value={q.date}
            onChange={(e) => e.target.value && change({ date: e.target.value })}
          />
          <button
            className={styles.iconButton}
            aria-label="Next period"
            onClick={() => move(1)}
          >
            <ChevronRight size={17} />
          </button>
          <button
            className={styles.button}
            onClick={() => change({ date: today(q.timezone) })}
          >
            Today
          </button>
          <button
            className={styles.iconButton}
            aria-label="Refresh calendar"
            disabled={busy}
            onClick={() => void load({ ...qRef.current, cursor: 0 })}
          >
            <RefreshCw size={16} />
          </button>
        </div>
      </section>
      <details className={styles.filterPanel}><summary>Filters · layers, timezone and hierarchy</summary>          <label>People hierarchy<select value={q.hierarchy} onChange={e => change({hierarchy:e.target.value as "organization"|"reporting"})}><option value="organization">Organization</option><option value="reporting">Reporting</option></select></label>
<div className={styles.layerBar} aria-label="Event layers">
        {calendarLayers.map((layer) => (
          <label key={layer}>
            <input
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
          </label>
        ))}
        <label className={styles.zone}>
          <Globe2 size={13} />
          <select
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
          </select>
        </label>
      </div>
      </details>
      <div className={styles.filterChips} aria-label="Active calendar filters"><span className="badge">{q.preset === "coverage" ? "Coverage" : q.mode === "mine" ? "My calendar" : "Team calendar"}</span><span className="badge">{q.timezone}</span>{q.layers.split(",").map(layer=><button className="button ghost" key={layer} disabled={q.layers.split(",").length===1} onClick={()=>change({layers:q.layers.split(",").filter(v=>v!==layer).join(",")})}>{layerNames[layer as keyof typeof layerNames]} ×</button>)}{q.person&&<button className="button ghost" onClick={()=>change({person:undefined})}>Selected person ×</button>}{q.node&&<button className="button ghost" onClick={()=>change({node:undefined})}>{hierarchy.nodes.find(n=>n.id===q.node)?.name??"Team"} ×</button>}{q.search&&<button className="button ghost" onClick={()=>change({search:""})}>{q.search} ×</button>}</div>
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
      {data.sources
        .filter((s) => s.status === "error")
        .map((s) => (
          <div className={styles.error} role="alert" key={s.source}>
            {s.message}{" "}
            <button
              className={styles.button}
              onClick={() => void load({ ...qRef.current, cursor: 0 })}
            >
              Retry
            </button>
          </div>
        ))}
      <div className={styles.summary}>
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
        {data.nextCursor !== null && (
          <span>Showing loaded events · more available below</span>
        )}
      </div>
      <div className={styles.layout}>
        <aside className={styles.sidebar}>
          <h2>
            <Users size={15} /> People
          </h2>
          <form
            className={styles.search}
            onSubmit={(e) => {
              e.preventDefault();
              change({
                search: String(
                  new FormData(e.currentTarget).get("search") ?? "",
                ),
              });
            }}
          >
            <Search size={14} />
            <input
              aria-label="Search people or events"
              name="search"
              placeholder="Search people or events"
              defaultValue={q.search}
              key={q.search}
            />
            <button
              type="submit"
              className={styles.iconButton}
              aria-label="Search"
            >
              ↵
            </button>
          </form>
          <button
            className={styles.person}
            aria-pressed={!q.person && !q.node}
            onClick={() =>
              change({
                person: undefined,
                node: undefined,
                search: "",
                mode: "team",
              })
            }
          >
            All authorized people
          </button>
          <div className={styles.tree}>
            {q.hierarchy === "organization" ? orgTree(null) : reportTree(null)}
          </div>
          {hierarchy.nextCursor !== null && (
            <button className={styles.button} onClick={() => void morePeople()}>
              Load more people
            </button>
          )}
          <p className={styles.muted}>
            Your selected Company and role determine what appears here.
          </p>
        </aside>
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
              <button
                className={styles.button}
                onClick={() => change({ person: undefined })}
              >
                Clear person
              </button>
            )}
          </div>
          {attention.length > 0 && (
            <details className={styles.attention}>
              <summary>{attention.length} items need attention</summary>
              {attention.map((i) => (
                <button key={i.id} onClick={() => open(i)}>
                  {i.title} · {i.warnings.join(" ")}
                </button>
              ))}
            </details>
          )}
          {q.view === "week" && (
            <div
              className={`${styles.scroll} ${styles.weekScroll}`}
              data-calendar-view="week"
              tabIndex={0}
              role="region"
              aria-label="Weekly people grid"
            >
              <table className={styles.week}>
                <thead>
                  <tr>
                    <th>People & coverage</th>
                    {days.map((day) => (
                      <th
                        key={day}
                        className={
                          day === today(q.timezone) ? styles.today : undefined
                        }
                      >
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
                      <td key={day} data-calendar-date={day} className={dateDrop === day ? styles.dateDropTarget : undefined}>
                        <div className={styles.weekCellEvents}>
                          {daily(day)
                            .filter(
                              (i) =>
                                i.kind === "announcement" ||
                                !i.participants.length ||
                                !people.some((p) =>
                                  i.participants.some((x) => x.id === p.id),
                                ) ||
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
                        <span className={styles.avatar}>{p.name[0]}</span>{" "}
                        {p.name}
                      </th>
                      {days.map((day) => (
                        <td key={day} data-calendar-date={day} className={dateDrop === day ? styles.dateDropTarget : undefined}>
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
              </table>
            </div>
          )}
          {q.view === "day" && (
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
            <div className={styles.scroll} data-calendar-view="month">
              <div className={styles.month}>
                {["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"].map(
                  (day) => (
                    <div key={day} className={styles.dayHeading}>
                      {day}
                    </div>
                  ),
                )}
                {Array.from(
                  {
                    length:
                      Math.ceil(
                        ((new Date(`${data.from}T12:00:00Z`).getUTCDay() + 6) %
                          7) /
                          7 +
                          days.length / 7,
                      ) * 7,
                  },
                  (_, index) => addDays(rosterMonday(data.from), index),
                ).map((day) => (
                  <section
                    key={day}
                    data-calendar-date={day}
                    className={`${styles.monthDay} ${day.slice(0, 7) !== data.from.slice(0, 7) ? styles.outside : ""} ${day === today(q.timezone) ? styles.today : ""} ${dateDrop === day ? styles.dateDropTarget : ""}`}
                  >
                    <header>{label(day, { day: "numeric" })}</header>
                    {daily(day).map((i) => event(i))}
                  </section>
                ))}
              </div>
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
                    <div className={styles.agendaItems}>
                      {daily(day).map((i) => event(i, true))}
                    </div>
                  </section>
                ))}
            </div>
          )}
          {!data.items.length &&
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
              <div className={styles.agendaItems}>
                {unscheduled.map((i) => event(i, true))}
              </div>
            </details>
          )}
          <div className={styles.footer}>
            {data.nextCursor !== null && (
              <button
                className={styles.button}
                disabled={busy}
                onClick={() =>
                  void load({ ...qRef.current, cursor: data.nextCursor! }, true)
                }
              >
                Load more events
              </button>
            )}
            <span>
              {busy
                ? "Refreshing…"
                : "Source records stay up to date when you return to this page."}
            </span>
          </div>
        </section>
      </div>
      <Dialog
        open={!!selected && !editor}
        onClose={closeDetail}
        title={selected?.title ?? "Event"}
        presentation="responsive-panel"
      >
        {selected && (
          <div className={styles.detail}>
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
                    ? new Intl.DateTimeFormat("en", {
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
                    <a
                      href={selected.details.meetingUrl}
                      target="_blank"
                      rel="noreferrer"
                    >
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
              {selected.permittedActions.includes("roster.open") && (
                <button
                  className={styles.primary}
                  onClick={() => void openRoster(selected)}
                >
                  Open roster actions
                </button>
              )}
              {selected.href && selected.source !== "roster" && (
                <Link
                  className={styles.button}
                  href={`${base}${selected.href}`}
                >
                  {selected.source === "support" ? "Open support case" : selected.source === "crm" ? "Open CRM record" : selected.source === "customer_followups" ? "Open customer follow-up" : selected.source === "supplier_bills" ? "Open supplier invoice review" : selected.source === "procurement_rfqs" ? "Open supplier quotation review" : selected.source === "supplier_promises" ? "Open supplier confirmation" : selected.source === "stock_transit" ? "Open transport record" : selected.source === "quality_holds" ? "Open quality hold" : selected.source === "supplier_batches" ? "Open supplier payment batch" : selected.source === "refunds" ? "Open refund obligation" : selected.source === "leave"
                    ? "Open leave request"
                    : "Open lifecycle case"}
                </Link>
              )}
              {selected.permittedActions.map((action) =>
                action === "appointment.save" ||
                action === "announcement.save" ||
                action === "task.plan" ? (
                  <button
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
                  </button>
                ) : action.startsWith("appointment.") ||
                  action.startsWith("announcement.") ||
                  action === "task.complete" ? (
                  <button
                    key={action}
                    className={styles.button}
                    disabled={busy}
                    onClick={() =>
                      void send({
                        action:
                          action === "task.complete"
                            ? action
                            : `${action.split(".")[0]}.status`,
                        id: selected.sourceId,
                        expectedVersion: selected.version,
                        ...(action !== "task.complete"
                          ? { status: action.split(".")[1] }
                          : {}),
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
                  </button>
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
                      <time>{new Date(h.at).toLocaleString()}</time>
                    </li>
                  ))}
                </ul>
              </>
            )}
          </div>
        )}
      </Dialog>
      <Dialog
        open={!!editor}
        onClose={() => setEditor(null)}
        title={
          editor?.kind === "appointment"
            ? editor.item
              ? "Edit appointment"
              : "New appointment"
            : editor?.kind === "announcement"
              ? "Announcement"
              : "Assign & schedule task"
        }
      >
        {editor && (
          <>
            {error && (
              <div className={styles.error} role="alert">
                {error}
              </div>
            )}
            <CalendarForm
              key={`${editor.kind}-${editor.item?.id ?? "new"}`}
              editor={editor}
              hierarchy={hierarchy}
              date={q.date}
              selfPersonId={data.selfPersonId}
              busy={busy}
              send={(value) => void send(value)}
              close={() => setEditor(null)}
            />
          </>
        )}
      </Dialog>
      <Dialog
        open={!!pendingShift}
        onClose={() => {
          if (pendingShift) replaceItem(pendingShift.item);
          setPendingShift(null);
        }}
        title="Explain published roster change"
      >
        {pendingShift && <form
          className={styles.form}
          onSubmit={(formEvent) => {
            formEvent.preventDefault();
            const reason = String(new FormData(formEvent.currentTarget).get("reason") ?? "").trim();
            if (reason.length < 3) { setError("Explain the change to this published roster."); return; }
            const pending = pendingShift;
            setPendingShift(null);
            rescheduleTimed(pending.item, pending.change, reason);
          }}
        >
          {error && <div className={styles.error} role="alert">{error}</div>}
          <p>The shift stays at the proposed time while you provide the required audit reason.</p>
          <label>Reason<textarea name="reason" required minLength={3} maxLength={1000} autoFocus /></label>
          <div className={styles.actions}>
            <button type="button" className={styles.button} onClick={() => { replaceItem(pendingShift.item); setPendingShift(null); }}>Cancel</button>
            <button type="submit" className={styles.primary}>Save change</button>
          </div>
        </form>}
      </Dialog>
      <Dialog
        open={!!roster}
        onClose={() => {
          setRoster(null);
          void load({ ...qRef.current, cursor: 0 });
        }}
        title="Roster planning"
        presentation="fullscreen"
      >
        {roster && <RosterWorkspace base={base} initial={roster} initialDate={rosterDate} embedded />}
      </Dialog>
    </div>
  );
}

export function CalendarWorkspace(props: Parameters<typeof CalendarView>[0]) {
  return <><PageHeader title="Calendar" description="See your schedule, team plans and coverage in one place."/><FeatureTabs/><CalendarView {...props}/></>;
}
