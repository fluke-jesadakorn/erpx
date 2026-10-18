import { z } from 'zod';
import { createPostSchema, createCommentSchema, createStorySchema } from '../domain/social';
import 'server-only';
import { assertion } from '../../workflow/public/commands';
import { sha256 } from '../../runtime/public/crypto';
import { holdingResources, runtimeEnv } from '../../runtime/public/runtime';
import type { SqlDatabase, SqlStatement } from '../../runtime/public/runtime/types';
import { ApiError } from '../../runtime/public/http';
import { BusinessRuleError } from '../../shared';
import { getRecord } from '../../records/public/records';
import { getLeave } from '../../people_payroll/public/leave';
import { collaborationMentionPrincipals } from '../../identity_access/public/collaboration-mention-principals';
import { collaborationSocialSources as identity } from '../../identity_access/public/collaboration-social-sources';
import { collaborationSocialSources as workflow } from '../../workflow/public/collaboration-social-sources';
import { socialPostScope } from '../public/social-source';
import type { WorkContext } from '../../identity_access/public/authorization';
import type { FeedFilter } from '../domain/social';
import { createSocialApplication, type SocialRepository } from '../application/social';
import {workSocialReadScope,withWorkSocialCommand} from './work-social-purchases';
async function inward<T>(action: () => Promise<T>) { try {
    return await action();
}
catch (error) {
    if (error instanceof ApiError)
        throw new BusinessRuleError(error.status, error.code, error.message);
    throw error;
} }
async function exposed<T>(action: () => Promise<T>) { try {
    return await action();
}
catch (error) {
    if (error instanceof BusinessRuleError)
        throw new ApiError(error.status, error.code, error.message);
    throw error;
} }
function scopeSql(context: WorkContext, alias = "p") {
    return socialPostScope(context, alias);
}
function filterSql(filter: FeedFilter) {
    if (filter === "team")
        return " AND p.kind='team'";
    if (filter === "records")
        return " AND p.kind='record'";
    if (filter === "following")
        return ` AND (
    EXISTS(SELECT 1 FROM social_follows f WHERE f.principal_id=? AND f.target_type='post' AND f.target_id=p.id)
    OR EXISTS(SELECT 1 FROM social_follows f WHERE f.principal_id=? AND f.target_type='person' AND f.target_id=p.author_id)
    OR EXISTS(SELECT 1 FROM social_post_topics pt JOIN social_follows f ON f.target_type='topic' AND f.target_id=pt.tag
      WHERE pt.post_id=p.id AND f.principal_id=?))`;
    return "";
}
function application(db: SqlDatabase,purchase:Awaited<ReturnType<typeof workSocialReadScope>>={prefix:'',where:'TRUE',args:[]}) {
    const repository: SocialRepository<SqlStatement> = { prepareMention(ownerType: "post" | "comment", ownerId: string, kind: "person" | "record", targetId: string, label: string, userId: string, now: number) { return db.prepare(`INSERT INTO social_mentions(owner_type,owner_id,target_kind,target_id,target_label,created_by,created_at) VALUES(?,?,?,?,?,?,?)`).bind(ownerType, ownerId, kind, targetId, label, userId, now); }, preparePost(id: string, value: "record" | "team", userId: string, name: string, companyId: string, branchId: string | null, value2: null, body: string, value3: "people_payroll" | null, value4: string | null, linkedLabel: string | null, value5: "published", value6: 1, commandId: string, now: number, now2: number, value7: "branch" | "company") {
            return db.prepare(`INSERT INTO social_posts(id,kind,author_id,author_name,company_id,branch_id,department_id,body,linked_module,
      linked_record_id,linked_record_label,state,version,last_command_id,created_at,updated_at,audience_kind)
      VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?)`).bind(id, value, userId, name, companyId, branchId, value2, body, value3, value4, linkedLabel, value5, value6, commandId, now, now2, value7);
        }, prepareTopic(tag: string, userId: string, now: number) { return db.prepare("INSERT INTO social_topics(tag,created_by,created_at) VALUES(?,?,?) ON CONFLICT DO NOTHING").bind(tag, userId, now); }, preparePostTopic(id: string, tag: string) { return db.prepare("INSERT INTO social_post_topics(post_id,tag) VALUES(?,?)").bind(id, tag); }, guardPublishedPost(commandId: string, value: "publish", id: string, commandId2: string, companyId: string) {
            return assertion(db, commandId, value, `EXISTS(
      SELECT 1 FROM social_posts WHERE id=? AND last_command_id=? AND company_id=? AND state='published'
    )`, [id, commandId2, companyId]);
        }, prepareComment(id: string, postId: string, userId: string, name: string, body: string, commandId: string, now: number, now2: number) {
            return db.prepare("INSERT INTO social_comments(id,post_id,author_id,author_name,body,state,last_command_id,created_at,updated_at) VALUES(?,?,?,?,?,'published',?,?,?)")
                .bind(id, postId, userId, name, body, commandId, now, now2);
        }, guardPublishedComment(commandId: string, value: "comment", id: string, postId: string, commandId2: string) {
            return assertion(db, commandId, value, `EXISTS(
        SELECT 1 FROM social_comments WHERE id=? AND post_id=? AND last_command_id=?
      )`, [id, postId, commandId2]);
        }, prepareAppreciation(postId: string, userId: string, now: number) { return db.prepare("INSERT INTO social_reactions(post_id,principal_id,kind,created_at) VALUES(?,?,'appreciate',?) ON CONFLICT DO NOTHING").bind(postId, userId, now); }, prepareAppreciationRemoval(postId: string, userId: string) { return db.prepare("DELETE FROM social_reactions WHERE post_id=? AND principal_id=? AND kind='appreciate'").bind(postId, userId); }, prepareFollow(userId: string, postId: string, now: number) { return db.prepare("INSERT INTO social_follows(principal_id,target_type,target_id,created_at) VALUES(?,'post',?,?) ON CONFLICT DO NOTHING").bind(userId, postId, now); }, prepareFollowRemoval(userId: string, postId: string) { return db.prepare("DELETE FROM social_follows WHERE principal_id=? AND target_type='post' AND target_id=?").bind(userId, postId); }, prepareStory(id: string, userId: string, name: string, companyId: string, branchId: string | null, departmentId: string | null, title: string, body: string, theme: "sage" | "sunset" | "sky" | "plum", commandId: string, now: number, now2: number) {
            return db.prepare(`INSERT INTO social_stories(id,author_id,author_name,company_id,branch_id,department_id,title,body,theme,state,version,last_command_id,created_at,updated_at)
        VALUES(?,?,?,?,?,?,?,?,?, 'published',1,?,?,?)`).bind(id, userId, name, companyId, branchId, departmentId, title, body, theme, commandId, now, now2);
        }, guardPublishedStory(commandId: string, value: "story", id: string, commandId2: string) {
            return assertion(db, commandId, value, `EXISTS(
        SELECT 1 FROM social_stories WHERE id=? AND last_command_id=? AND state='published')`, [id, commandId2]);
        }, prepareArchiveStory(nextVersion: number, commandId: string, now: number, storyId: string, userId: string, expectedVersion: number, companyId: string, branchId: string | null, branchId2: string | null, departmentId: string | null, departmentId2: string | null) {
            return db.prepare(`UPDATE social_stories SET state='archived',version=?,last_command_id=?,updated_at=?
        WHERE id=? AND author_id=? AND version=? AND state='published' AND company_id=?
        AND (?::text IS NULL OR branch_id=?) AND (?::text IS NULL OR department_id=?)`)
                .bind(nextVersion, commandId, now, storyId, userId, expectedVersion, companyId, branchId, branchId2, departmentId, departmentId2);
        }, guardArchivedStory(commandId: string, value: "archive-story", storyId: string, nextVersion: number, commandId2: string) {
            return assertion(db, commandId, value, `EXISTS(
        SELECT 1 FROM social_stories WHERE id=? AND version=? AND state='archived' AND last_command_id=?)`, [storyId, nextVersion, commandId2]);
        }, ...identity(db), ...workflow(db) };
    return createSocialApplication({ repository, clock: { now: () => Date.now() }, identifiers: { next: () => crypto.randomUUID() }, sha256, commit: effects => db.batch(effects), async feed(context, filter) {
            const scope = scopeSql(context);
            const followingBindings = filter === "following" ? [context.userId, context.userId, context.userId] : [];
            const response = await db.prepare(`${purchase.prefix}SELECT p.*,
      (SELECT count(*) FROM social_reactions r WHERE r.post_id=p.id AND r.kind='appreciate') AS appreciation_count,
      (SELECT count(*) FROM social_comments c WHERE c.post_id=p.id AND c.state='published') AS comment_count,
      EXISTS(SELECT 1 FROM social_reactions r WHERE r.post_id=p.id AND r.principal_id=? AND r.kind='appreciate') AS appreciated,
      EXISTS(SELECT 1 FROM social_follows f WHERE f.principal_id=? AND f.target_type='post' AND f.target_id=p.id) AS followed,
      COALESCE((SELECT string_agg(pt.tag, ',' ORDER BY pt.tag) FROM social_post_topics pt WHERE pt.post_id=p.id),'') AS topic_list
    FROM social_posts p WHERE ${scope.sql}${filterSql(filter)} AND ${purchase.where}
    ORDER BY p.created_at DESC LIMIT 30`)
                .bind(...purchase.args,context.userId, context.userId, ...scope.bindings, ...followingBindings)
                .all<Record<string, unknown>>();
            const rows = response.results;
            const commentStatements = rows.map((row) => db.prepare(`SELECT id,post_id,author_id,author_name,body,created_at
    FROM social_comments WHERE post_id=? AND state='published' ORDER BY created_at LIMIT 50`).bind(row.id));
            const comments = commentStatements.length ? await db.batch(commentStatements) : [];
            return { rows, comments };
        }, async stories(context) {
            const scope = scopeSql(context, "social_stories");
            const result = await db.prepare(`${purchase.prefix}SELECT id,author_id,author_name,title,body,theme,branch_id,version,created_at
    FROM social_stories WHERE ${scope.sql} AND ${purchase.where} ORDER BY created_at DESC LIMIT 20`)
                .bind(...purchase.args,...scope.bindings).all<Record<string, unknown>>();
            return result;
        }, async visiblePost(context, id) {
            const scope = scopeSql(context);
            const post = await db.prepare(`${purchase.prefix}SELECT id,version FROM social_posts p WHERE p.id=? AND ${scope.sql} AND ${purchase.where}`)
                .bind(...purchase.args,id, ...scope.bindings).first<{
                id: string;
                version: number;
            }>();
            return post;
        }, mentionPrincipal: async (id) => (await collaborationMentionPrincipals((await runtimeEnv()).CONTROL_DB, [id]))[0] ?? null, linkedLeave: (...args) => inward(() => getLeave(...args)), getRecord: (...args) => inward(() => getRecord(...args)), flagGuard(key, type, post, user, active) { const table = type === 'reaction' ? 'social_reactions' : 'social_follows', predicate = type === 'reaction' ? "post_id=? AND principal_id=? AND kind='appreciate'" : "target_id=? AND principal_id=? AND target_type='post'"; return assertion(db, key, type, `${active ? 'EXISTS' : 'NOT EXISTS'}(SELECT 1 FROM ${table} WHERE ${predicate})`, [post, user]); } });
}
export function listFeed(holdingId: string, context: WorkContext, filter: FeedFilter = "all") { return holdingResources(holdingId).then(async({ write: db }) => exposed(() => workSocialReadScope(db,context).then(scope=>application(db,scope).listFeed(holdingId, context, filter)))); }
export function listStories(holdingId: string, context: WorkContext) { return holdingResources(holdingId).then(async({ write: db }) => exposed(() => workSocialReadScope(db,context,true).then(scope=>application(db,scope).listStories(holdingId, context)))); }
export function createPost(holdingId: string, input: z.infer<typeof createPostSchema>, commandId: string, context: WorkContext) { return holdingResources(holdingId).then(({ write: db }) => withWorkSocialCommand(db,context,{key:commandId,operation:'social.post.publish',type:'post',purpose:'new-work'},tx=>exposed(() => application(tx).createPost(holdingId, input, commandId, context)))); }
export function createComment(holdingId: string, postId: string, input: z.infer<typeof createCommentSchema>, commandId: string, context: WorkContext) { return holdingResources(holdingId).then(({ write: db }) => withWorkSocialCommand(db,context,{key:commandId,operation:'social.comment.publish',type:'post',id:postId,purpose:'settle-existing'},async tx=>exposed(() => workSocialReadScope(tx,context).then(scope=>application(tx,scope).createComment(holdingId, postId, input, commandId, context))))); }
export function setAppreciation(holdingId: string, postId: string, active: boolean, commandId: string, context: WorkContext) { return holdingResources(holdingId).then(({ write: db }) => withWorkSocialCommand(db,context,{key:commandId,operation:'social.post.appreciate',type:'post',id:postId,purpose:active?'new-work':'settle-existing'},async tx=>exposed(() => workSocialReadScope(tx,context).then(scope=>application(tx,scope).setAppreciation(holdingId, postId, active, commandId, context))))); }
export function setFollow(holdingId: string, postId: string, active: boolean, commandId: string, context: WorkContext) { return holdingResources(holdingId).then(({ write: db }) => withWorkSocialCommand(db,context,{key:commandId,operation:'social.post.follow',type:'post',id:postId,purpose:active?'new-work':'settle-existing'},async tx=>exposed(() => workSocialReadScope(tx,context).then(scope=>application(tx,scope).setFollow(holdingId, postId, active, commandId, context))))); }
export function createStory(holdingId: string, input: z.infer<typeof createStorySchema>, commandId: string, context: WorkContext) { return holdingResources(holdingId).then(({ write: db }) => withWorkSocialCommand(db,context,{key:commandId,operation:'social.story.publish',type:'story',purpose:'new-work'},tx=>exposed(() => application(tx).createStory(holdingId, input, commandId, context)))); }
export function archiveStory(holdingId: string, storyId: string, expectedVersion: number, commandId: string, context: WorkContext) { return holdingResources(holdingId).then(({ write: db }) => withWorkSocialCommand(db,context,{key:commandId,operation:'social.story.archive',type:'story',id:storyId,purpose:'settle-existing'},tx=>exposed(() => application(tx).archiveStory(holdingId, storyId, expectedVersion, commandId, context)))); }
export { feedFilterSchema, createPostSchema, createCommentSchema, setActiveSchema, createStorySchema } from '../domain/social';
export type { FeedFilter, SocialComment, SocialPost } from '../domain/social';
