/** Collaboration read-tail fragments shared across owner adapters. Text only: binds and locks are unchanged. */
export const socialTaskOriginPostExists=`EXISTS(SELECT 1 FROM social_task_origins origin WHERE origin.task_id=t.id AND origin.post_id=post.id)`;
export const socialTaskOriginByTask=`SELECT post_id FROM social_task_origins WHERE task_id=?`;
export const socialTaskOriginByPost=`SELECT task_id FROM social_task_origins WHERE post_id=?`;
export const messagingRoomMembersAgg=`(SELECT jsonb_agg(to_jsonb(m) ORDER BY m.user_id) FROM messaging_room_members m WHERE m.room_id=r.id)`;
export const socialFollowPostExists=`EXISTS(SELECT 1 FROM social_follows f WHERE f.principal_id=? AND f.target_type='post' AND f.target_id=p.id)`;
export const messagingTeamMemberExists=`EXISTS(SELECT 1 FROM messaging_team_members m WHERE m.team_id=t.id AND m.user_id=?)`;
export const legacyNativeUnion=`(SELECT * FROM legacy_rows UNION ALL SELECT * FROM native_rows)`;
export const workforceActorScopeColumns=`(SELECT ?::text company_id,?::text branch_id,?::text department_id)`;
export const messagingTeamMembersAgg=`(SELECT jsonb_agg(to_jsonb(m) ORDER BY m.user_id) FROM messaging_team_members m WHERE m.team_id=r.team_id)`;
export const workSocialPostsAgg=`(SELECT jsonb_agg(to_jsonb(p) ORDER BY p.id) FROM posts p)`;
export const workSocialCommentsAgg=`(SELECT jsonb_agg(to_jsonb(c) ORDER BY c.id) FROM social_comments c JOIN posts p ON p.id=c.post_id)`;
export const workSocialSourcesAgg=`(SELECT jsonb_agg(to_jsonb(s) ORDER BY s.post_id,s.ordinal) FROM social_post_sources s JOIN posts p ON p.id=s.post_id)`;
export const requestedSourcesJsonb=`(SELECT kind,id FROM jsonb_to_recordset(?::jsonb) AS r(kind text,id text))`;
export const jsonbTextIds=`(SELECT jsonb_array_elements_text(?::jsonb))`;
export const workforceActorScopeExists=(where:string)=>`EXISTS(SELECT 1 FROM ${workforceActorScopeColumns} workforce_actor_scope WHERE ${where})`;
export const messagingRoomMemberExists=`EXISTS(SELECT 1 FROM messaging_room_members m WHERE m.room_id=r.id AND m.user_id=?)`;
export const messagingRoomTeamId=`(SELECT team_id FROM messaging_rooms WHERE id=?)`;
export const socialAiMonthlyUsageCount=`SELECT count(*) FROM social_ai_usage WHERE company_id=? AND month=?`;
export const publishedCommentRead=`SELECT 1 FROM social_comments WHERE id=? AND post_id=? AND state='published'`;
export const socialPostDraftById=`SELECT * FROM social_post_drafts WHERE id=? AND company_id=?`;
export const socialPurchaseActorFeature=(predicate:string)=>`SELECT 1 FROM social_purchase_actor WHERE ${predicate}`;
export const socialPurchaseActorExists=(predicate:string)=>`EXISTS(${socialPurchaseActorFeature(predicate)})`;
export const socialPostByIdPredicate=(predicate:string)=>`SELECT 1 FROM social_posts p WHERE p.id=? AND p.company_id=? AND ${predicate}`;
export const messagingMemberIds=(table:string,column:string)=>`SELECT user_id FROM ${table} WHERE ${column}=?`;
export const socialPostTopicExists=(tag:string)=>`EXISTS(SELECT 1 FROM social_post_topics pt WHERE pt.post_id=p.id AND pt.tag=${tag})`;
export const socialCommentTopicExists=(tag:string)=>`EXISTS(SELECT 1 FROM social_comment_topics ct JOIN social_comments c ON c.id=ct.comment_id WHERE c.post_id=p.id AND c.state='published' AND ct.tag=${tag})`;
export const socialCommentAncestorStep=`SELECT parent.* FROM ancestors child JOIN social_comments parent ON parent.id=child.parent_comment_id AND parent.post_id=child.post_id WHERE parent.state='published'`;
export const socialAgentReplyLeaseIdentity=(tail:string)=>`SELECT 1 FROM social_agent_replies WHERE id=? AND company_id=? AND actor_id=? AND post_id=? AND lease_token=? AND actor_json=? AND owner_type=? AND owner_id=?${tail}`;
/** Byte-identical project-purchase source-absence guard shared across the two
 * read-policy call sites in work-social-read-policy.ts. Only the trailing
 * authorization predicate varies; binds and lock semantics are unchanged. */
export const projectPurchaseSourceAbsent=(sourceRows:string,purchaseSources:string,predicate:string)=>`SELECT 1 FROM (${sourceRows}) source JOIN (${purchaseSources}) social_project_source ON social_project_source.source_kind=source.kind AND social_project_source.source_id=source.id AND social_project_source.company_id=p.company_id WHERE NOT (${predicate})`;
/** Byte-identical publish-receipt witness shared between the work package-obligation
 * facts query and the social draft-replay guard. Only the post-id and actor-id column
 * expressions vary; the operation literal, binds and lock semantics are unchanged. */
export const socialPostPublishReceipt=(postId:string,actorId:string)=>`SELECT 1 FROM command_receipts receipt WHERE receipt.operation='social.post.publish' AND receipt.record_id=${postId} AND receipt.actor_id=${actorId} AND receipt.result_json::jsonb->>'id'=${postId}`;
/** Byte-identical Social draft-existence guard shared across the three Social
 * home call sites. Only the trailing field predicate varies; the fixed
 * `id=? AND author_id=?` prefix, bind order and lock semantics stay unchanged. */
export const socialPostDraftExists=(tail:string)=>`EXISTS(SELECT 1 FROM social_post_drafts WHERE id=? AND author_id=? AND ${tail})`;
/** Byte-identical owner-scoped post existence guard shared by the retained Social
 * publish and draft-policy readers. Only the trailing predicate varies; the fixed
 * `id=? AND company_id=?` prefix, bind order and lock semantics stay unchanged. */
export const socialPostByIdGuard=(tail:string)=>`SELECT 1 FROM social_posts WHERE id=? AND company_id=? AND ${tail}`;
/** Byte-identical Social draft-field existence guard shared by the retained comment
 * and rich draft readers in social-home.ts. Only the trailing field predicate
 * varies; the fixed `id=?` prefix, bind order and lock semantics stay unchanged. */
export const socialPostDraftFieldGuard=(tail:string)=>`SELECT 1 FROM social_post_drafts WHERE id=? AND ${tail}`;
