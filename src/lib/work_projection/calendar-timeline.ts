import type { CalendarItem } from "@/lib/work_projection/calendar";
import { addDays } from "@/server/modules/people_payroll/public/rules/people-roster-domain";

export const TIMELINE_STEP_MS = 15 * 60_000;

function partsAt(instant: number, timeZone: string) {
  const parts = new Intl.DateTimeFormat("en-CA", {
    timeZone,
    year: "numeric",
    month: "2-digit",
    day: "2-digit",
    hour: "2-digit",
    minute: "2-digit",
    hourCycle: "h23",
  }).formatToParts(new Date(instant));
  return Object.fromEntries(parts.map((part) => [part.type, part.value]));
}

export function localMinuteAt(instant: number, timeZone: string) {
  const p = partsAt(instant, timeZone);
  return {
    date: `${p.year}-${p.month}-${p.day}`,
    time: `${p.hour}:${p.minute}`,
    minute: Number(p.hour) * 60 + Number(p.minute),
  };
}

/** Returns every instant represented by a local wall-clock minute (two on a fall-back fold). */
export function localDateTimeCandidates(local: string, timeZone: string) {
  const target = local.replace("T", " ");
  const naive = Date.parse(`${local}:00Z`);
  const offsets = new Set(
    [-36, -12, 0, 12, 36].map((hours) => {
      const instant = naive + hours * 3_600_000;
      const p = partsAt(instant, timeZone);
      return Date.UTC(Number(p.year), Number(p.month) - 1, Number(p.day), Number(p.hour), Number(p.minute)) - instant;
    }),
  );
  return [...offsets]
    .map((offset) => naive - offset)
    .filter((instant) => {
      const p = partsAt(instant, timeZone);
      return `${p.year}-${p.month}-${p.day} ${p.hour}:${p.minute}` === target;
    })
    .sort((a, b) => a - b);
}

export function localDateTimeToInstant(
  local: string,
  timeZone: string,
  disambiguation: "earlier" | "later" = "earlier",
) {
  const candidates = localDateTimeCandidates(local, timeZone);
  if (!candidates.length) throw new Error("This local time does not exist in the selected timezone.");
  return disambiguation === "later" ? candidates.at(-1)! : candidates[0];
}

export function timelineDay(date: string, timeZone: string) {
  const start = localDateTimeToInstant(`${date}T00:00`, timeZone);
  const end = localDateTimeToInstant(`${addDays(date, 1)}T00:00`, timeZone);
  const slots = Array.from({ length: Math.round((end - start) / TIMELINE_STEP_MS) }, (_, index) => {
    const instant = start + index * TIMELINE_STEP_MS;
    return { instant, ...localMinuteAt(instant, timeZone) };
  });
  const hourCounts = new Map<string, number>();
  for (const slot of slots.filter((slot) => slot.time.endsWith(":00")))
    hourCounts.set(slot.time, (hourCounts.get(slot.time) ?? 0) + 1);
  const zoneName = (instant: number) =>
    new Intl.DateTimeFormat("en", { timeZone, timeZoneName: "shortOffset" })
      .formatToParts(new Date(instant))
      .find((part) => part.type === "timeZoneName")?.value ?? "";
  return {
    date,
    timeZone,
    start,
    end,
    duration: end - start,
    slots,
    hours: slots
      .filter((slot) => slot.time.endsWith(":00"))
      .map((slot) => ({
        ...slot,
        label: (hourCounts.get(slot.time) ?? 0) > 1 ? `${slot.time} ${zoneName(slot.instant)}` : slot.time,
        percent: ((slot.instant - start) / (end - start)) * 100,
      })),
  };
}

export function snapInstant(instant: number, origin: number, mode: "round" | "floor" | "ceil" = "round") {
  return origin + Math[mode]((instant - origin) / TIMELINE_STEP_MS) * TIMELINE_STEP_MS;
}

export function clipToDay(startsAt: number, endsAt: number, start: number, end: number) {
  const clippedStart = Math.max(startsAt, start), clippedEnd = Math.min(endsAt, end);
  if (clippedEnd <= clippedStart) return null;
  return {
    start: clippedStart,
    end: clippedEnd,
    clippedStart: startsAt < start,
    clippedEnd: endsAt > end,
    left: ((clippedStart - start) / (end - start)) * 100,
    width: ((clippedEnd - clippedStart) / (end - start)) * 100,
  };
}

export type TimelinePlacement = ReturnType<typeof clipToDay> & { item: CalendarItem; lane: number; laneCount: number };

export function placeTimelineItems(items: CalendarItem[], start: number, end: number): TimelinePlacement[] {
  const clipped = items
    .flatMap((item) => {
      if (item.startsAt === null || item.endsAt === null) return [];
      const geometry = clipToDay(item.startsAt, item.endsAt, start, end);
      return geometry ? [{ item, ...geometry }] : [];
    })
    .sort((a, b) => a.start - b.start || b.end - a.end || a.item.id.localeCompare(b.item.id));
  const groups: typeof clipped[] = [];
  let group: typeof clipped = [], groupEnd = -Infinity;
  for (const entry of clipped) {
    if (group.length && entry.start >= groupEnd) { groups.push(group); group = []; groupEnd = -Infinity; }
    group.push(entry); groupEnd = Math.max(groupEnd, entry.end);
  }
  if (group.length) groups.push(group);
  return groups.flatMap((entries) => {
    const laneEnds: number[] = [];
    const assigned = entries.map((entry) => {
      let lane = laneEnds.findIndex((laneEnd) => laneEnd <= entry.start);
      if (lane < 0) lane = laneEnds.length;
      laneEnds[lane] = entry.end;
      return { ...entry, lane };
    });
    return assigned.map((entry) => ({ ...entry, laneCount: laneEnds.length })) as TimelinePlacement[];
  });
}

export function moveInclusiveRange(startDate: string, endDate: string, targetStart: string) {
  const days = Math.round((Date.parse(endDate) - Date.parse(startDate)) / 86_400_000);
  return { startDate: targetStart, endDate: addDays(targetStart, days) };
}
