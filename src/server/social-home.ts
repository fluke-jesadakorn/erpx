import type { WorkContext } from "./authorization";
import type { Mention } from "@/lib/social-text";
import { mentionsSchema, validateMentions, textStatements } from "./social-text";
import type { SqlDatabase } from "./runtime/types";
import { z } from "zod";
import { ApiError } from "./http";
import { assertion, executeCommand } from "./commands";
import {
  readablePost,
  sourceAccessSql,
  sourceGuard,
  type PostRow,
} from "./social-source";
import {
  notification,
  authorityGuard,
  requireCompanyAuthority,
  type StaffActor,
} from "./staff-policy";
type Actor = WorkContext;
export const postDraftSchema = z.object({
  body: z.string().max(3000),
  mentions: mentionsSchema.optional(),
  branchId: z.string().max(160).nullable().default(null),
  expectedVersion: z.number().int().positive().default(1),
});
export type PostDraft = {
  id: string;
  author_id: string;
  company_id: string;
  branch_id: string | null;
  body: string;
  department_id: string | null;
  version: number;
  published_post_id: string | null;
  mentions_json: string;
};
export async function socialUploadOwner(
  db: SqlDatabase,
  actor: StaffActor,
  id: string,
  edit = false,
) {
  const draft = await db
    .prepare("SELECT * FROM social_post_drafts WHERE id=? AND company_id=?")
    .bind(id, actor.companyId)
    .first<PostDraft>();
  if (!draft) throw new ApiError(404, "DRAFT_NOT_FOUND", "Draft unavailable.");
  if (edit || !draft.published_post_id) {
    if (draft.author_id !== actor.userId || draft.published_post_id)
      throw new ApiError(
        403,
        "DRAFT_LOCKED",
        "Only the author can edit an unpublished draft.",
      );
  } else await readablePost(db, actor, draft.published_post_id);
  return draft;
}
export async function savePostDraft(
  db: SqlDatabase,
  actor: Actor,
  id: string | null,
  input: z.infer<typeof postDraftSchema>,
  key: string,
) {
  const recordId = id ?? `post-draft-${key}`,
    now = Date.now();
  if (id) await socialUploadOwner(db, actor, id, true);
  const mentions = await validateMentions(db, actor, input.body, input.mentions);
  const branch = input.branchId;
  if (actor.branchId && branch !== actor.branchId)
    throw new ApiError(
      403,
      "AUDIENCE_FORBIDDEN",
      "Use your current branch audience.",
    );
  return executeCommand(
    db,
    actor,
    key,
    "social.draft.save",
    input,
    recordId,
    id ? input.expectedVersion + 1 : 1,
    { id: recordId, version: id ? input.expectedVersion + 1 : 1 },
    [
      ...(branch
        ? [
            assertion(
              db,
              key,
              "branch",
              "EXISTS(SELECT 1 FROM organization_nodes WHERE id=? AND company_id=? AND kind='branch' AND status='active')",
              [branch, actor.companyId],
            ),
          ]
        : []),
      ...(id
        ? [
            assertion(
              db,
              key,
              "draft-version",
              "EXISTS(SELECT 1 FROM social_post_drafts WHERE id=? AND author_id=? AND version=? AND published_post_id IS NULL)",
              [id, actor.userId, input.expectedVersion],
            ),
          ]
        : []),
      id
        ? db
            .prepare(
              "UPDATE social_post_drafts SET body=?,branch_id=?,mentions_json=?,version=version+1,updated_at=? WHERE id=?",
            )
            .bind(input.body, branch, JSON.stringify(mentions), now, id)
        : db
            .prepare(
              "INSERT INTO social_post_drafts(id,author_id,company_id,branch_id,body,created_at,updated_at,department_id,mentions_json) VALUES(?,?,?,?,?,?,?,?,?)",
            )
            .bind(
              recordId,
              actor.userId,
              actor.companyId,
              branch,
              input.body,
              now,
              now,
              actor.departmentId,
              JSON.stringify(mentions),
            ),
    ],
  );
}
export async function publishPost(
  db: SqlDatabase,
  actor: Actor,
  id: string,
  version: number,
  key: string,
) {
  const draft = await db
    .prepare(
      "SELECT * FROM social_post_drafts WHERE id=? AND company_id=? AND author_id=?",
    )
    .bind(id, actor.companyId, actor.userId)
    .first<PostDraft>();
  if (!draft) throw new ApiError(404, "DRAFT_NOT_FOUND", "Draft unavailable.");
  if (draft.published_post_id) {
    await readablePost(db, actor, draft.published_post_id);
    return { id: draft.published_post_id, version: 1 };
  }
  if (!draft.body.trim())
    throw new ApiError(
      422,
      "POST_BODY_REQUIRED",
      "Add a message before publishing.",
    );
  const mentions = await validateMentions(db, actor, draft.body, JSON.parse(draft.mentions_json));
  const postId = `post-${id}`,
    now = Date.now();
  return executeCommand(
    db,
    actor,
    key,
    "social.post.publish",
    { id, version },
    postId,
    1,
    { id: postId, version: 1 },
    [
      assertion(
        db,
        key,
        "draft",
        "EXISTS(SELECT 1 FROM social_post_drafts WHERE id=? AND author_id=? AND version=? AND published_post_id IS NULL)",
        [id, actor.userId, version],
      ),
      assertion(
        db,
        key,
        "uploads",
        "NOT EXISTS(SELECT 1 FROM upload_intents WHERE owner_type='social_draft' AND owner_id=? AND status IN ('pending','uploaded') AND expires_at>?)",
        [id, now],
      ),
      db
        .prepare(
          "INSERT INTO social_posts(id,kind,author_id,author_name,company_id,branch_id,body,last_command_id,created_at,updated_at,department_id,mentions_json) VALUES(?,'team',?,?,?,?,?,?,?,?,?,?)",
        )
        .bind(
          postId,
          actor.userId,
          actor.name,
          actor.companyId,
          draft.branch_id,
          draft.body,
          key,
          now,
          now,
          draft.department_id,
          JSON.stringify(mentions),
        ),
      ...textStatements(db, actor, postId, "post", postId, draft.body, mentions),
      db
        .prepare(
          "UPDATE social_post_drafts SET published_post_id=?,version=version+1,updated_at=? WHERE id=?",
        )
        .bind(postId, now, id),
      db
        .prepare(
          "INSERT INTO social_follows(principal_id,target_type,target_id,created_at) VALUES(?,'post',?,?) ON CONFLICT DO NOTHING",
        )
        .bind(actor.userId, postId, now),
    ],
  );
}
export async function postView(db: SqlDatabase, actor: StaffActor, id: string) {
  const post = await readablePost(db, actor, id);
  const [comments, files, counts, origin] = await Promise.all([
    db
      .prepare(
        "SELECT id,body,author_id,author_name,created_at,mentions_json,author_kind FROM social_comments WHERE post_id=? AND state='published' ORDER BY created_at DESC,id DESC LIMIT 50",
      )
      .bind(id)
      .all<{
        id: string;
        body: string;
        author_id: string;
        author_name: string;
        created_at: number;
        mentions_json: string;
        author_kind: "person" | "agent";
      }>(),
    db
      .prepare(
        "SELECT a.id,a.original_name,a.media_type FROM attachments a JOIN social_post_drafts d ON d.id=a.owner_id WHERE a.owner_type='social_draft' AND d.published_post_id=? AND a.revoked_at IS NULL",
      )
      .bind(id)
      .all<{ id: string; original_name: string; media_type: string }>(),
    db
      .prepare(
        "SELECT (SELECT count(*) FROM social_reactions WHERE post_id=?) appreciationCount,EXISTS(SELECT 1 FROM social_reactions WHERE post_id=? AND principal_id=?) appreciated,EXISTS(SELECT 1 FROM social_follows WHERE target_type='post' AND target_id=? AND principal_id=?) followed",
      )
      .bind(id, id, actor.userId, id, actor.userId)
      .first<{
        appreciationCount: number;
        appreciated: number;
        followed: number;
      }>(),
    db
      .prepare("SELECT task_id FROM social_task_origins WHERE post_id=?")
      .bind(id)
      .first<{ task_id: string }>(),
  ]);
  // A linked task's existence does not grant its details. The task endpoint rechecks participants.
  let task: null | { id: string; state: string } = null;
  let trackingState: string | null = null;
  if (origin) {
    const { getTask } = await import("./staff-tasks");
    try {
      const t = await getTask(db, actor, origin.task_id);
      task = { id: t.id, state: t.state };
    } catch (e) {
      if (!(e instanceof ApiError) || ![403, 404].includes(e.status)) throw e;
    }
    // The original reporter may track progress without gaining task/evidence access.
    if (post.author_id === actor.userId)
      trackingState = await db
        .prepare("SELECT state FROM staff_tasks WHERE id=? AND company_id=?")
        .bind(origin.task_id, actor.companyId)
        .first<string>("state");
  }
  return {
    ...post,
    mentions: JSON.parse((post as PostRow & {mentions_json:string}).mentions_json ?? "[]") as Mention[],
    agentReplies: (await db.prepare("SELECT id,state,error,actor_id FROM social_agent_replies WHERE post_id=? ORDER BY created_at").bind(id).all<{id:string;state:string;error:string|null;actor_id:string}>()).results,
    comments: comments.results.reverse().map(c=>({...c, mentions: JSON.parse(c.mentions_json) as Mention[]})),
    files: files.results,
    ...counts,
    task,
    trackingState,
    hasTask: !!origin,
  };
}
export async function homePosts(
  db: SqlDatabase,
  actor: StaffActor,
  filter: {
    tag?: string;
    following?: boolean;
    branchId?: string;
    before?: number;
    beforeId?: string;
  } = {},
) {
  const now = Date.now();
  const result = await db
    .prepare(
      `SELECT p.id FROM social_posts p WHERE p.company_id=? AND p.state='published' AND p.linked_module IS NULL AND ${sourceAccessSql()} AND (?::text IS NULL OR p.branch_id=? OR p.branch_id IS NULL) AND (?::text IS NULL OR p.branch_id=?) AND (p.created_at<? OR (p.created_at=? AND p.id<?)) AND (?=0 OR EXISTS(SELECT 1 FROM social_follows f WHERE f.principal_id=? AND f.target_type='post' AND f.target_id=p.id)) AND (?::text IS NULL OR EXISTS(SELECT 1 FROM social_post_topics pt WHERE pt.post_id=p.id AND pt.tag=?) OR EXISTS(SELECT 1 FROM social_comment_topics ct JOIN social_comments c ON c.id=ct.comment_id WHERE c.post_id=p.id AND c.state='published' AND ct.tag=?)) ORDER BY p.created_at DESC,p.id DESC LIMIT 20`,
    )
    .bind(
      actor.companyId,
      actor.userId,
      now,
      now,
      actor.branchId,
      actor.branchId,
      filter.branchId ?? null,
      filter.branchId ?? null,
      filter.before ?? now + 1,
      filter.before ?? now + 1,
      filter.beforeId ?? "",
      filter.following ? 1 : 0,
      actor.userId,
      filter.tag ?? null, filter.tag ?? null, filter.tag ?? null,
    )
    .all<{ id: string }>();
  const posts = [];
  for (const row of result.results) {
    try {
      posts.push(await postView(db, actor, row.id));
    } catch (e) {
      if (!(e instanceof ApiError) || e.status !== 404) throw e;
    }
  }
  return posts;
}
export async function postAction(
  db: SqlDatabase,
  actor: Actor,
  id: string,
  input: {
    action: "comment" | "appreciate" | "follow" | "report" | "hide";
    body?: string;
    active?: boolean;
    mentionUserIds?: string[];
    mentions?: Mention[];
  },
  key: string,
) {
  const post = await readablePost(db, actor, id),
    now = Date.now(),
    steps = [sourceGuard(db, key, post, actor.userId, "source")];
  if (input.action === "comment") {
    const body = z.string().min(1).max(1000).refine(b=>!!b.trim()).parse(input.body);
    const mentions = await validateMentions(db, actor, body, input.mentions);
    steps.push(
      db
        .prepare(
          "INSERT INTO social_comments(id,post_id,author_id,author_name,body,last_command_id,created_at,updated_at,mentions_json) VALUES(?,?,?,?,?,?,?,?,?)",
        )
        .bind(
          `comment-${key}`,
          id,
          actor.userId,
          actor.name,
          body,
          key,
          now,
          now,
          JSON.stringify(mentions),
        ),
      ...textStatements(db, actor, id, "comment", `comment-${key}`, body, mentions),
    );
    for (const [i, user] of [...new Set(input.mentionUserIds ?? [])]
      .slice(0, 10)
      .entries()) {
      await readablePost(db, { companyId: actor.companyId, userId: user }, id);
      steps.push(
        sourceGuard(
          db,
          key,
          { ...post, version: post.version + 1 },
          user,
          `mention-${i}`,
        ),
        notification(
          db,
          post,
          user,
          "post",
          id,
          post.version + 1,
          "You were mentioned in a conversation",
        ),
      );
    }
  } else if (input.action === "appreciate")
    steps.push(
      input.active
        ? db
            .prepare(
              "INSERT INTO social_reactions(post_id,principal_id,kind,created_at) VALUES(?,?,'appreciate',?) ON CONFLICT DO NOTHING",
            )
            .bind(id, actor.userId, now)
        : db
            .prepare(
              "DELETE FROM social_reactions WHERE post_id=? AND principal_id=?",
            )
            .bind(id, actor.userId),
    );
  else if (input.action === "follow")
    steps.push(
      input.active
        ? db
            .prepare(
              "INSERT INTO social_follows(principal_id,target_type,target_id,created_at) VALUES(?,'post',?,?) ON CONFLICT DO NOTHING",
            )
            .bind(actor.userId, id, now)
        : db
            .prepare(
              "DELETE FROM social_follows WHERE principal_id=? AND target_type='post' AND target_id=?",
            )
            .bind(actor.userId, id),
    );
  else if (input.action === "report")
    steps.push(
      db
        .prepare(
          "INSERT INTO social_reports(id,post_id,reporter_id,reason,created_at) VALUES(?,?,?,?,?) ON CONFLICT(post_id,reporter_id) DO UPDATE SET reason=excluded.reason",
        )
        .bind(
          `report-${key}`,
          id,
          actor.userId,
          z.string().trim().min(1).max(2000).parse(input.body),
          now,
        ),
    );
  else {
    await requireCompanyAuthority(db, actor, ["holding_owner"]);
    steps.push(
      authorityGuard(db, key, actor, ["holding_owner"]),
      db
        .prepare(
          "UPDATE social_posts SET state='moderated',version=version+1,updated_at=? WHERE id=?",
        )
        .bind(now, id),
    );
  }
  return executeCommand(
    db,
    actor,
    key,
    `social.post.${input.action}`,
    input,
    id,
    post.version + 1,
    { id },
    steps,
  );
}
export type HomePost = Awaited<ReturnType<typeof postView>>;
export type { PostRow };
