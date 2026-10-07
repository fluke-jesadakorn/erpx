import { z } from "zod";
import { dateSchema } from "./model";
export function previewDeadline(raw: unknown) {
  const x = z
    .object({
      start: dateSchema,
      amount: z.coerce.number().int().min(0).max(3660),
      unit: z.enum(["calendar_days", "business_days", "months"]),
      holidays: z.array(dateSchema).max(1000).default([]),
      weekdays: z
        .array(z.number().int().min(0).max(6))
        .min(1)
        .max(7)
        .default([1, 2, 3, 4, 5]),
    })
    .parse(raw);
  const date = new Date(`${x.start}T12:00:00Z`),
    holidays = new Set(x.holidays),
    weekdays = new Set(x.weekdays);
  let skipped = 0;
  if (x.unit === "months") {
    const day = date.getUTCDate();
    date.setUTCDate(1);
    date.setUTCMonth(date.getUTCMonth() + x.amount);
    const last = new Date(
      Date.UTC(date.getUTCFullYear(), date.getUTCMonth() + 1, 0),
    ).getUTCDate();
    date.setUTCDate(Math.min(day, last));
  } else
    for (let n = 0; n < x.amount;) {
      date.setUTCDate(date.getUTCDate() + 1);
      if (
        x.unit === "calendar_days" ||
        (weekdays.has(date.getUTCDay()) &&
          !holidays.has(date.toISOString().slice(0, 10)))
      )
        n++;
      else skipped++;
    }
  return {
    date: date.toISOString().slice(0, 10),
    inputs: x,
    skipped,
    requiresReview: true,
    convention:
      "Exclude the start date; month arithmetic clamps to the last day. Dates are local calendar dates. Confirm the governing rule and holiday list before assigning this deadline.",
  };
}
