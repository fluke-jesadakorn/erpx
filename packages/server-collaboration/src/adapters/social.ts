import 'server-only';
import { holdingResources } from '../../../../packages/server-runtime/src/public/runtime';
import type { SqlDatabase } from '@erpx/server-runtime/public/runtime/types';
import { ApiError } from '@erpx/server-runtime/public/http';
import { BusinessRuleError } from '@erpx/server-shared/index';
import {socialPostScope} from '@erpx/server-collaboration/public/social-source-policy';
import type { WorkContext } from '@erpx/server-identity_access/public/authorization';
import type { FeedFilter } from '@erpx/server-collaboration/domain/social';
import { createSocialApplication } from '@erpx/server-collaboration/application/social';
import {workSocialReadScope} from '@erpx/server-collaboration/public/work-social-read-policy';
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
    return createSocialApplication({ async feed(context, filter) {
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
export { feedFilterSchema, createPostSchema, createCommentSchema, setActiveSchema, createStorySchema } from '@erpx/server-collaboration/domain/social';
export type { FeedFilter, SocialComment, SocialPost } from '@erpx/server-collaboration/domain/social';
