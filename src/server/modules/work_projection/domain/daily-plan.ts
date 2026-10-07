import {z} from "zod";
import type {WorkItem} from "@/lib/work-item";
import type {WorkSnapshot} from "./work-items";
const referenceSchema = z.object({
  type: z.string().min(1).max(80),
  id: z.string().min(1).max(200),
  variant: z.enum(["record", "override"]).default("record"),
}).strict();

const plannedItemSchema = referenceSchema.extend({
  startMinute: z.number().int().min(0).max(1439).nullable().default(null),
  plannedMinutes: z.number().int().min(1).max(1440).nullable().default(null),
  sourceUpdatedAt: z.number().int().nonnegative(),
});

export const dailyPlanSaveSchema = z.object({
  date: z.iso.date(),
  expectedVersion: z.number().int().nonnegative(),
  items: z.array(plannedItemSchema).max(40),
}).strict().superRefine((plan, context) => {
  const seen = new Set<string>();
  for (const [index, item] of plan.items.entries()) {
    const key = dailyPlanReferenceKey(item);
    if (seen.has(key)) context.addIssue({
      code: "custom", path: ["items", index], message: "Add each work item only once per day.",
    });
    seen.add(key);
    if (item.startMinute !== null && item.plannedMinutes === null) context.addIssue({
      code: "custom", path: ["items", index, "plannedMinutes"],
      message: "Set a duration for a timed work block.",
    });
    if (item.startMinute !== null && item.plannedMinutes !== null &&
      item.startMinute + item.plannedMinutes > 1440) context.addIssue({
      code: "custom", path: ["items", index, "plannedMinutes"],
      message: "Finish the time block within the selected day.",
    });
  }
});

export type DailyPlanSave = z.infer<typeof dailyPlanSaveSchema>;
export type DailyPlanReference = Pick<DailyPlanSave["items"][number], "type" | "id" | "variant">;
export type StoredItem = DailyPlanReference & Pick<DailyPlanSave["items"][number], "startMinute" | "plannedMinutes">;
export type PlanRow = { items_json: string; version: number };
export type DailyPlanWork = Pick<WorkItem,
  "type" | "id" | "title" | "detail" | "href" | "status" | "queue" | "dueAt" | "updatedAt" | "nextAction"
> & { variant: "record" | "override" };
export type DailyPlanView = {
  date: string;
  ownerKey: string;
  version: number;
  items: (StoredItem & { sourceUpdatedAt: number; work: DailyPlanWork })[];
  choices: DailyPlanWork[];
  hiddenCount: number;
  sourcesUnavailable: boolean;
};

export function dailyPlanReferenceKey(reference: DailyPlanReference) {
  return JSON.stringify([reference.type, reference.id, reference.variant]);
}

export function visibleWork(item: WorkItem): DailyPlanWork {
  return {
    type: item.type,
    id: item.id,
    variant: item.actionKind === "override" ? "override" : "record",
    title: item.title,
    detail: item.detail,
    href: item.href,
    status: item.status,
    queue: item.queue,
    dueAt: item.dueAt ?? null,
    updatedAt: item.updatedAt,
    nextAction: item.nextAction,
  };
}

export function currentItems(snapshot: WorkSnapshot) {
  return new Map(snapshot.items.map((item) => {
    const work = visibleWork(item);
    return [dailyPlanReferenceKey(work), work] as const;
  }));
}

