import type { SqlDatabase } from "./runtime/types";
import { sourceAccessSql } from "./social-source";
import "server-only";

import { z } from "zod";
import type { WorkContext } from "./authorization";
import { runtimeEnv, holdingResources } from "./runtime";
import { sha256 } from "./crypto";
import { ApiError } from "./http";
import { getRecord } from "./records";

export const feedFilterSchema = z.enum(["all", "following", "team", "records"]);
export type FeedFilter = z.infer<typeof feedFilterSchema>;

const audienceSchema = z.enum(["holding", "branch"]);
const tagSchema = z.string().trim().toLowerCase().regex(/^[a-z0-9][a-z0-9_-]{0,39}$/);
const mentionSchema = z.object({
  kind: z.enum(["person", "record"]),
  targetId: z.string().min(1).max(260),
  label: z.string().min(1).max(160),
});

export const createPostSchema = z.object({
  body: z.string().trim().min(1).max(3000),
  audience: audienceSchema.default("holding"),
  tags: z.array(tagSchema).max(5).default([]),
  linkedRecord: z.object({
    module: z.literal("people_payroll"),
    id: z.string().min(1).max(128),
  }).nullable().default(null),
  mentions: z.array(mentionSchema).max(20).default([]),
});

export const createCommentSchema = z.object({ body: z.string().trim().min(1).max(1000), mentions: z.array(mentionSchema).max(20).default([]) });
export const setActiveSchema = z.object({ active: z.boolean() });
export const createStorySchema = z.object({
  title: z.string().trim().min(1).max(80),
  body: z.string().trim().min(1).max(500),
  audience: audienceSchema.default("holding"),
  theme: z.enum(["sage", "sunset", "sky", "plum"]).default("sage"),
});

export type SocialComment = {
  id: string;
  postId: string;
  authorId: string;
  authorName: string;
  body: string;
  createdAt: number;
};

export type SocialPost = {
  id: string;
  kind: "team" | "record" | "system";
  authorId: string;
  authorName: string;
  body: string;
  audience: "Holding" | "Branch";
  branchId: string | null;
  linkedRecord: { module: string; id: string; label: string } | null;
  version: number;
  createdAt: number;
  appreciationCount: number;
  commentCount: number;
  appreciated: boolean;
  followed: boolean;
  topics: string[];
  comments: SocialComment[];
};

function scopeSql(context: WorkContext, alias = "p") {
  const now=Date.now();
  const base = `${alias}.company_id=? AND ${alias}.state='published' AND ${sourceAccessSql(alias)}`;
  if (context.departmentId)
    return {
      sql: `${base} AND (${alias}.department_id=? OR (${alias}.department_id IS NULL AND (${alias}.branch_id IS NULL OR ${alias}.branch_id=?)))`,
      bindings: [context.companyId,context.userId,now,now, context.departmentId, context.branchId],
    };
  return {
    sql: `${base} AND (${alias}.branch_id IS NULL OR ?::text IS NULL OR ${alias}.branch_id=?)`,
    bindings: [context.companyId,context.userId,now,now, context.branchId, context.branchId],
  };
}

function filterSql(filter: FeedFilter) {
  if (filter === "team") return " AND p.kind='team'";
  if (filter === "records") return " AND p.kind='record'";
  if (filter === "following") return ` AND (
    EXISTS(SELECT 1 FROM social_follows f WHERE f.principal_id=? AND f.target_type='post' AND f.target_id=p.id)
    OR EXISTS(SELECT 1 FROM social_follows f WHERE f.principal_id=? AND f.target_type='person' AND f.target_id=p.author_id)
    OR EXISTS(SELECT 1 FROM social_post_topics pt JOIN social_follows f ON f.target_type='topic' AND f.target_id=pt.tag
      WHERE pt.post_id=p.id AND f.principal_id=?))`;
  return "";
}

