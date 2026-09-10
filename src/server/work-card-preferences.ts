import type { SqlDatabase } from "./runtime/types";
import "server-only";
import { z } from "zod";
import type { WorkContext } from "./authorization";
import {
  workCardIds,
  type WorkCardId,
  type WorkCardPreferences,
} from "@/lib/work-cards";
import { assertion, executeCommand, replayCommand } from "./commands";
import { serviceAuthoritySnapshot, serviceMember } from "./service-access";
import { ApiError } from "./http";

export const workCardPreferenceSchema = z
  .object({
    pinnedIds: z
      .array(z.enum(workCardIds))
      .max(6)
      .refine(
        (ids) => new Set(ids).size === ids.length,
        "Choose each shortcut once.",
      )
      .nullable(),
    expectedVersion: z.number().int().nonnegative(),
  })
  .strict();
async function requireMember(db: SqlDatabase, actor: WorkContext) {
  if (
    !(await serviceMember(
      db,
      {
        company_id: actor.companyId,
        branch_id: actor.branchId,
        department_id: actor.departmentId,
      },
      actor.userId,
    ))
  )
    throw new ApiError(
      403,
      "MEMBERSHIP_REQUIRED",
      "Current workspace membership is required.",
    );
}
export async function readWorkCardPreferences(
  db: SqlDatabase,
  actor: WorkContext,
): Promise<WorkCardPreferences> {
  await requireMember(db, actor);
  const row = await db
    .prepare(
      "SELECT pinned_ids_json,version FROM work_card_preferences WHERE company_id=? AND user_id=?",
    )
    .bind(actor.companyId, actor.userId)
    .first<{ pinned_ids_json: string | null; version: number }>();
  return {
    pinnedIds: row?.pinned_ids_json
      ? (JSON.parse(row.pinned_ids_json) as WorkCardId[])
      : null,
    version: row?.version ?? 0,
  };
}
export async function saveWorkCardPreferences(
  db: SqlDatabase,
  actor: WorkContext,
  input: z.infer<typeof workCardPreferenceSchema>,
  key: string,
  available: () => Promise<WorkCardId[]>,
) {
  await requireMember(db, actor);
  const operation = "work.cards.preferences",
    recordId = `${actor.companyId}:${actor.userId}`;
  const replay = await replayCommand<WorkCardPreferences>(
    db,
    actor,
    key,
    operation,
    recordId,
    input,
  );
  if (replay) return replay;
  const current = await readWorkCardPreferences(db, actor);
  if (current.version !== input.expectedVersion)
    throw new ApiError(
      409,
      "VERSION_CONFLICT",
      "Your shortcuts changed on another device. Review the latest arrangement before saving.",
    );
  const guard = await serviceAuthoritySnapshot(db, actor.companyId);
  const added = (input.pinnedIds ?? []).filter(
    (id) => !current.pinnedIds?.includes(id),
  );
  if (added.length) {
    const allowed = await available();
    if (added.some((id) => !allowed.includes(id)))
      throw new ApiError(
        403,
        "SHORTCUT_UNAVAILABLE",
        "A selected shortcut is unavailable in this workspace. Review your available actions.",
      );
  }
  const result: WorkCardPreferences = {
    pinnedIds: input.pinnedIds,
    version: current.version + 1,
  };
  return executeCommand(
    db,
    actor,
    key,
    operation,
    input,
    recordId,
    result.version,
    result,
    [
      ...guard(key),
      assertion(
        db,
        key,
        "work-cards-version",
        "COALESCE((SELECT version FROM work_card_preferences WHERE company_id=? AND user_id=?),0)=?",
        [actor.companyId, actor.userId, input.expectedVersion],
      ),
      db
        .prepare(
          "INSERT INTO work_card_preferences(company_id,user_id,pinned_ids_json,version,updated_at) VALUES(?,?,?,?,?) ON CONFLICT(company_id,user_id) DO UPDATE SET pinned_ids_json=excluded.pinned_ids_json,version=excluded.version,updated_at=excluded.updated_at",
        )
        .bind(
          actor.companyId,
          actor.userId,
          input.pinnedIds === null ? null : JSON.stringify(input.pinnedIds),
          result.version,
          Date.now(),
        ),
    ],
  );
}
