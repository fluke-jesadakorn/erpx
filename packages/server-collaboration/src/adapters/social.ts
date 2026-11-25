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
import {socialFollowPostExists} from './read-tail-sql';
import {nativeInboxSqlFragment} from './native-inbox-statements';
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
function filterSql(filter: FeedFilter, topicExists: string = "") {
    if (filter === "team")
        return " AND p.kind='team'";
    if (filter === "records")
        return " AND p.kind='record'";
    if (filter === "following")
        return ` AND (
    ${socialFollowPostExists("'post'",'p.id')}
    OR ${socialFollowPostExists("'person'",'p.author_id')}
    ${topicExists}`;
    return "";
}
function application(db: SqlDatabase,purchase:Awaited<ReturnType<typeof workSocialReadScope>>={prefix:'',where:'TRUE',args:[]}) {
    return createSocialApplication({ async feed(context, filter) {
            const scope = scopeSql(context);
            const followingBindings = filter === "following" ? [context.userId, context.userId, context.userId] : [];
            const [head, appreciations, commentCount, appreciated, topics, topicExists, commentsByPost, storiesHead] = await Promise.all([
                nativeInboxSqlFragment(context.companyId, 'social_feed_head'),
                nativeInboxSqlFragment(context.companyId, 'social_feed_appreciations'),
                nativeInboxSqlFragment(context.companyId, 'social_feed_comment_count'),
                nativeInboxSqlFragment(context.companyId, 'social_feed_appreciated'),
                nativeInboxSqlFragment(context.companyId, 'social_feed_topics'),
                filter === "following" ? nativeInboxSqlFragment(context.companyId, 'social_following_topic_exists') : Promise.resolve(""),
                nativeInboxSqlFragment(context.companyId, 'social_feed_comments_by_post'),
                nativeInboxSqlFragment(context.companyId, 'social_stories_head'),
            ]);
            const response = await db.prepare(`${purchase.prefix}${head}${appreciations}${commentCount}${appreciated}${socialFollowPostExists("'post'",'p.id')} AS followed,\n      ${topics}${scope.sql}${filterSql(filter, topicExists)} AND ${purchase.where}
    ORDER BY p.created_at DESC LIMIT 30`)
                .bind(...purchase.args,context.userId, context.userId, ...scope.bindings, ...followingBindings)
                .all<Record<string, unknown>>();
            const rows = response.results;
            const commentStatements = rows.map((row) => db.prepare(commentsByPost).bind(row.id));
            const comments = commentStatements.length ? await db.batch(commentStatements) : [];
            return { rows, comments };
        }, async stories(context) {
            const scope = scopeSql(context, "social_stories");
            const [storiesHead] = await Promise.all([nativeInboxSqlFragment(context.companyId, 'social_stories_head')]);
            const result = await db.prepare(`${purchase.prefix}${storiesHead}${scope.sql} AND ${purchase.where} ORDER BY created_at DESC LIMIT 20`)
                .bind(...purchase.args,...scope.bindings).all<Record<string, unknown>>();
            return result;
        } });
}
export function listFeed(holdingId: string, context: WorkContext, filter: FeedFilter = "all") { return holdingResources(holdingId).then(async({ write: db }) => exposed(() => workSocialReadScope(db,context).then(scope=>application(db,scope).listFeed(holdingId, context, filter)))); }
export function listStories(holdingId: string, context: WorkContext) { return holdingResources(holdingId).then(async({ write: db }) => exposed(() => workSocialReadScope(db,context,true).then(scope=>application(db,scope).listStories(holdingId, context)))); }
export { feedFilterSchema, createPostSchema, createCommentSchema, setActiveSchema, createStorySchema } from '@erpx/server-collaboration/domain/social';
export type { FeedFilter, SocialComment, SocialPost } from '@erpx/server-collaboration/domain/social';