export async function listFeed(holdingId: string, context: WorkContext, filter: FeedFilter = "all") {
  const { write } = await holdingResources(holdingId);
  const scope = scopeSql(context);
  const followingBindings = filter === "following" ? [context.userId, context.userId, context.userId] : [];
  const response = await write.prepare(`SELECT p.*,
      (SELECT count(*) FROM social_reactions r WHERE r.post_id=p.id AND r.kind='appreciate') AS appreciation_count,
      (SELECT count(*) FROM social_comments c WHERE c.post_id=p.id AND c.state='published') AS comment_count,
      EXISTS(SELECT 1 FROM social_reactions r WHERE r.post_id=p.id AND r.principal_id=? AND r.kind='appreciate') AS appreciated,
      EXISTS(SELECT 1 FROM social_follows f WHERE f.principal_id=? AND f.target_type='post' AND f.target_id=p.id) AS followed,
      COALESCE((SELECT group_concat(pt.tag, ',') FROM social_post_topics pt WHERE pt.post_id=p.id),'') AS topic_list
    FROM social_posts p WHERE ${scope.sql}${filterSql(filter)}
    ORDER BY p.created_at DESC LIMIT 30`)
    .bind(context.userId, context.userId, ...scope.bindings, ...followingBindings)
    .all<Record<string, unknown>>();
  const rows = response.results;
  const commentStatements = rows.map((row) => write.prepare(`SELECT id,post_id,author_id,author_name,body,created_at
    FROM social_comments WHERE post_id=? AND state='published' ORDER BY created_at LIMIT 50`).bind(row.id));
  const comments = commentStatements.length ? await write.batch(commentStatements) : [];
  return rows.map((row, index): SocialPost => ({
    id: String(row.id),
    kind: row.kind as SocialPost["kind"],
    authorId: String(row.author_id),
    authorName: String(row.author_name),
    body: String(row.body),
    audience: row.branch_id ? "Branch" : "Holding",
    branchId: row.branch_id ? String(row.branch_id) : null,
    linkedRecord: row.linked_record_id ? {
      module: String(row.linked_module), id: String(row.linked_record_id), label: String(row.linked_record_label),
    } : null,
    version: Number(row.version),
    createdAt: Number(row.created_at),
    appreciationCount: Number(row.appreciation_count),
    commentCount: Number(row.comment_count),
    appreciated: Boolean(row.appreciated),
    followed: Boolean(row.followed),
    topics: String(row.topic_list).split(",").filter(Boolean),
    comments: ((comments[index]?.results ?? []) as Record<string, unknown>[]).map((comment) => ({
      id: String(comment.id), postId: String(comment.post_id), authorId: String(comment.author_id),
      authorName: String(comment.author_name), body: String(comment.body), createdAt: Number(comment.created_at),
    })),
  }));
}

export async function listStories(holdingId: string, context: WorkContext) {
  const { write } = await holdingResources(holdingId);
  const scope = scopeSql(context, "social_stories");
  const result = await write.prepare(`SELECT id,author_id,author_name,title,body,theme,branch_id,version,created_at
    FROM social_stories WHERE ${scope.sql} ORDER BY created_at DESC LIMIT 20`)
    .bind(...scope.bindings).all<Record<string, unknown>>();
  return result.results.map((row) => ({
    id: String(row.id), authorId: String(row.author_id), authorName: String(row.author_name),
    title: String(row.title), body: String(row.body), theme: String(row.theme), version: Number(row.version),
    audience: row.branch_id ? "Branch" : "Holding", createdAt: Number(row.created_at),
  }));
}

async function existingReceipt(db: SqlDatabase, commandId: string, requestHash: string) {
  const row = await db.prepare("SELECT request_hash,result_json FROM command_receipts WHERE command_id=?")
    .bind(commandId).first<{ request_hash: string; result_json: string | null }>();
  if (!row) return null;
  if (row.request_hash !== requestHash) throw new ApiError(409, "IDEMPOTENCY_CONFLICT", "This command key was used with different input.");
  return row.result_json ? JSON.parse(row.result_json) : { commandId };
}

