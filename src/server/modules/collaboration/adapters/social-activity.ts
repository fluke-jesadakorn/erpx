import "server-only";
import type { WorkContext } from "@/server/modules/identity_access/public/authorization";
import { socialPostScope } from "@/server/modules/collaboration/public/social-source";
export type { SocialActivityEvent } from '../domain/social-activity';
/** Exposes only authorized activity metadata; post and reply bodies never enter Executive data. */
export function socialActivityEvents(actor: WorkContext) {
    const scope = socialPostScope(actor);
    return {
        sql: `WITH visible_posts AS (
        SELECT p.id,p.company_id,p.author_name,p.version
        FROM social_posts p WHERE ${scope.sql}
      ), visible_comments AS (
        SELECT c.id,c.post_id,c.author_name,c.version,c.created_at,p.id thread_id
        FROM visible_posts p JOIN social_comments c ON c.post_id=p.id AND c.state='published'
      ), events AS (
        SELECT 'reply:'||c.id id,c.id source_id,'reply'::text source_type,'Conversation reply'::text title,
          c.created_at at,c.version,c.author_name activity_actor,c.thread_id,'/posts/'||c.thread_id href
        FROM visible_comments c
        UNION ALL
        SELECT 'mention:'||md5(m.owner_type||':'||m.owner_id||':'||m.target_kind||':'||m.target_id),
          m.owner_id,'mention'::text,'Conversation mention'::text,m.created_at,
          COALESCE(c.version,p.version)::bigint,COALESCE(c.author_name,p.author_name),p.id,'/posts/'||p.id
        FROM social_mentions m
        LEFT JOIN visible_posts p ON m.owner_type='post' AND m.owner_id=p.id
        LEFT JOIN visible_comments c ON m.owner_type='comment' AND m.owner_id=c.id
        WHERE p.id IS NOT NULL OR c.id IS NOT NULL
      )
      SELECT e.id,e.source_id,e.source_type,e.title,e.at,e.version,e.activity_actor,e.thread_id,e.href FROM events e`,
        values: scope.bindings,
    };
}
