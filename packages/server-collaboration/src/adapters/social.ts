import { z } from 'zod';
import { createStorySchema } from '@erpx/server-collaboration/domain/social';
import 'server-only';
import {assertion} from "@erpx/server-workflow/public/command-assertions";
import { sha256 } from '@erpx/server-runtime/public/crypto';
import { holdingResources } from '../../../../packages/server-runtime/src/public/runtime';
import type { SqlDatabase, SqlStatement } from '@erpx/server-runtime/public/runtime/types';
import { ApiError } from '@erpx/server-runtime/public/http';
import { BusinessRuleError } from '@erpx/server-shared/index';
import { collaborationSocialSources as workflow } from '@erpx/server-workflow/public/collaboration-social-sources';
import {socialPostScope} from '@erpx/server-collaboration/public/social-source-policy';
import type { WorkContext } from '@erpx/server-identity_access/public/authorization';
import type { FeedFilter } from '@erpx/server-collaboration/domain/social';
import { createSocialApplication, type SocialRepository } from '@erpx/server-collaboration/application/social';
import {workSocialReadScope} from '@erpx/server-collaboration/public/work-social-read-policy';
import {withWorkSocialCommand} from '@erpx/server-collaboration/adapters/work-social-purchases';
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
    const repository: SocialRepository<SqlStatement> = { prepareStory(id: string, userId: string, name: string, companyId: string, branchId: string | null, departmentId: string | null, title: string, body: string, theme: "sage" | "sunset" | "sky" | "plum", commandId: string, now: number, now2: number) {
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
        }, ...workflow(db) };
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
        } });
}
export function listFeed(holdingId: string, context: WorkContext, filter: FeedFilter = "all") { return holdingResources(holdingId).then(async({ write: db }) => exposed(() => workSocialReadScope(db,context).then(scope=>application(db,scope).listFeed(holdingId, context, filter)))); }
export function listStories(holdingId: string, context: WorkContext) { return holdingResources(holdingId).then(async({ write: db }) => exposed(() => workSocialReadScope(db,context,true).then(scope=>application(db,scope).listStories(holdingId, context)))); }
export function createStory(holdingId: string, input: z.infer<typeof createStorySchema>, commandId: string, context: WorkContext) { return holdingResources(holdingId).then(({ write: db }) => withWorkSocialCommand(db,context,{key:commandId,operation:'social.story.publish',type:'story',purpose:'new-work'},tx=>exposed(() => application(tx).createStory(holdingId, input, commandId, context)))); }
export function archiveStory(holdingId: string, storyId: string, expectedVersion: number, commandId: string, context: WorkContext) { return holdingResources(holdingId).then(({ write: db }) => withWorkSocialCommand(db,context,{key:commandId,operation:'social.story.archive',type:'story',id:storyId,purpose:'settle-existing'},tx=>exposed(() => application(tx).archiveStory(holdingId, storyId, expectedVersion, commandId, context)))); }
export { feedFilterSchema, createPostSchema, createCommentSchema, setActiveSchema, createStorySchema } from '@erpx/server-collaboration/domain/social';
export type { FeedFilter, SocialComment, SocialPost } from '@erpx/server-collaboration/domain/social';
