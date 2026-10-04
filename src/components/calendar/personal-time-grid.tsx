"use client";
import { Button } from "@/components/ui/foundation";

import { useEffect, useRef, useState, type PointerEvent as ReactPointerEvent } from "react";
import type { CalendarItem } from "@/lib/calendar";
import { addDays } from "@/lib/people-roster";
import { localDateTimeToInstant, localMinuteAt } from "@/lib/calendar-timeline";
import styles from "./personal-time-grid.module.css";

type Range = { startsAt: number; endsAt: number };
type Gesture = {
  item?: CalendarItem;
  mode: "create" | "move" | "start" | "end";
  day: string;
  minute: number;
  original?: Range;
  next?: Range;
};
const STEP = 15,
  DAY = 1440;
const minuteAt = (event: ReactPointerEvent, element: HTMLElement) =>
  Math.min(
    DAY - STEP,
    Math.max(0, Math.floor((event.clientY - element.getBoundingClientRect().top) / 24) * STEP),
  );
const time = (minutes: number) =>
  `${String(Math.floor(minutes / 60)).padStart(2, "0")}:${String(minutes % 60).padStart(2, "0")}`;
const instant = (date: string, minute: number, zone: string) =>
  localDateTimeToInstant(`${date}T${time(minute)}`, zone);
