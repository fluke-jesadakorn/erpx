import "server-only";
import { createHash } from "node:crypto";
import { z } from "zod";
import type { QuickAccessPreference } from "@/lib/quick-access";
import { ApiError } from "./http";
import { getRuntime } from "./runtime/env";
import {
  isTransactionalSqlDatabase,
  type SqlDatabase,
} from "./runtime/types";

export const quickAccessPreferenceSchema = z.object({
  pinnedIds: z.array(z.string().min(1).max(200)).max(6)
    .refine((ids) => new Set(ids).size === ids.length, "Choose each shortcut once.")
    .nullable(),
  expectedVersion: z.number().int().nonnegative(),
}).strict();

type SaveInput = z.infer<typeof quickAccessPreferenceSchema> & {
  userId: string;
  surface: string;
  contextKey: string;
  availableIds: string[];
  requestKey: string;
};

type PreferenceRow = { pinned_ids_json: string | null; version: number };

async function readFrom(
  db: SqlDatabase,
  userId: string,
  surface: string,
  contextKey: string,
): Promise<QuickAccessPreference> {
  const row = await db.prepare(
    "SELECT pinned_ids_json,version FROM quick_access_preferences WHERE user_id=? AND surface=? AND context_key=?",
  ).bind(userId, surface, contextKey).first<PreferenceRow>();
  return {
    pinnedIds: row?.pinned_ids_json === null || row === null
      ? null
      : JSON.parse(row.pinned_ids_json) as string[],
    version: row?.version ?? 0,
  };
}

export async function readQuickAccessPreferences(
  userId: string,
  surface: string,
  contextKey: string,
): Promise<QuickAccessPreference> {
  return readFrom(getRuntime().CONTROL_DB, userId, surface, contextKey);
}

export async function saveQuickAccessPreferences(input: SaveInput): Promise<QuickAccessPreference> {
  const parsed = quickAccessPreferenceSchema.parse({
    pinnedIds: input.pinnedIds,
    expectedVersion: input.expectedVersion,
  });
  if (!input.userId || !input.surface || !input.contextKey)
    throw new ApiError(422, "INVALID_SCOPE", "The shortcut scope is invalid.");
  if (!input.requestKey || input.requestKey.length > 200)
    throw new ApiError(422, "IDEMPOTENCY_KEY_REQUIRED", "Provide a command key to save shortcuts.");
  const db = getRuntime().CONTROL_DB;
  if (!isTransactionalSqlDatabase(db)) throw new Error("CONTROL_TRANSACTION_REQUIRED");
  const hash = createHash("sha256")
    .update(JSON.stringify([input.userId, input.surface, input.contextKey, parsed]))
    .digest("hex");
  const available = new Set(input.availableIds);

  return db.transaction(async (tx) => {
    const prior = await tx.prepare(
      "SELECT user_id,surface,context_key,request_hash,result_json FROM quick_access_preference_receipts WHERE id=?",
    ).bind(input.requestKey).first<{
      user_id: string; surface: string; context_key: string; request_hash: string; result_json: string;
    }>();
    if (prior) {
      if (prior.user_id !== input.userId || prior.surface !== input.surface ||
          prior.context_key !== input.contextKey || prior.request_hash !== hash)
        throw new ApiError(409, "IDEMPOTENCY_CONFLICT", "This save key belongs to a different shortcut change.");
      return JSON.parse(prior.result_json) as QuickAccessPreference;
    }
    const current = await readFrom(tx, input.userId, input.surface, input.contextKey);
    if (current.version !== parsed.expectedVersion)
      throw new ApiError(409, "VERSION_CONFLICT", "Your shortcuts changed on another device. Review the latest order before saving.");
    const added = (parsed.pinnedIds ?? []).filter((id) => !current.pinnedIds?.includes(id));
    if (added.some((id) => !available.has(id)))
      throw new ApiError(403, "SHORTCUT_UNAVAILABLE", "A selected shortcut is no longer available.");
    const result: QuickAccessPreference = {
      pinnedIds: parsed.pinnedIds,
      version: current.version + 1,
    };
    await tx.prepare(
      "INSERT INTO quick_access_preferences(user_id,surface,context_key,pinned_ids_json,version,updated_at) VALUES(?,?,?,?,?,?) ON CONFLICT(user_id,surface,context_key) DO UPDATE SET pinned_ids_json=excluded.pinned_ids_json,version=excluded.version,updated_at=excluded.updated_at",
    ).bind(input.userId, input.surface, input.contextKey,
      result.pinnedIds === null ? null : JSON.stringify(result.pinnedIds),
      result.version, Date.now()).run();
    await tx.prepare(
      "INSERT INTO quick_access_preference_receipts(id,user_id,surface,context_key,request_hash,result_json,created_at) VALUES(?,?,?,?,?,?,?)",
    ).bind(input.requestKey, input.userId, input.surface, input.contextKey, hash,
      JSON.stringify(result), Date.now()).run();
    return result;
  });
}
