"use client";
import { Button } from "@/components/ui/foundation";

import {
  useEffect,
  useMemo,
  useRef,
  useState,
  type CSSProperties,
  type PointerEvent as ReactPointerEvent,
} from "react";
import type { CalendarItem, CalendarPerson } from "@/lib/work_projection/calendar";
import {
  localDateTimeToInstant,
  localMinuteAt,
  placeTimelineItems,
  snapInstant,
  timelineDay,
  TIMELINE_STEP_MS,
} from "@/lib/work_projection/calendar-timeline";
import { eventStyle } from "@/components/work_projection/calendar/calendar-event";
import styles from "@/components/work_projection/calendar/calendar.module.css";

type Change = { startsAt: number; endsAt: number };
type Drag = {
  item: CalendarItem;
  mode: "move" | "start" | "end";
  x: number;
  startsAt: number;
  endsAt: number;
  next: Change;
  moved: boolean;
};

export function CalendarDayTimeline({
  date,
  zone,
  items,
  people,
  onOpen,
  onReschedule,
}: {
  date: string;
  zone: string;
  items: CalendarItem[];
  people: CalendarPerson[];
  onOpen: (item: CalendarItem) => void;
  onReschedule: (item: CalendarItem, change: Change) => void;
}) {
  const day = useMemo(() => timelineDay(date, zone), [date, zone]);
  const viewport = useRef<HTMLDivElement>(null);
  const grid = useRef<HTMLDivElement>(null);
  const dragRef = useRef<Drag | null>(null);
  const suppressClick = useRef(false);
  const [drag, setDrag] = useState<Drag | null>(null);
  const width = day.slots.length * 32;
  const timed = useMemo(
    () => items.filter((item) => !item.allDay && item.startsAt !== null && item.endsAt !== null),
    [items],
  );
  const allDay = useMemo(() => items.filter((item) => item.allDay), [items]);
  const rows = [
    { id: "team", name: "Team & open", person: null as CalendarPerson | null },
    ...people.map((person) => ({ id: person.id, name: person.name, person })),
  ];
  const rowItems = (person: CalendarPerson | null, source: CalendarItem[]) =>
    source.filter((item) =>
      person
        ? item.participants.some((participant) => participant.id === person.id) ||
          (item.kind === "task" && !!person.userId && item.details?.assigneeId === person.userId)
        : item.kind === "announcement" ||
          !item.participants.length ||
          !people.some((candidate) =>
            item.participants.some((participant) => participant.id === candidate.id),
          ) ||
          (item.capacity ?? 0) > (item.assigned ?? 0),
    );

  useEffect(() => {
    const element = viewport.current;
    if (!element) return;
    const now = Date.now();
    const gridElement = grid.current;
    if (gridElement) {
      const visible = now >= day.start && now < day.end;
      gridElement.style.setProperty("--now-left", `${((now - day.start) / day.duration) * 100}%`);
      gridElement.style.setProperty("--now-visibility", visible ? "visible" : "hidden");
    }
    let target =
      now >= day.start && now < day.end
        ? now
        : timed.reduce((value, item) => Math.min(value, item.startsAt!), Infinity);
    if (!Number.isFinite(target)) target = localDateTimeToInstant(`${date}T08:00`, zone);
    element.scrollLeft = Math.max(0, ((target - day.start) / day.duration) * width - 180);
  }, [date, day.duration, day.end, day.start, timed, width, zone]);

  useEffect(() => {
    const cancel = (event: KeyboardEvent) => {
      if (event.key === "Escape" && dragRef.current) {
        dragRef.current = null;
        setDrag(null);
      }
    };
    window.addEventListener("keydown", cancel);
    return () => window.removeEventListener("keydown", cancel);
  }, []);

  const begin = (item: CalendarItem, mode: Drag["mode"], event: ReactPointerEvent) => {
    if (!item.permittedActions.includes(`${item.kind}.reschedule`)) return;
    const next = { startsAt: item.startsAt!, endsAt: item.endsAt! };
    dragRef.current = { item, mode, x: event.clientX, ...next, next, moved: false };
  };
  const move = (event: ReactPointerEvent) => {
    const current = dragRef.current;
    if (!current) return;
    event.preventDefault();
    const raw = ((event.clientX - current.x) / width) * day.duration;
    const delta = snapInstant(day.start + raw, day.start) - day.start;
    const moved = current.moved || Math.abs(event.clientX - current.x) >= 5;
    if (moved && !event.currentTarget.hasPointerCapture(event.pointerId))
      event.currentTarget.setPointerCapture(event.pointerId);
    let startsAt = current.startsAt,
      endsAt = current.endsAt;
    if (current.mode === "move") {
      startsAt += delta;
      endsAt += delta;
      if (startsAt < day.start) {
        endsAt += day.start - startsAt;
        startsAt = day.start;
      }
      if (endsAt > day.end) {
        startsAt -= endsAt - day.end;
        endsAt = day.end;
      }
    } else if (current.mode === "start")
      startsAt = Math.min(endsAt - TIMELINE_STEP_MS, Math.max(day.start, startsAt + delta));
    else endsAt = Math.max(startsAt + TIMELINE_STEP_MS, Math.min(day.end, endsAt + delta));
    const next = { ...current, next: { startsAt, endsAt }, moved };
    dragRef.current = next;
    setDrag(next);
  };
  const end = (event: ReactPointerEvent) => {
    const current = dragRef.current;
    if (!current) return;
    if (event.currentTarget.hasPointerCapture(event.pointerId))
      event.currentTarget.releasePointerCapture(event.pointerId);
    dragRef.current = null;
    setDrag(null);
    if (current.moved) {
      suppressClick.current = true;
      onReschedule(current.item, current.next);
      window.setTimeout(() => {
        suppressClick.current = false;
      }, 0);
    }
  };
  const cancel = () => {
    dragRef.current = null;
    setDrag(null);
  };
  const format = (instant: number) => localMinuteAt(instant, zone).time;

  return (
    <div
      ref={viewport}
      className={`${styles.scroll} ${styles.dayViewport}`}
      data-calendar-view="day"
      tabIndex={0}
      role="region"
      aria-label="Daily people timeline"
    >
      <div
        ref={grid}
        className={styles.dayGrid}
        style={{ "--timeline-width": `${width}px`, "--now-visibility": "hidden" } as CSSProperties}
      >
        <div className={`${styles.dayPersonCell} ${styles.dayCorner}`}>People</div>
        <div className={styles.timeRuler}>
          {day.hours.map((hour) => (
            <span key={hour.instant} style={{ left: `${hour.percent}%` }}>
              {hour.label}
            </span>
          ))}
          {day.slots.map((slot) => (
            <i
              key={slot.instant}
              style={{ left: `${((slot.instant - day.start) / day.duration) * 100}%` }}
            />
          ))}
        </div>
        {rows.map((row) => {
          const placements = placeTimelineItems(rowItems(row.person, timed), day.start, day.end);
          const rail = rowItems(row.person, allDay);
          const laneCount = Math.max(1, ...placements.map((placement) => placement.laneCount));
          return (
            <div className={styles.dayRowGroup} key={row.id}>
              <div className={styles.dayPersonCell}>
                <span className={styles.avatar}>{row.name[0]}</span>
                <strong>{row.name}</strong>
                {rail.length > 0 && (
                  <div className={styles.allDayRail}>
                    {rail.slice(0, 3).map((item) => (
                      <Button
                        type="submit"
                        variant="plain"
                        key={item.id}
                        style={eventStyle(item.source)}
                        onClick={() => onOpen(item)}
                      >
                        {item.title}
                      </Button>
                    ))}
                    {rail.length > 3 && <small>+{rail.length - 3}</small>}
                  </div>
                )}
              </div>
              <div
                className={styles.timelineTrack}
                style={{ height: `${Math.max(54, laneCount * 34 + 12)}px` }}
              >
                {day.slots.map((slot) => (
                  <i
                    key={slot.instant}
                    style={{ left: `${((slot.instant - day.start) / day.duration) * 100}%` }}
                  />
                ))}
                <span className={styles.nowMarker} />
                {placements.map((placement) => {
                  const active = drag?.item.id === placement.item.id;
                  const change = active
                    ? drag.next
                    : { startsAt: placement.start, endsAt: placement.end };
                  const left =
                    ((Math.max(change.startsAt, day.start) - day.start) / day.duration) * 100;
                  const barWidth =
                    ((Math.min(change.endsAt, day.end) - Math.max(change.startsAt, day.start)) /
                      day.duration) *
                    100;
                  const movable =
                    placement.item.permittedActions.includes("appointment.reschedule") ||
                    placement.item.permittedActions.includes("shift.reschedule");
                  return (
                    <Button
                      variant="plain"
                      key={placement.item.id}
                      type="button"
                      className={`${styles.timelineBar} ${active ? styles.dragging : ""}`}
                      style={{
                        ...eventStyle(placement.item.source),
                        left: `${left}%`,
                        width: `${barWidth}%`,
                        top: `${6 + placement.lane * 34}px`,
                      }}
                      data-calendar-event
                      data-reschedulable={movable || undefined}
                      aria-label={`${placement.item.title}, ${format(change.startsAt)} to ${format(change.endsAt)}`}
                      onClick={() => {
                        if (!suppressClick.current) onOpen(placement.item);
                      }}
                      onPointerDown={(event) => {
                        if (movable) begin(placement.item, "move", event);
                      }}
                      onPointerMove={move}
                      onPointerUp={end}
                      onPointerCancel={cancel}
                    >
                      {movable && (
                        <span
                          className={styles.resizeHandle}
                          data-edge="start"
                          onPointerDown={(event) => {
                            event.stopPropagation();
                            begin(placement.item, "start", event);
                          }}
                        />
                      )}
                      <small>
                        {format(change.startsAt)}–{format(change.endsAt)}
                      </small>
                      <strong>{placement.item.title}</strong>
                      {movable && (
                        <span
                          className={styles.resizeHandle}
                          data-edge="end"
                          onPointerDown={(event) => {
                            event.stopPropagation();
                            begin(placement.item, "end", event);
                          }}
                        />
                      )}
                      {active && drag.moved && (
                        <span className={styles.dragTooltip}>
                          {format(change.startsAt)}–{format(change.endsAt)}
                        </span>
                      )}
                    </Button>
                  );
                })}
              </div>
            </div>
          );
        })}
      </div>
    </div>
  );
}
