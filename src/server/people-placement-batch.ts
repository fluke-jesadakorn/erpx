import "server-only";

import { createHash } from "node:crypto";
import { z } from "zod";
import type { WorkContext } from "./authorization";
import { requirePermission } from "./iam-permissions";
import { ApiError } from "./http";
import type { PostgresDatabase } from "./runtime/postgres";
import type { SqlDatabase } from "./runtime/types";
import { previewLifecycle, saveLifecycle } from "./people-lifecycle";
import { executeCommand, replayCommand } from "./commands";
import { lifecycleInput, type LifecycleInput, type LifecyclePreview } from "@/lib/people-lifecycle";

const id = z.string().min(1).max(180);
const legacy = z.object({ id, expectedVersion: z.number().int().positive() }).strict();
export const placementBatchInput = z.object({
  effectiveAt: z.number().int().positive(),
  timezone: z.string().min(1).max(80).refine(value => { try { new Intl.DateTimeFormat("en", { timeZone: value }); return true; } catch { return false; } }),
  reason: z.string().trim().min(3).max(1000),
  items: z.array(z.object({
    engagementId: id,
    positionId: id,
    managerPersonId: id.nullable(),
    acceptRecommendedLeave: z.boolean(),
    supersedeLegacyCase: legacy.optional(),
  }).strict()).min(1).max(20),
}).strict().superRefine((value, context) => {
  const seen = new Set<string>();
  value.items.forEach((item, index) => {
    if (seen.has(item.engagementId)) context.addIssue({ code: "custom", path: ["items", index, "engagementId"], message: "Choose each employee once." });
    seen.add(item.engagementId);
  });
});
export type PlacementBatchInput = z.infer<typeof placementBatchInput>;

type Prepared = { engagementId: string; input?: LifecycleInput; error?: string };
type BatchItemPreview = { engagementId: string; ready: boolean; blockers: string[]; token?: string; legacyCaseToSupersede?: LifecyclePreview["legacyCaseToSupersede"]; affectedPeople?: number };

function authority(actor: WorkContext) {
  requirePermission(actor, "people.manage");
  requirePermission(actor, "organization.manage");
  if (actor.scope.kind !== "holding" || actor.assignmentScope !== "holding" || actor.branchId || actor.departmentId)
    throw new ApiError(403, "COMPANY_AUTHORITY_REQUIRED", "Open the Company workspace with HR authority to move employees.");
}

async function prepare(db: SqlDatabase, actor: WorkContext, batch: PlacementBatchInput): Promise<Prepared[]> {
  authority(actor);
  const people = (await db.prepare("SELECT e.id engagement_id,e.person_id,e.status,e.position_id FROM hr_engagements e JOIN hr_people p ON p.id=e.person_id AND p.company_id=e.company_id WHERE e.company_id=? AND e.status!='ended' AND p.status!='archived' AND p.merged_into_id IS NULL")
    .bind(actor.companyId).all<{engagement_id:string;person_id:string;status:string;position_id:string|null}>()).results;
  const positions = (await db.prepare("SELECT id FROM organization_positions WHERE company_id=? AND active=1")
    .bind(actor.companyId).all<{id:string}>()).results;
  return batch.items.map(item => {
    const person = people.find(value => value.engagement_id === item.engagementId);
    const position = positions.find(value => value.id === item.positionId);
    let error: string | undefined;
    if (!person || person.status === "ended") error = "This employee is no longer current in this Company.";
    else if (!position) error = "Choose an active position in this Company.";
    else if (person.position_id === item.positionId) error = "This employee already holds this position.";
    if (error || !person) return { engagementId: item.engagementId, error };
    return { engagementId: item.engagementId, input: lifecycleInput.parse({
      expectedVersion: 0,
      kind: person.status === "onboarding" ? "onboarding" : "transfer",
      personId: person.person_id,
      engagementId: item.engagementId,
      positionId: item.positionId,
      managerPersonId: item.managerPersonId,
      acceptRecommendedPay: false,
      acceptRecommendedLeave: item.acceptRecommendedLeave,
      effectiveAt: batch.effectiveAt,
      timezone: batch.timezone,
      reason: batch.reason,
    }) };
  });
}