async function mentionStatements(db:SqlDatabase,ownerType:"post"|"comment",ownerId:string,mentions:z.infer<typeof mentionSchema>[],context:WorkContext,now:number){
  const unique=[...new Map(mentions.map(item=>[`${item.kind}:${item.targetId}`,item])).values()];
  const resolved:typeof unique=[];
  for(const mention of unique){
    if(mention.kind==="person"){
      const found=await db.prepare(`SELECT 1 AS ok FROM assignments WHERE user_id=? AND revoked_at IS NULL AND valid_from<=? AND valid_until>? LIMIT 1`).bind(mention.targetId,now,now).first();
      if(!found)throw new ApiError(404,"MENTION_TARGET_NOT_FOUND","That person is not available in this Holding.");
      const env=await runtimeEnv();const principal=await env.CONTROL_DB.prepare(`SELECT display_name FROM principals WHERE user_id=? AND status='active'`).bind(mention.targetId).first<{display_name:string}>();
      if(!principal)throw new ApiError(404,"MENTION_TARGET_NOT_FOUND","That person is unavailable.");
      resolved.push({...mention,label:principal.display_name});
    }else{
      const [moduleId,...idParts]=mention.targetId.split("/");
      const record=await getRecord(context,moduleId,idParts.join("/"));resolved.push({...mention,label:record.title});
    }
  }
  return resolved.map(mention=>db.prepare(`INSERT INTO social_mentions(owner_type,owner_id,target_kind,target_id,target_label,created_by,created_at) VALUES(?,?,?,?,?,?,?)`).bind(ownerType,ownerId,mention.kind,mention.targetId,mention.label,context.userId,now));
}

