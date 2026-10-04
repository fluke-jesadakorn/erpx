import { today } from "./calendar-date";
export { today } from "./calendar-date";
import { clearanceCompletion } from "./retirement";
import { z } from "zod";
import { addDays, rosterMonday } from "./people-roster";
export const calendarLayers = [
  "support",
  "crm",
  "customer_followups",
  "stock_transit",
  "supplier_promises",
  "procurement_rfqs",
  "supplier_bills",
  "quality_holds",
  "supplier_batches",
  "refunds",
  "maintenance",
  "legal",
  "roster",
  "leave",
  "appointments",
  "announcements",
  "lifecycle",
] as const;
export type CalendarLayer = (typeof calendarLayers)[number];
const id = z
  .string()
  .min(1)
  .max(180)
  .regex(/^[a-zA-Z0-9:_.-]+$/);
const timezone = z
  .string()
  .max(80)
  .refine((value) => {
    try {
      new Intl.DateTimeFormat("en", { timeZone: value });
      return true;
    } catch {
      return false;
    }
  }, "Choose an IANA timezone.");
export const calendarQuerySchema = z.object({
  date: z.iso.date().default(() => today()),
  view: z.enum(["day", "week", "month", "agenda"]).default("week"),
  hierarchy: z.enum(["organization", "reporting"]).default("organization"),
  mode: z.enum(["mine", "team"]).optional(),
  preset: z.enum(["mine","team","coverage"]).optional(),
  person: id.optional(),
  node: id.optional(),
  search: z.string().trim().max(100).default(""),
  layers: z
    .string()
    .default(calendarLayers.join(","))
    .refine(
      (v) =>
        v
          .split(",")
          .every((x) => (calendarLayers as readonly string[]).includes(x)),
      "Choose valid calendar layers.",
    ),
  timezone: timezone.default("Asia/Bangkok"),
  cursor: z.coerce.number().int().min(0).max(10000).default(0),
  limit: z.coerce.number().int().min(1).max(200).default(100),
});
export type CalendarQuery = z.infer<typeof calendarQuerySchema>;
export type CalendarNode = {
  inScope?: boolean;
  id: string;
  parentId: string | null;
  name: string;
  kind: string;
  branchId: string | null;
};
export type CalendarPerson = {
  id: string;
  name: string;
  userId: string | null;
  departmentId: string | null;
  branchId: string | null;
  managerId: string | null;
  status: string;
};
export type CalendarHistory = { action: string; actor: string; at: number };
export type CalendarItem = {
  id: string;
  source: CalendarLayer;
  sourceId: string;
  kind:
    | "shift"
    | "leave"
    | "holiday"
    | "appointment"
    | "announcement"
    | "task"
    | "start"
    | "last_day"
    | "cutoff"
    | "directory";
  title: string;
  description?: string;
  allDay: boolean;
  startDate: string | null;
  endDate: string | null;
  startsAt: number | null;
  endsAt: number | null;
  timezone: string;
  departmentId: string | null;
  branchId: string | null;
  participants: { id: string; name: string }[];
  owner: string | null;
  status: string;
  version: number;
  permittedActions: string[];
  href?: string;
  warnings: string[];
  history: CalendarHistory[];
  redacted?: boolean;
  occurrenceId?: string;
  seriesId?: string;
  myResponse?: "pending" | "accepted" | "tentative" | "declined";
  recurrence?: {
    frequency: "daily" | "weekly" | "monthly" | "yearly";
    intervalCount: number;
    weekdays: number[];
    endKind: "never" | "count" | "until";
    occurrenceCount?: number | null;
    untilDate?: string | null;
  };
  capacity?: number;
  assigned?: number;
  details?: {
    location?: string;
    meetingUrl?: string;
    guests?: { name: string; email: string; phone: string }[];
    private?: boolean;
    organizerPersonId?: string;
    attendees?: { userId: string; name: string; attendance: "organizer" | "required" | "optional"; response: "pending" | "accepted" | "tentative" | "declined" }[];
    caseId?: string;
    responsibility?: string;
    assigneeId?: string | null;
    privileged?: boolean;
    requiresReason?: boolean;
    published?: boolean;
  };
};
export type CalendarData = {
  query: CalendarQuery;
  from: string;
  to: string;
  items: CalendarItem[];
  nextCursor: number | null;
  sources: {
    source: CalendarLayer;
    status: "ready" | "error";
    message?: string;
  }[];
  coverage: {
    shifts: number;
    assigned: number;
    open: number;
    warnings: number;
    partial: boolean;
  };
  selfPersonId: string | null;
  canManageRoster: boolean;
  canSchedule: boolean;
  canAnnounce: boolean;
  teamDefault: boolean;
};
export type CalendarHierarchy = {
  nodes: CalendarNode[];
  people: CalendarPerson[];
  nextCursor: number | null;
  selfPersonId: string | null;
};
export function calendarRange(q: Pick<CalendarQuery, "date" | "view">) {
  const from =
    q.view === "week"
      ? rosterMonday(q.date)
      : q.view === "month"
        ? `${q.date.slice(0, 7)}-01`
        : q.date;
  const to =
    q.view === "month"
      ? new Date(
          Date.UTC(Number(from.slice(0, 4)), Number(from.slice(5, 7)), 1),
        )
          .toISOString()
          .slice(0, 10)
      : addDays(from, q.view === "week" ? 7 : q.view === "day" ? 1 : 30);
  return { from, to };
}
export function itemOnDate(item: CalendarItem, date: string, zone: string) {
  if (item.allDay)
    return (
      !!item.startDate &&
      item.startDate <= date &&
      (item.endDate ?? addDays(item.startDate, 1)) > date
    );
  if (item.startsAt === null || item.endsAt === null) return false;
  const fmt = (n: number) =>
    new Intl.DateTimeFormat("en-CA", {
      timeZone: zone,
      year: "numeric",
      month: "2-digit",
      day: "2-digit",
    }).format(new Date(n));
  return fmt(item.startsAt) <= date && fmt(item.endsAt - 1) >= date;
}
const localDateTime = z
  .string()
  .regex(/^\d{4}-\d{2}-\d{2}T([01]\d|2[0-3]):[0-5]\d$/)
  .refine(
    (value) => z.iso.date().safeParse(value.slice(0, 10)).success,
    "Choose a valid local date and time.",
  );