async function previewPrepared(db: SqlDatabase, actor: WorkContext, rows: Prepared[]): Promise<BatchItemPreview[]> {
  const output: BatchItemPreview[] = [];
  for (const row of rows) {
    if (!row.input) { output.push({ engagementId: row.engagementId, ready: false, blockers: [row.error ?? "This employee changed."] }); continue; }
    try {
      const preview = await previewLifecycle(db, actor, row.input, true);
      output.push({ engagementId: row.engagementId, ready: preview.ready, blockers: preview.blockers, token: preview.token, legacyCaseToSupersede: preview.legacyCaseToSupersede, affectedPeople: preview.affectedPeople.length });
    } catch (error) {
      if (!(error instanceof ApiError)) throw error;
      output.push({ engagementId: row.engagementId, ready: false, blockers: [error.message] });
    }
  }
  return output;
}

function batchToken(input: PlacementBatchInput, items: BatchItemPreview[]) {
  return createHash("sha256").update(JSON.stringify({ input, tokens: items.map(item => item.token) })).digest("hex");
}

export async function previewPlacementBatch(db: SqlDatabase, actor: WorkContext, input: PlacementBatchInput) {
  const items = await previewPrepared(db, actor, await prepare(db, actor, input));
  for (const item of items) {
    const acknowledged = input.items.find(value => value.engagementId === item.engagementId)?.supersedeLegacyCase;
    if (item.legacyCaseToSupersede && (acknowledged?.id !== item.legacyCaseToSupersede.id || acknowledged?.expectedVersion !== item.legacyCaseToSupersede.version)) {
      item.ready = false;
      item.blockers.push("Confirm the older request before scheduling this employee.");
    }
  }
  const ready = items.every(item => item.ready);
  return { ready, items, token: ready ? batchToken(input, items) : null };
}

export async function submitPlacementBatch(db: PostgresDatabase, actor: WorkContext, input: PlacementBatchInput, token: string, key: string) {
  authority(actor);
  if (!/^[a-zA-Z0-9:_.-]{8,180}$/.test(key)) throw new ApiError(422, "COMMAND_KEY_REQUIRED", "Use a valid Idempotency-Key.");
  const recordId = `placement-batch-${key}`;
  const payload = { input, token };
  const operation = "people.lifecycle.batch.submit";
  const prior = await replayCommand<{ ids: string[]; count: number }>(db, actor, key, operation, recordId, payload);
  if (prior) return prior;
  try {
    return await db.transaction(async tx => {
      const rows = await prepare(tx, actor, input);
      const items = await previewPrepared(tx, actor, rows);
      if (items.some((item, index) => !item.ready || item.legacyCaseToSupersede && (input.items[index].supersedeLegacyCase?.id !== item.legacyCaseToSupersede.id || input.items[index].supersedeLegacyCase?.expectedVersion !== item.legacyCaseToSupersede.version)) || token !== batchToken(input, items))
        throw new ApiError(409, "BATCH_PREVIEW_CHANGED", "An employee or destination changed. Review the entire batch again.");
      const ids: string[] = [];
      for (const [index, row] of rows.entries()) {
        const item = input.items[index];
        const preview = items[index];
        if (!row.input || !preview.token) throw new ApiError(409, "BATCH_PREVIEW_CHANGED", "Review the entire batch again.");
        const childKey = `placement-batch:${createHash("sha256").update(key).digest("hex").slice(0, 32)}:${index}`;
        const result = await saveLifecycle(tx, actor, row.input, childKey, true, preview.token, true, item.supersedeLegacyCase);
        ids.push(String(result.id));
      }
      const result = { ids, count: ids.length };
      await executeCommand(tx, actor, key, operation, payload, recordId, 1, result, []);
      return result;
    });
  } catch (error) {
    if (error instanceof ApiError) throw error;
    throw new ApiError(409, "BATCH_CHANGED", "The batch could not be submitted. No placements were saved. Refresh and review again.");
  }
}