export async function createPost(holdingId: string, input: z.infer<typeof createPostSchema>, commandId: string, context: WorkContext) {
  const { write } = await holdingResources(holdingId);
  const requestHash = await sha256(JSON.stringify({ operation: "social.post.publish", input }));
  const prior = await existingReceipt(write, commandId, requestHash);
  if (prior) return prior;
  if (input.audience === "branch" && !context.branchId) throw new ApiError(422, "BRANCH_REQUIRED", "Select a branch assignment to publish to a branch.");
  let linkedLabel: string | null = null;
  if (input.linkedRecord) {
    const {getLeave}=await import("./leave");
    const record=await getLeave(holdingId,input.linkedRecord.id,context);
    linkedLabel = `${record.number} · Leave request`;
  }
  const id = crypto.randomUUID();
  const now = Date.now();
  const branchId = input.audience === "branch" ? context.branchId : null;
  const tags = [...new Set([
    ...input.tags,
    ...Array.from(input.body.matchAll(/#([a-z0-9][a-z0-9_-]{0,39})/gi), (match) => match[1].toLowerCase()),
  ])].slice(0, 5);
  const result = { id, state: "published", version: 1, commandId };
  const mentions = await mentionStatements(write,"post",id,input.mentions,context,now);
  const statements = [
    write.prepare("INSERT INTO command_receipts(command_id,operation,actor_id,request_hash,record_id,result_json,created_at) VALUES(?,?,?,?,?,?,?)")
      .bind(commandId, "social.post.publish", context.userId, requestHash, id, JSON.stringify(result), now),
    write.prepare(`INSERT INTO social_posts(id,kind,author_id,author_name,company_id,branch_id,department_id,body,linked_module,
      linked_record_id,linked_record_label,state,version,last_command_id,created_at,updated_at)
      VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?)`).bind(id, input.linkedRecord ? "record" : "team", context.userId,
      context.name, context.companyId, branchId, context.departmentId, input.body, input.linkedRecord?.module ?? null,
      input.linkedRecord?.id ?? null, linkedLabel, "published", 1, commandId, now, now),
    ...tags.flatMap((tag) => [
      write.prepare("INSERT INTO social_topics(tag,created_by,created_at) VALUES(?,?,?) ON CONFLICT DO NOTHING").bind(tag, context.userId, now),
      write.prepare("INSERT INTO social_post_topics(post_id,tag) VALUES(?,?)").bind(id, tag),
    ]),
    ...mentions,
    write.prepare(`INSERT INTO command_assertions(command_id,step,ok) VALUES(?,?,CASE WHEN EXISTS(
      SELECT 1 FROM social_posts WHERE id=? AND last_command_id=? AND company_id=? AND state='published'
    ) THEN 1 ELSE 0 END)`).bind(commandId, "publish", id, commandId, context.companyId),
    write.prepare("INSERT INTO audit_events VALUES(?,?,?,?,?,?,?,?,?,?)").bind(crypto.randomUUID(), commandId,
      context.userId, context.assignmentId, "social.post.publish", "social_post", id, 1, "Publish workplace update", now),
    write.prepare("INSERT INTO outbox_events(id,command_id,event_type,aggregate_type,aggregate_id,aggregate_version,payload_json,created_at) VALUES(?,?,?,?,?,?,?,?)")
      .bind(crypto.randomUUID(), commandId, "social.post.published", "social_post", id, 1, JSON.stringify({ ...result, companyId: context.companyId, branchId }), now),
    write.prepare("DELETE FROM command_assertions WHERE command_id=?").bind(commandId),
  ];
  try { await write.batch(statements); return result; }
  catch {
    const committed = await existingReceipt(write, commandId, requestHash);
    if (committed) return committed;
    throw new ApiError(409, "PUBLISH_CONFLICT", "The update could not be published. Refresh and try again.");
  }
}

async function visiblePost(db: SqlDatabase, id: string, context: WorkContext) {
  const scope = scopeSql(context);
  const post = await db.prepare(`SELECT id,version FROM social_posts p WHERE p.id=? AND ${scope.sql}`)
    .bind(id, ...scope.bindings).first<{ id: string; version: number }>();
  if (!post) throw new ApiError(404, "POST_NOT_FOUND", "The update is unavailable.");
  return post;
}

export async function createComment(holdingId: string, postId: string, input: z.infer<typeof createCommentSchema>, commandId: string, context: WorkContext) {
  const { write } = await holdingResources(holdingId);
  const post = await visiblePost(write, postId, context);
  const requestHash = await sha256(JSON.stringify({ operation: "social.comment.publish", postId, input }));
  const prior = await existingReceipt(write, commandId, requestHash);
  if (prior) return prior;
  const id = crypto.randomUUID(); const now = Date.now();
  const result = { id, postId, authorId: context.userId, authorName: context.name, body: input.body, createdAt: now, commandId };
  const mentions = await mentionStatements(write,"comment",id,input.mentions,context,now);
  try {
    await write.batch([
      write.prepare("INSERT INTO command_receipts(command_id,operation,actor_id,request_hash,record_id,result_json,created_at) VALUES(?,?,?,?,?,?,?)")
        .bind(commandId, "social.comment.publish", context.userId, requestHash, id, JSON.stringify(result), now),
      write.prepare("INSERT INTO social_comments(id,post_id,author_id,author_name,body,state,last_command_id,created_at,updated_at) VALUES(?,?,?,?,?,'published',?,?,?)")
        .bind(id, postId, context.userId, context.name, input.body, commandId, now, now),
      ...mentions,
      write.prepare(`INSERT INTO command_assertions(command_id,step,ok) VALUES(?,?,CASE WHEN EXISTS(
        SELECT 1 FROM social_comments WHERE id=? AND post_id=? AND last_command_id=?
      ) THEN 1 ELSE 0 END)`).bind(commandId, "comment", id, postId, commandId),
      write.prepare("INSERT INTO audit_events VALUES(?,?,?,?,?,?,?,?,?,?)").bind(crypto.randomUUID(), commandId,
        context.userId, context.assignmentId, "social.comment.publish", "social_post", postId, post.version, "Comment on workplace update", now),
      write.prepare("INSERT INTO outbox_events(id,command_id,event_type,aggregate_type,aggregate_id,aggregate_version,payload_json,created_at) VALUES(?,?,?,?,?,?,?,?)")
        .bind(crypto.randomUUID(), commandId, "social.comment.published", "social_post", postId, post.version, JSON.stringify(result), now),
      write.prepare("DELETE FROM command_assertions WHERE command_id=?").bind(commandId),
    ]);
    return result;
  } catch {
    const committed = await existingReceipt(write, commandId, requestHash);
    if (committed) return committed;
    throw new ApiError(409, "COMMENT_CONFLICT", "The comment could not be published.");
  }
}

async function setPostFlag(holdingId: string, postId: string, active: boolean, commandId: string, context: WorkContext, type: "reaction" | "follow") {
  const { write } = await holdingResources(holdingId);
  const post = await visiblePost(write, postId, context);
  const operation = type === "reaction" ? "social.post.appreciate" : "social.post.follow";
  const requestHash = await sha256(JSON.stringify({ operation, postId, active }));
  const prior = await existingReceipt(write, commandId, requestHash);
  if (prior) return prior;
  const now = Date.now(); const result = { postId, active, commandId };
  const mutation = type === "reaction"
    ? active
      ? write.prepare("INSERT INTO social_reactions(post_id,principal_id,kind,created_at) VALUES(?,?,'appreciate',?) ON CONFLICT DO NOTHING").bind(postId, context.userId, now)
      : write.prepare("DELETE FROM social_reactions WHERE post_id=? AND principal_id=? AND kind='appreciate'").bind(postId, context.userId)
    : active
      ? write.prepare("INSERT INTO social_follows(principal_id,target_type,target_id,created_at) VALUES(?,'post',?,?) ON CONFLICT DO NOTHING").bind(context.userId, postId, now)
      : write.prepare("DELETE FROM social_follows WHERE principal_id=? AND target_type='post' AND target_id=?").bind(context.userId, postId);
  const table = type === "reaction" ? "social_reactions" : "social_follows";
  const predicate = type === "reaction" ? "post_id=? AND principal_id=? AND kind='appreciate'" : "target_id=? AND principal_id=? AND target_type='post'";
  try {
    await write.batch([
      write.prepare("INSERT INTO command_receipts(command_id,operation,actor_id,request_hash,record_id,result_json,created_at) VALUES(?,?,?,?,?,?,?)")
        .bind(commandId, operation, context.userId, requestHash, postId, JSON.stringify(result), now),
      mutation,
      write.prepare(`INSERT INTO command_assertions(command_id,step,ok) VALUES(?,?,CASE WHEN
        ${active ? "EXISTS" : "NOT EXISTS"}(SELECT 1 FROM ${table} WHERE ${predicate}) THEN 1 ELSE 0 END)`)
        .bind(commandId, type, postId, context.userId),
      write.prepare("INSERT INTO audit_events VALUES(?,?,?,?,?,?,?,?,?,?)").bind(crypto.randomUUID(), commandId,
        context.userId, context.assignmentId, operation, "social_post", postId, post.version, active ? `Enable ${type}` : `Disable ${type}`, now),
      write.prepare("DELETE FROM command_assertions WHERE command_id=?").bind(commandId),
    ]);
    return result;
  } catch {
    const committed = await existingReceipt(write, commandId, requestHash);
    if (committed) return committed;
    throw new ApiError(409, "SOCIAL_ACTION_CONFLICT", "The update changed before this action completed.");
  }
}

export const setAppreciation = (holdingId: string, postId: string, active: boolean, commandId: string, context: WorkContext) =>
  setPostFlag(holdingId, postId, active, commandId, context, "reaction");
export const setFollow = (holdingId: string, postId: string, active: boolean, commandId: string, context: WorkContext) =>
  setPostFlag(holdingId, postId, active, commandId, context, "follow");

export async function createStory(holdingId: string, input: z.infer<typeof createStorySchema>, commandId: string, context: WorkContext) {
  const { write } = await holdingResources(holdingId);
  if (input.audience === "branch" && !context.branchId) throw new ApiError(422, "BRANCH_REQUIRED", "Select a branch assignment to publish to a branch.");
  const requestHash = await sha256(JSON.stringify({ operation: "social.story.publish", input }));
  const prior = await existingReceipt(write, commandId, requestHash); if (prior) return prior;
  const id = crypto.randomUUID(); const now = Date.now(); const branchId = input.audience === "branch" ? context.branchId : null;
  const result = { id, state: "published", version: 1, commandId };
  try {
    await write.batch([
      write.prepare("INSERT INTO command_receipts(command_id,operation,actor_id,request_hash,record_id,result_json,created_at) VALUES(?,?,?,?,?,?,?)")
        .bind(commandId, "social.story.publish", context.userId, requestHash, id, JSON.stringify(result), now),
      write.prepare(`INSERT INTO social_stories(id,author_id,author_name,company_id,branch_id,department_id,title,body,theme,state,version,last_command_id,created_at,updated_at)
        VALUES(?,?,?,?,?,?,?,?,?, 'published',1,?,?,?)`).bind(id, context.userId, context.name, context.companyId, branchId, context.departmentId,
        input.title, input.body, input.theme, commandId, now, now),
      write.prepare(`INSERT INTO command_assertions(command_id,step,ok) VALUES(?,?,CASE WHEN EXISTS(
        SELECT 1 FROM social_stories WHERE id=? AND last_command_id=? AND state='published') THEN 1 ELSE 0 END)`)
        .bind(commandId, "story", id, commandId),
      write.prepare("INSERT INTO audit_events VALUES(?,?,?,?,?,?,?,?,?,?)").bind(crypto.randomUUID(), commandId,
        context.userId, context.assignmentId, "social.story.publish", "social_story", id, 1, "Publish workplace story", now),
      write.prepare("INSERT INTO outbox_events(id,command_id,event_type,aggregate_type,aggregate_id,aggregate_version,payload_json,created_at) VALUES(?,?,?,?,?,?,?,?)")
        .bind(crypto.randomUUID(), commandId, "social.story.published", "social_story", id, 1, JSON.stringify({ ...result, companyId: context.companyId, branchId }), now),
      write.prepare("DELETE FROM command_assertions WHERE command_id=?").bind(commandId),
    ]); return result;
  } catch {
    const committed = await existingReceipt(write, commandId, requestHash); if (committed) return committed;
    throw new ApiError(409, "STORY_CONFLICT", "The story could not be published.");
  }
}

export async function archiveStory(holdingId: string, storyId: string, expectedVersion: number, commandId: string, context: WorkContext) {
  const { write } = await holdingResources(holdingId);
  const requestHash = await sha256(JSON.stringify({ operation: "social.story.archive", storyId, expectedVersion }));
  const prior = await existingReceipt(write, commandId, requestHash); if (prior) return prior;
  const now = Date.now(); const nextVersion = expectedVersion + 1;
  const result = { id: storyId, state: "archived", version: nextVersion, commandId };
  try {
    await write.batch([
      write.prepare("INSERT INTO command_receipts(command_id,operation,actor_id,request_hash,record_id,result_json,created_at) VALUES(?,?,?,?,?,?,?)")
        .bind(commandId, "social.story.archive", context.userId, requestHash, storyId, JSON.stringify(result), now),
      write.prepare(`UPDATE social_stories SET state='archived',version=?,last_command_id=?,updated_at=?
        WHERE id=? AND author_id=? AND version=? AND state='published' AND company_id=?
        AND (?::text IS NULL OR branch_id=?) AND (?::text IS NULL OR department_id=?)`)
        .bind(nextVersion, commandId, now, storyId, context.userId, expectedVersion, context.companyId,
          context.branchId, context.branchId, context.departmentId, context.departmentId),
      write.prepare(`INSERT INTO command_assertions(command_id,step,ok) VALUES(?,?,CASE WHEN EXISTS(
        SELECT 1 FROM social_stories WHERE id=? AND version=? AND state='archived' AND last_command_id=?) THEN 1 ELSE 0 END)`)
        .bind(commandId, "archive-story", storyId, nextVersion, commandId),
      write.prepare("INSERT INTO audit_events VALUES(?,?,?,?,?,?,?,?,?,?)").bind(crypto.randomUUID(), commandId,
        context.userId, context.assignmentId, "social.story.archive", "social_story", storyId, nextVersion, "Archive workplace story", now),
      write.prepare("DELETE FROM command_assertions WHERE command_id=?").bind(commandId),
    ]); return result;
  } catch {
    const committed = await existingReceipt(write, commandId, requestHash); if (committed) return committed;
    throw new ApiError(409, "STORY_CONFLICT", "Only the author can archive the current story.");
  }
}
