export type RoutineRule = {
  frequency: "daily" | "weekly" | "monthly";
  timezone: string;
  occurrence_time: string;
  start_date: string;
  end_date: string | null;
  weekday: number | null;
  month_day: number | null;
};
function parts(timestamp: number, timezone: string) {
  return Object.fromEntries(
    new Intl.DateTimeFormat("en-CA", {
      timeZone: timezone,
      year: "numeric",
      month: "2-digit",
      day: "2-digit",
      hour: "2-digit",
      minute: "2-digit",
      second: "2-digit",
      hourCycle: "h23",
    })
      .formatToParts(timestamp)
      .filter((x) => x.type !== "literal")
      .map((x) => [x.type, x.value]),
  );
}
function localDate(timestamp: number, timezone: string) {
  const p = parts(timestamp, timezone);
  return `${p.year}-${p.month}-${p.day}`;
}
/** One local calendar occurrence, including DST. Gaps move forward; overlaps use the first occurrence. */
export function zonedTime(date: string, time: string, timezone: string) {
  const desired = Date.parse(`${date}T${time}:00Z`);
  let candidate = desired;
  const seen: number[] = [];
  for (let i = 0; i < 6; i++) {
    const p = parts(candidate, timezone),
      asUtc = Date.parse(
        `${p.year}-${p.month}-${p.day}T${p.hour}:${p.minute}:${p.second}Z`,
      ),
      next = candidate + desired - asUtc;
    if (next === candidate) {
      // A backward clock change may have two matches, but a schedule generates only one task.
      for (const delta of [7200000, 3600000, 1800000]) {
        const earlier = candidate - delta;
        if (localDate(earlier, timezone) === date) {
          const e = parts(earlier, timezone);
          if (`${e.hour}:${e.minute}` === time) return earlier;
        }
      }
      return candidate;
    }
    if (seen.includes(next)) return Math.max(candidate, next);
    seen.push(candidate);
    candidate = next;
  }
  throw new Error("TIMEZONE_OCCURRENCE_UNRESOLVED");
}
export function nextOccurrence(
  rule: RoutineRule,
  after: number,
): number | null {
  let date = localDate(after, rule.timezone);
  if (date < rule.start_date) date = rule.start_date;
  const anchor = new Date(`${rule.start_date}T00:00:00Z`),
    weekday = rule.weekday ?? anchor.getUTCDay(),
    monthDay = rule.month_day ?? anchor.getUTCDate();
  for (let i = 0; i < 370; i++) {
    if (rule.end_date && date > rule.end_date) return null;
    const d = new Date(`${date}T00:00:00Z`),
      last = new Date(
        Date.UTC(d.getUTCFullYear(), d.getUTCMonth() + 1, 0),
      ).getUTCDate();
    const matches =
      rule.frequency === "daily" ||
      (rule.frequency === "weekly"
        ? d.getUTCDay() === weekday
        : d.getUTCDate() === Math.min(monthDay, last));
    if (matches) {
      const timestamp = zonedTime(date, rule.occurrence_time, rule.timezone);
      if (timestamp > after) return timestamp;
    }
    d.setUTCDate(d.getUTCDate() + 1);
    date = d.toISOString().slice(0, 10);
  }
  throw new Error("SCHEDULE_SEARCH_EXHAUSTED");
}