function range(date: string, from: number, to: number, zone: string): Range | null {
  try {
    return {
      startsAt: instant(date, from, zone),
      endsAt: instant(addDays(date, Math.floor(to / DAY)), to % DAY, zone),
    };
  } catch {
    return null;
  }
}
export function PersonalTimeGrid({
  days,
  zone,
  items,
  onOpen,
  onCreate,
  onReschedule,
}: {
  days: string[];
  zone: string;
  items: CalendarItem[];
  onOpen: (item: CalendarItem) => void;
  onCreate: (date: string, start: string, end: string) => void;
  onReschedule: (item: CalendarItem, change: Range) => void;
}) {
  const gesture = useRef<Gesture | null>(null),
    [preview, setPreview] = useState<Gesture | null>(null);
  const viewport = useRef<HTMLDivElement>(null);
  const suppressEventClick = useRef(false);
  const dayKey = days.join(",");
  useEffect(() => {
    if (viewport.current) viewport.current.scrollTop = 7 * 96;
  }, [dayKey]);
  const suppressAllDayClick = useRef(false);
  const beginAllDay = (
    event: ReactPointerEvent,
    item: CalendarItem,
    mode: "move" | "start" | "end",
  ) => {
    if (
      !item.startDate ||
      !item.endDate ||
      !item.permittedActions.includes("appointment.reschedule")
    )
      return;
    event.stopPropagation();
    const origin = (event.currentTarget as HTMLElement).closest<HTMLElement>("[data-all-day-date]")
      ?.dataset.allDayDate;
    if (!origin) return;
    let target = origin,
      moved = false;
    const onMove = (pointer: PointerEvent) => {
      const next = document
        .elementFromPoint(pointer.clientX, pointer.clientY)
        ?.closest<HTMLElement>("[data-all-day-date]")?.dataset.allDayDate;
      if (next) {
        target = next;
        moved ||= target !== origin;
      }
    };
    const finish = () => {
      window.removeEventListener("pointermove", onMove);
      window.removeEventListener("pointerup", finish);
      if (!moved) return;
      suppressAllDayClick.current = true;
      window.setTimeout(() => {
        suppressAllDayClick.current = false;
      }, 0);
      const delta = Math.round((Date.parse(target) - Date.parse(origin)) / 86400000);
      const start = mode === "end" ? item.startDate! : addDays(item.startDate!, delta);
      const end = mode === "start" ? item.endDate! : addDays(item.endDate!, delta);
      const checkedEnd = end <= start ? addDays(start, 1) : end;
      const checkedStart = start >= checkedEnd ? addDays(checkedEnd, -1) : start;
      try {
        onReschedule(item, {
          startsAt: instant(checkedStart, 0, item.timezone),
          endsAt: instant(checkedEnd, 0, item.timezone),
        });
      } catch {
        /* Use the form for an invalid local midnight. */
      }
    };
    window.addEventListener("pointermove", onMove);
    window.addEventListener("pointerup", finish, { once: true });
  };
  const pointerMinute = (event: ReactPointerEvent) => {
    const column = (event.target as HTMLElement).closest<HTMLElement>("[data-time-day]");
    return column ? { date: column.dataset.timeDay!, minute: minuteAt(event, column) } : null;
  };
  const startCreate = (event: ReactPointerEvent) => {
    if ((event.target as HTMLElement).closest("[data-time-event]")) return;
    const point = pointerMinute(event);
    if (!point) return;
    gesture.current = { mode: "create", day: point.date, minute: point.minute };
    setPreview(gesture.current);
    event.currentTarget.setPointerCapture(event.pointerId);
  };
  const startEvent = (event: ReactPointerEvent, item: CalendarItem, mode: Gesture["mode"]) => {
    if (
      !item.permittedActions.includes("appointment.reschedule") ||
      item.startsAt === null ||
      item.endsAt === null
    )
      return;
    event.stopPropagation();
    const point = pointerMinute(event);
    if (!point) return;
    gesture.current = {
      item,
      mode,
      day: point.date,
      minute: point.minute,
      original: { startsAt: item.startsAt, endsAt: item.endsAt },
    };
    event.currentTarget.closest<HTMLElement>("[data-time-day]")?.setPointerCapture(event.pointerId);
  };
  const move = (event: ReactPointerEvent) => {
    const current = gesture.current;
    if (!current) return;
    const column = document
      .elementFromPoint(event.clientX, event.clientY)
      ?.closest<HTMLElement>("[data-time-day]");
    if (!column) return;
    const point = { date: column.dataset.timeDay!, minute: minuteAt(event, column) };
    let next: Range | null = null;
    if (current.mode === "create") {
      const start =
        current.day < point.date || (current.day === point.date && current.minute <= point.minute)
          ? { date: current.day, minute: current.minute }
          : { date: point.date, minute: point.minute };
      const end =
        start.date === current.day && start.minute === current.minute
          ? point
          : { date: current.day, minute: current.minute };
      next = range(
        start.date,
        start.minute,
        Math.max(
          start.minute + STEP,
          (Date.parse(end.date) - Date.parse(start.date)) / 60000 + end.minute + STEP,
        ),
        zone,
      );
    } else if (current.original) {
      try {
        const delta =
          instant(point.date, point.minute, zone) - instant(current.day, current.minute, zone);
        if (current.mode === "move")
          next = {
            startsAt: current.original.startsAt + delta,
            endsAt: current.original.endsAt + delta,
          };
        if (current.mode === "start")
          next = {
            startsAt: Math.min(
              current.original.endsAt - STEP * 60000,
              current.original.startsAt + delta,
            ),
            endsAt: current.original.endsAt,
          };
        if (current.mode === "end")
          next = {
            startsAt: current.original.startsAt,
            endsAt: Math.max(
              current.original.startsAt + STEP * 60000,
              current.original.endsAt + delta,
            ),
          };
      } catch {
        return;
      }
    }
    if (next) {
      gesture.current = { ...current, next };
      setPreview(gesture.current);
    }
  };
  const finish = (event: ReactPointerEvent) => {
    const current = gesture.current;
    gesture.current = null;
    setPreview(null);
    if (event.currentTarget.hasPointerCapture(event.pointerId))
      event.currentTarget.releasePointerCapture(event.pointerId);
    if (!current?.next) return;
    if (current.item) {
      suppressEventClick.current = true;
      window.setTimeout(() => {
        suppressEventClick.current = false;
      }, 0);
    }
    if (current.mode === "create") {
      const start = localMinuteAt(current.next.startsAt, zone),
        end = localMinuteAt(current.next.endsAt, zone);
      onCreate(start.date, `${start.date}T${start.time}`, `${end.date}T${end.time}`);
    } else if (current.item) onReschedule(current.item, current.next);
  };
  const dateTitle = (date: string) =>
    new Intl.DateTimeFormat("en", {
      weekday: "short",
      month: "short",
      day: "numeric",
      timeZone: "UTC",
    }).format(new Date(`${date}T12:00:00Z`));
  return (
    <div ref={viewport} className={styles.viewport} data-personal-time-grid>
      <div
        className={styles.grid}
        style={{
          gridTemplateColumns: `62px repeat(${days.length}, minmax(${days.length === 1 ? "0" : "150px"}, 1fr))`,
          minWidth: days.length === 1 ? "100%" : "620px",
        }}
      >
        <div className={styles.corner}>All day</div>
        {days.map((date) => (
          <div key={`head:${date}`} data-all-day-date={date} className={styles.header}>
            <strong>{dateTitle(date)}</strong>
            <div className={styles.allDay}>
              {items
                .filter(
                  (item) =>
                    item.allDay &&
                    item.startDate &&
                    item.endDate &&
                    item.startDate <= date &&
                    item.endDate > date,
                )
                .map((item) => (
                  <Button
                    type="submit"
                    variant="plain"
                    key={item.id}
                    onClick={() => {
                      if (!suppressAllDayClick.current) onOpen(item);
                    }}
                    onPointerDown={(event) => beginAllDay(event, item, "move")}
                  >
                    {item.permittedActions.includes("appointment.reschedule") && (
                      <span
                        aria-label="Drag to change first day"
                        onPointerDown={(event) => beginAllDay(event, item, "start")}
                      >
                        ◀
                      </span>
                    )}
                    {item.title}
                    {item.permittedActions.includes("appointment.reschedule") && (
                      <span
                        aria-label="Drag to change last day"
                        onPointerDown={(event) => beginAllDay(event, item, "end")}
                      >
                        ▶
                      </span>
                    )}
                  </Button>
                ))}
            </div>
          </div>
        ))}
        <div className={styles.hours}>
          {Array.from({ length: 24 }, (_, hour) => (
            <span key={hour} style={{ top: `${hour * 96}px` }}>
              {time(hour * 60)}
            </span>
          ))}
        </div>
        {days.map((date) => (
          <div
            key={date}
            data-time-day={date}
            className={styles.column}
            onPointerDown={startCreate}
            onPointerMove={move}
            onPointerUp={finish}
            onPointerCancel={() => {
              gesture.current = null;
              setPreview(null);
            }}
          >
            {Array.from({ length: 24 }, (_, hour) => (
              <i key={hour} style={{ top: `${hour * 96}px` }} />
            ))}
            {items
              .filter((item) => !item.allDay && item.startsAt !== null && item.endsAt !== null)
              .flatMap((item) => {
                const begin =
                  preview?.item?.id === item.id && preview.next
                    ? preview.next.startsAt
                    : item.startsAt!;
                const end =
                  preview?.item?.id === item.id && preview.next
                    ? preview.next.endsAt
                    : item.endsAt!;
                let dayStart: number, dayEnd: number;
                try {
                  dayStart = instant(date, 0, zone);
                  dayEnd = instant(addDays(date, 1), 0, zone);
                } catch {
                  return [];
                }
                if (begin >= dayEnd || end <= dayStart) return [];
                const top = (Math.max(0, (begin - dayStart) / 60000) / STEP) * 24,
                  height = Math.max(
                    24,
                    ((Math.min(end, dayEnd) - Math.max(begin, dayStart)) / 60000 / STEP) * 24,
                  );
                return [
                  <Button
                    variant="plain"
                    key={item.id}
                    type="button"
                    data-time-event
                    className={styles.event}
                    style={{ top, height }}
                    onClick={(e) => {
                      e.stopPropagation();
                      if (!suppressEventClick.current) onOpen(item);
                    }}
                    onPointerDown={(e) => startEvent(e, item, "move")}
                    aria-label={`${item.title}, ${localMinuteAt(begin, zone).time} to ${localMinuteAt(end, zone).time}`}
                  >
                    {item.permittedActions.includes("appointment.reschedule") && (
                      <span
                        className={styles.handle}
                        aria-label="Drag to change start"
                        onPointerDown={(e) => startEvent(e, item, "start")}
                      />
                    )}
                    <strong>{item.title}</strong>
                    <small>
                      {localMinuteAt(begin, zone).time}–{localMinuteAt(end, zone).time}
                    </small>
                    {item.permittedActions.includes("appointment.reschedule") && (
                      <span
                        className={styles.handle}
                        aria-label="Drag to change end"
                        onPointerDown={(e) => startEvent(e, item, "end")}
                      />
                    )}
                  </Button>,
                ];
              })}
            {preview?.mode === "create" &&
              preview.next &&
              (() => {
                try {
                  const dayStart = instant(date, 0, zone),
                    dayEnd = instant(addDays(date, 1), 0, zone);
                  if (preview.next.startsAt >= dayEnd || preview.next.endsAt <= dayStart)
                    return null;
                  return (
                    <div
                      className={styles.preview}
                      style={{
                        top: `${(Math.max(0, (preview.next.startsAt - dayStart) / 60000) / STEP) * 24}px`,
                        height: `${Math.max(24, ((Math.min(preview.next.endsAt, dayEnd) - Math.max(preview.next.startsAt, dayStart)) / 60000 / STEP) * 24)}px`,
                      }}
                    >
                      New event
                    </div>
                  );
                } catch {
                  return null;
                }
              })()}
          </div>
        ))}
      </div>
      <p className={styles.help}>
        Drag an empty slot to schedule. Drag an event or either edge to change its time. Use the
        event form for keyboard editing.
      </p>
    </div>
  );
}
