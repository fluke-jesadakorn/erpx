import "server-only";
import type { WorkContext } from "@erpx/server-identity_access/public/authorization";
import {socialPostScope} from '@erpx/server-collaboration/public/social-source-policy';
import {nativeInboxSqlFragment} from './native-inbox-statements';
export type { SocialActivityEvent } from '@erpx/server-collaboration/domain/social-activity';
/** Exposes only authorized activity metadata; post and reply bodies never enter Executive data. */
export async function socialActivityEvents(actor: WorkContext) {
    const scope = socialPostScope(actor);
    const company = actor.companyId;
    const [visiblePosts, visibleComments, replyArm, mentionArm, finalSelect] = await Promise.all([
        nativeInboxSqlFragment(company, 'social_activity_visible_posts'),
        nativeInboxSqlFragment(company, 'social_activity_visible_comments'),
        nativeInboxSqlFragment(company, 'social_activity_reply_arm'),
        nativeInboxSqlFragment(company, 'social_activity_mention_arm'),
        nativeInboxSqlFragment(company, 'social_activity_final'),
    ]);
    return {
        sql: `${visiblePosts}${scope.sql}${visibleComments}${replyArm}${mentionArm}${finalSelect}`,
        values: scope.bindings,
    };
}
