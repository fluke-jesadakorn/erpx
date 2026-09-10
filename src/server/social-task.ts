import type { SqlDatabase, SqlStatement } from "./runtime/types";
import { z } from "zod";
import { ApiError } from "./http";
import { assertion } from "./commands";
import { readablePost, sourceSnapshot, sourceGuard } from "./social-source";
import type { StaffActor } from "./staff-policy";
import { aiSnapshotGuard, type AiProviderSnapshot } from "./ai-providers";
export const taskSourceSchema = z.object({
  postId: z.string().min(1).max(160),
  sourceVersion: z.number().int().positive(),
  sourceHash: z.string().length(64),
  commentIds: z.array(z.string().max(160)).max(20).default([]),
  aiDraftId: z.string().max(160).nullable().default(null),
});
export type TaskSource = z.infer<typeof taskSourceSchema>;
export async function taskSourceStatements(
  db: SqlDatabase,
  actor: StaffActor,
  key: string,
  taskId: string,
  users: string[],
  source?: TaskSource,
  projectId?: string | null,
) {
  const existing = await db
    .prepare("SELECT post_id FROM social_task_origins WHERE task_id=?")
    .bind(taskId)
    .first<{ post_id: string }>();
  if (!source && !existing) return [];
  if (projectId)
    throw new ApiError(
      422,
      "POST_TASK_PROJECT",
      "Post tasks use named participants. Remove the project before saving.",
    );
  const post = source
    ? (await sourceSnapshot(db, actor, source.postId, source.commentIds)).post
    : await readablePost(db, actor, existing!.post_id);
  const statements: SqlStatement[] = [];
  if (source) {
    const snapshot = await sourceSnapshot(
      db,
      actor,
      source.postId,
      source.commentIds,
    );
    if (
      snapshot.hash !== source.sourceHash ||
      post.version !== source.sourceVersion
    )
      throw new ApiError(
        409,
        "SOURCE_CHANGED",
        "The conversation changed. Review it and prepare the task again.",
      );
    if (source.aiDraftId) {
      const ai = await db
        .prepare(
          "SELECT provider_json FROM social_ai_drafts WHERE id=? AND actor_id=?",
        )
        .bind(source.aiDraftId, actor.userId)
        .first<{ provider_json: string | null }>();
      if (ai?.provider_json)
        statements.push(
          aiSnapshotGuard(
            db,
            key,
            JSON.parse(ai.provider_json) as AiProviderSnapshot,
          ),
        );
    }
    if (source.aiDraftId)
      statements.push(
        assertion(
          db,
          key,
          "ai-draft",
          "EXISTS(SELECT 1 FROM social_ai_drafts WHERE id=? AND actor_id=? AND post_id=? AND source_hash=? AND source_version=? AND state='ready' AND kind='task' AND expires_at>?)",
          [
            source.aiDraftId,
            actor.userId,
            post.id,
            source.sourceHash,
            post.version,
            Date.now(),
          ],
        ),
      );
    statements.push(
      db
        .prepare(
          "INSERT INTO social_task_origins(post_id,task_id,source_version,source_hash,ai_draft_id,created_by,created_at) VALUES(?,?,?,?,?,?,?)",
        )
        .bind(
          post.id,
          taskId,
          post.version,
          source.sourceHash,
          source.aiDraftId,
          actor.userId,
          Date.now(),
        ),
    );
  }
  for (const [i, user] of [...new Set([actor.userId, ...users])].entries()) {
    await readablePost(
      db,
      { userId: user, companyId: actor.companyId },
      post.id,
    );
    statements.unshift(
      sourceGuard(db, key, post, user, `source-participant-${i}`),
    );
  }
  return statements;
}
