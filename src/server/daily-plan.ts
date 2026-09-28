import "server-only";

import { z } from "zod";
import type { WorkItem } from "@/lib/work-item";
import type { WorkContext } from "./authorization";
import { assertion, executeCommand, replayCommand } from "./commands";
import { ApiError } from "./http";
import type { SqlDatabase } from "./runtime/types";
import type { WorkSnapshot } from "./work-items";

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
type StoredItem = DailyPlanReference & Pick<DailyPlanSave["items"][number], "startMinute" | "plannedMinutes">;
type PlanRow = { items_json: string; version: number };
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

function visibleWork(item: WorkItem): DailyPlanWork {
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

function currentItems(snapshot: WorkSnapshot) {
  return new Map(snapshot.items.map((item) => {
    const work = visibleWork(item);
    return [dailyPlanReferenceKey(work), work] as const;
  }));
}

/** The plan contains references only. Each read resolves current, authorized work. */
export async function readDailyPlan(
  db: SqlDatabase,
  actor: Pick<WorkContext, "companyId" | "userId">,
  date: string,
  snapshot: WorkSnapshot,
): Promise<DailyPlanView> {
  z.iso.date().parse(date);
  const row = await db.prepare(
    "SELECT items_json,version FROM staff_daily_plans WHERE company_id=? AND user_id=? AND plan_date=?",
  ).bind(actor.companyId, actor.userId, date).first<PlanRow>();
  const stored = row ? JSON.parse(row.items_json) as StoredItem[] : [];
  const visible = currentItems(snapshot);
  const items = stored.flatMap((item) => {
    const work = visible.get(dailyPlanReferenceKey(item));
    return work ? [{ ...item, sourceUpdatedAt: work.updatedAt, work }] : [];
  });
  const choices = [...visible.values()]
    .filter((work) => work.queue !== "completed");
  return {
    date,
    ownerKey: JSON.stringify([actor.companyId, actor.userId]),
    version: row?.version ?? 0,
    items,
    choices,
    hiddenCount: stored.length - items.length,
    sourcesUnavailable: snapshot.unavailableSources.length > 0,
  };
}

export async function saveDailyPlan(
  db: SqlDatabase,
  actor: WorkContext,
  raw: unknown,
  commandKey: string,
  snapshot: WorkSnapshot,
) {
  const input = dailyPlanSaveSchema.parse(raw);
  const recordId = `daily-plan:${actor.companyId}:${actor.userId}:${input.date}`;
  const prior = await replayCommand<{ date: string; version: number }>(
    db, actor, commandKey, "daily.plan.save", recordId, input,
  );
  if (prior) return prior;
  const visible = currentItems(snapshot);
  for (const item of input.items) {
    const current = visible.get(dailyPlanReferenceKey(item));
    if (!current) throw new ApiError(
      snapshot.unavailableSources.length ? 503 : 403,
      snapshot.unavailableSources.length ? "PLAN_SOURCE_UNAVAILABLE" : "PLAN_ITEM_UNAVAILABLE",
      snapshot.unavailableSources.length
        ? "A work source is temporarily unavailable. Try saving again when it returns."
        : "A planned work item is no longer available to you.",
    );
    if (current.updatedAt !== item.sourceUpdatedAt) throw new ApiError(
      409, "PLAN_ITEM_CHANGED", "A planned work item changed. Reload the plan before saving.",
    );
  }
  const stored: StoredItem[] = input.items.map(({ type, id, variant, startMinute, plannedMinutes }) => ({
    type, id, variant, startMinute, plannedMinutes,
  }));
  const result = { date: input.date, version: input.expectedVersion + 1 };
  const now = Date.now();
  return executeCommand(db, actor, commandKey, "daily.plan.save", input, recordId, result.version, result, [
    db.prepare(
      "SELECT version FROM staff_daily_plans WHERE company_id=? AND user_id=? AND plan_date=? FOR UPDATE",
    ).bind(actor.companyId, actor.userId, input.date),
    assertion(db, commandKey, "daily-plan-version",
      "COALESCE((SELECT version FROM staff_daily_plans WHERE company_id=? AND user_id=? AND plan_date=?),0)=?",
      [actor.companyId, actor.userId, input.date, input.expectedVersion]),
    db.prepare(`INSERT INTO staff_daily_plans
      (company_id,user_id,plan_date,items_json,version,last_command_id,updated_at)
      VALUES(?,?,?,?,?,?,?)
      ON CONFLICT(company_id,user_id,plan_date) DO UPDATE SET
        items_json=excluded.items_json,version=excluded.version,
        last_command_id=excluded.last_command_id,updated_at=excluded.updated_at
      WHERE staff_daily_plans.version=?`)
      .bind(actor.companyId, actor.userId, input.date, JSON.stringify(stored),
        result.version, commandKey, now, input.expectedVersion),
    assertion(db, commandKey, "daily-plan-written",
      "EXISTS(SELECT 1 FROM staff_daily_plans WHERE company_id=? AND user_id=? AND plan_date=? AND version=? AND last_command_id=?)",
      [actor.companyId, actor.userId, input.date, result.version, commandKey]),
  ]);
}