const recurrenceSchema=z.object({
  frequency:z.enum(["daily","weekly","monthly","yearly"]),
  intervalCount:z.number().int().min(1).max(99).default(1),
  weekdays:z.array(z.number().int().min(1).max(7)).max(7).default([]),
  endKind:z.enum(["never","count","until"]).default("never"),
  occurrenceCount:z.number().int().min(1).max(1000).optional(),
  untilDate:z.iso.date().optional(),
}).refine(v=>v.endKind!=="count"||v.occurrenceCount!==undefined,"Choose the number of occurrences.")
  .refine(v=>v.endKind!=="until"||v.untilDate!==undefined,"Choose a final date.");
export const appointmentInput = z.object({
  action: z.literal("appointment.save"),
  id: id.optional(),
  expectedVersion: z.number().int().positive().optional(),
  organizerPersonId: id.optional(),
  participantIds: z.array(id).max(100).default([]),
  attendees:z.array(z.object({userId:id,attendance:z.enum(["required","optional"])})).max(100).optional(),
  title: z.string().trim().min(1).max(160),
  description: z.string().trim().max(3000).default(""),
  startLocal: localDateTime.optional(),
  endLocal: localDateTime.optional(),
  allDay:z.boolean().default(false),
  startDate:z.iso.date().optional(),
  endDate:z.iso.date().optional(),
  recurrence:z.union([recurrenceSchema,z.null()]).optional(),
  timezone: timezone.default("Asia/Bangkok"),
  location: z.string().trim().max(300).default(""),
  meetingUrl: z
    .union([
      z.literal(""),
      z
        .url()
        .refine((v) => /^https?:\/\//.test(v), "Use an HTTP or HTTPS link."),
    ])
    .default(""),
  private: z.boolean().default(true),
  guests: z
    .array(
      z.object({
        name: z.string().trim().min(1).max(120),
        email: z.union([z.literal(""), z.email()]).default(""),
        phone: z.string().trim().max(80).default(""),
      }),
    )
    .max(30)
    .default([]),
}).refine(v=>v.allDay?Boolean(v.startDate&&v.endDate&&v.endDate>v.startDate):Boolean(v.startLocal&&v.endLocal),
  "Choose a valid start and end.");
export const announcementInput = z
  .object({
    action: z.literal("announcement.save"),
    id: id.optional(),
    expectedVersion: z.number().int().positive().optional(),
    audienceId: id,
    title: z.string().trim().min(1).max(160),
    content: z.string().trim().min(1).max(5000),
    startDate: z.iso.date(),
    endDate: z.iso.date(),
  })
  .refine(
    (v) => v.endDate >= v.startDate,
    "The visible end date must follow the start date.",
  );
export const calendarCommandSchema = z.union([
  appointmentInput,
  announcementInput,
  z.object({
    action: z.literal("appointment.reschedule"),
    id,
    expectedVersion: z.number().int().positive(),
    startLocal: localDateTime,
    endLocal: localDateTime,
    timezone,
    occurrenceId:localDateTime.optional(),
    scope:z.enum(["single","future","series"]).optional(),
  }),
  z
    .object({
      action: z.literal("announcement.reschedule"),
      id,
      expectedVersion: z.number().int().positive(),
      startDate: z.iso.date(),
      endDate: z.iso.date(),
    })
    .refine((v) => v.endDate >= v.startDate, "The visible end date must follow the start date."),
  z.object({
    action: z.literal("task.reschedule"),
    id,
    expectedVersion: z.number().int().positive(),
    dueDate: z.iso.date(),
  }),
  z.object({
    action: z.literal("shift.reschedule"),
    id,
    expectedVersion: z.number().int().positive(),
    date: z.iso.date(),
    startTime: z.string().regex(/^([01]\d|2[0-3]):[0-5]\d$/),
    endTime: z.string().regex(/^([01]\d|2[0-3]):[0-5]\d$/),
    reason: z.string().trim().max(1000).default(""),
  }),
  z.object({
    action: z.literal("appointment.status"),
    id,
    expectedVersion: z.number().int().positive(),
    status: z.enum(["cancelled", "completed", "no_show"]),
    occurrenceId:localDateTime.optional(),
    scope:z.enum(["single","future","series"]).optional(),
  }),
  z.object({action:z.literal("appointment.respond"),id,expectedVersion:z.number().int().positive().optional(),
    response:z.enum(["accepted","tentative","declined"]),occurrenceId:localDateTime.optional(),
    scope:z.enum(["single","series"]).default("series")}),
  z.object({action:z.literal("appointment.reminder.set"),id,expectedVersion:z.number().int().positive().optional(),
    minutes:z.number().int().min(0).max(10080).nullable()}),
  z.object({
    action: z.literal("announcement.status"),
    id,
    expectedVersion: z.number().int().positive(),
    status: z.enum(["published", "withdrawn"]),
  }),
  z.object({
    action: z.literal("task.plan"),
    id,
    expectedVersion: z.number().int().positive(),
    dueDate: z.iso.date().nullable(),
    assigneeId: id.nullable(),
    responsibility: z.string().trim().max(300),
  }),
  z.object({
    action: z.literal("task.complete"),
    ...clearanceCompletion,
    id,
    expectedVersion: z.number().int().positive(),
  }),
]);
export type CalendarCommand = z.infer<typeof calendarCommandSchema>;
