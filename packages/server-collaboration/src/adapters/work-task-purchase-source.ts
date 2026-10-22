import 'server-only';
import {workOperationsSettlementSources} from '@erpx/server-operations/public/work-package-obligation-sources';
import type {SqlDatabase} from '@erpx/server-runtime/public/runtime/types';
import {workSocialNativePostPredicate} from '@erpx/server-collaboration/adapters/work-social-source';
export function workTaskSocialSourceLocks(database:SqlDatabase){return database.prepare('LOCK TABLE social_posts,social_comments,social_post_sources,social_post_revisions,social_task_origins IN SHARE ROW EXCLUSIVE MODE');}
/** Original team audience membership is a source of private conversation
 * authority. Its lease must precede a consumer's first Holding snapshot. */
export function workTaskSocialAudienceSourceLocks(database:SqlDatabase){return database.prepare('LOCK TABLE messaging_teams,messaging_team_members IN SHARE MODE');}
/** A linked-module label never licenses freeform social content. Native
 * non-system posts require Work Social unless actual PMS/Project source FKs
 * prove their included owner. Unknown lineage stays conservative. Resolve
 * native Company + persisted task
 * origin, and return only a private source witness to its command owner. */
export async function workTaskSocialPurchaseSource(database:SqlDatabase,companyId:string,taskId:string,requestedPostId?:string){
 const row=await database.prepare(`WITH origins AS(SELECT * FROM social_task_origins WHERE task_id=?),ids AS(SELECT post_id id FROM origins UNION SELECT ?::text WHERE ?::text IS NOT NULL),posts AS(SELECT p.* FROM social_posts p JOIN ids ON ids.id=p.id WHERE p.company_id=?)
 SELECT COALESCE((SELECT bool_or(${workSocialNativePostPredicate('p')}) FROM posts p),false) work_social,
 jsonb_build_object('posts',(SELECT jsonb_agg(to_jsonb(p) ORDER BY p.id) FROM posts p),'comments',(SELECT jsonb_agg(to_jsonb(c) ORDER BY c.id) FROM social_comments c JOIN posts p ON p.id=c.post_id),'sources',(SELECT jsonb_agg(to_jsonb(s) ORDER BY s.post_id,s.ordinal) FROM social_post_sources s JOIN posts p ON p.id=s.post_id),'revisions',(SELECT jsonb_agg(to_jsonb(r) ORDER BY r.id) FROM social_post_revisions r JOIN posts p ON p.id=r.post_id))::text posts_image,
 (SELECT COALESCE(jsonb_agg(to_jsonb(o) ORDER BY o.post_id),'[]'::jsonb)::text FROM origins o) origin_image`).bind(taskId,requestedPostId??null,requestedPostId??null,companyId).first<{work_social:boolean;posts_image:string;origin_image:string}>();
 return {workSocial:row?.work_social??false,postsImage:row?.posts_image??'',originImage:row?.origin_image??''};
}

/** Fixed native task alias for the owner list statement, before LIMIT. */
export function workTaskSocialPurchasePredicate(){return `EXISTS(SELECT 1 FROM social_task_origins origin JOIN social_posts post ON post.id=origin.post_id AND post.company_id=t.company_id WHERE origin.task_id=t.id AND ${workSocialNativePostPredicate('post')})`;}

/** Closed existing-task source facts for private evidence consumers. Origins
 * without a live same-Company post remain in the witness rather than silently
 * disappearing. This is never a client authority snapshot. */
export const workTaskSocialFileSourceRows=`SELECT t.id,t.company_id,${workTaskSocialPurchasePredicate()} work_social,
 jsonb_build_object('origins',(SELECT jsonb_agg(to_jsonb(origin) ORDER BY origin.post_id) FROM social_task_origins origin WHERE origin.task_id=t.id),
 'posts',(SELECT jsonb_agg(to_jsonb(post) ORDER BY post.id) FROM social_posts post WHERE post.company_id=t.company_id AND EXISTS(SELECT 1 FROM social_task_origins origin WHERE origin.task_id=t.id AND origin.post_id=post.id)),
 'comments',(SELECT jsonb_agg(to_jsonb(comment) ORDER BY comment.id) FROM social_comments comment JOIN social_posts post ON post.id=comment.post_id WHERE post.company_id=t.company_id AND EXISTS(SELECT 1 FROM social_task_origins origin WHERE origin.task_id=t.id AND origin.post_id=post.id)),
 'sources',(SELECT jsonb_agg(to_jsonb(source) ORDER BY source.post_id,source.ordinal) FROM social_post_sources source JOIN social_posts post ON post.id=source.post_id WHERE post.company_id=t.company_id AND EXISTS(SELECT 1 FROM social_task_origins origin WHERE origin.task_id=t.id AND origin.post_id=post.id)),
 'revisions',(SELECT jsonb_agg(to_jsonb(revision) ORDER BY revision.id) FROM social_post_revisions revision JOIN social_posts post ON post.id=revision.post_id WHERE post.company_id=t.company_id AND EXISTS(SELECT 1 FROM social_task_origins origin WHERE origin.task_id=t.id AND origin.post_id=post.id))) image
 FROM (${workOperationsSettlementSources.tasks}) t`;
