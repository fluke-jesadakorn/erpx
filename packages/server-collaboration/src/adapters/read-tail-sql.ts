/** Collaboration read-tail fragments shared across owner adapters. Text only: binds and locks are unchanged. */
export const socialTaskOriginPostExists=`EXISTS(SELECT 1 FROM social_task_origins origin WHERE origin.task_id=t.id AND origin.post_id=post.id)`;
export const socialTaskOriginByTask=`SELECT post_id FROM social_task_origins WHERE task_id=?`;
export const socialTaskOriginByPost=`SELECT task_id FROM social_task_origins WHERE post_id=?`;
export const messagingRoomMembersAgg=`(SELECT jsonb_agg(to_jsonb(m) ORDER BY m.user_id) FROM messaging_room_members m WHERE m.room_id=r.id)`;
export const socialFollowPostExists=(targetType:string,targetId:string)=>`EXISTS(SELECT 1 FROM social_follows f WHERE f.principal_id=? AND f.target_type=${targetType} AND f.target_id=${targetId})`;
export const messagingTeamMemberExists=`EXISTS(SELECT 1 FROM messaging_team_members m WHERE m.team_id=t.id AND m.user_id=?)`;
/** Byte-identical managed/team-member access guard shared by the single-team and
 * team-list readers in messaging.ts. Only the leading id predicate, the managed
 * team SQL and the trailing ORDER BY vary; the fixed company scope, bind order
 * and lock semantics stay unchanged. */
export const messagingTeamAccessSql=(prefix:string,managedSql:string,suffix:string)=>`SELECT t.* FROM messaging_teams t WHERE ${prefix}t.company_id=? AND ((NOT t.organization_managed AND ${messagingTeamMemberExists}) OR (t.organization_managed AND ${managedSql}))${suffix}`;
export const legacyNativeUnion=`(SELECT * FROM legacy_rows UNION ALL SELECT * FROM native_rows)`;
export const workforceActorScopeColumns=`(SELECT ?::text company_id,?::text branch_id,?::text department_id)`;
export const messagingTeamMembersAgg=(teamId:string)=>`(SELECT jsonb_agg(to_jsonb(m) ORDER BY m.user_id) FROM messaging_team_members m WHERE m.team_id=${teamId})`;
export const workSocialPostsAgg=`(SELECT jsonb_agg(to_jsonb(p) ORDER BY p.id) FROM posts p)`;
export const workSocialCommentsAgg=`(SELECT jsonb_agg(to_jsonb(c) ORDER BY c.id) FROM social_comments c JOIN posts p ON p.id=c.post_id)`;
export const workSocialSourcesAgg=`(SELECT jsonb_agg(to_jsonb(s) ORDER BY s.post_id,s.ordinal) FROM social_post_sources s JOIN posts p ON p.id=s.post_id)`;
export const requestedSourcesJsonb=`(SELECT kind,id FROM jsonb_to_recordset(?::jsonb) AS r(kind text,id text))`;
export const jsonbTextIds=`(SELECT jsonb_array_elements_text(?::jsonb))`;
export const workforceActorScopeExists=(where:string)=>`EXISTS(SELECT 1 FROM ${workforceActorScopeColumns} workforce_actor_scope WHERE ${where})`;
export const messagingRoomMemberExists=`EXISTS(SELECT 1 FROM messaging_room_members m WHERE m.room_id=r.id AND m.user_id=?)`;
export const messagingRoomTeamId=`(SELECT team_id FROM messaging_rooms WHERE id=?)`;
/** Byte-identical monthly AI-usage projection shared by the budget, records and
 * agent readers. Only the projected column list varies; the fixed company/month
 * bind order, lock semantics and notification helpers stay unchanged. */
export const socialAiMonthlyUsageCount=(cols:string)=>`SELECT ${cols} FROM social_ai_usage WHERE company_id=? AND month=?`;
/** Byte-identical published-comment read shared by the agent, Social home and
 * Social home-read call sites. Only the projected column list varies; the fixed
 * `id=? AND post_id=? AND state='published'` prefix, bind order and lock
 * semantics stay unchanged. */
export const publishedCommentRead=(cols:string)=>`SELECT ${cols} FROM social_comments WHERE id=? AND post_id=? AND state='published'`;
/** Byte-identical draft identity read shared by the Social home, purchase and
 * draft-policy call sites. Only the projected column list varies; the fixed
 * `id=? AND company_id=?` prefix, bind order and lock semantics stay unchanged. */
export const socialPostDraftById=(cols:string)=>`SELECT ${cols} FROM social_post_drafts WHERE id=? AND company_id=?`;
export const socialPurchaseActorFeature=(predicate:string)=>`SELECT 1 FROM social_purchase_actor WHERE ${predicate}`;
export const socialPurchaseActorExists=(predicate:string)=>`EXISTS(${socialPurchaseActorFeature(predicate)})`;
export const socialPostByIdPredicate=(predicate:string)=>`SELECT 1 FROM social_posts p WHERE p.id=? AND p.company_id=? AND ${predicate}`;
/** Byte-identical recipient/participant existence guard shared by the handover
 * receipts projection and the Work package-obligation handover facts. Only the
 * trailing predicate varies; the fixed `recipient.post_id=p.id AND
 * recipient.user_id<>p.author_id` head, bind order and lock semantics stay. */
export const socialPostRecipientExists=(tail:string)=>`EXISTS(SELECT 1 FROM social_post_participants recipient WHERE recipient.post_id=p.id AND recipient.user_id<>p.author_id AND ${tail})`;
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
/** Byte-identical Social agent-reply state guard shared by the failed-reply and
 * current-lease readers in social-agent.ts. Only the trailing state predicate
 * varies; the fixed `id=?` prefix, bind order and lock semantics stay unchanged. */
export const socialAgentReplyStateGuard=(tail:string)=>`EXISTS(SELECT 1 FROM social_agent_replies WHERE id=? AND ${tail})`;
/** Byte-identical messaging owner/version existence guard shared by the team-owner
 * and owner-version readers in messaging.ts. Only the table and trailing version
 * predicate vary; the fixed `id=? AND company_id=? AND owner_id=?` prefix, bind
 * order and lock semantics stay unchanged. */
export const messagingOwnerGuard=(table:string,tail:string)=>`EXISTS(SELECT 1 FROM ${table} WHERE id=? AND company_id=? AND owner_id=?${tail})`;
/** Byte-identical AI-phase image projection shared by the raw and redacted
 * readers in work-social-purchases.ts. Only the per-field `-ARRAY[...]`
 * redaction suffixes vary; the query heads, bind order and lock semantics
 * stay unchanged. */
export const socialAiPhaseImageSql=(draft:string,usage:string,reply:string)=>`SELECT jsonb_build_object('draft',(SELECT to_jsonb(d)${draft} FROM social_ai_drafts d WHERE d.id=?),'usage',(SELECT to_jsonb(u)${usage} FROM social_ai_usage u WHERE u.id=?),'reply',(SELECT to_jsonb(r)${reply} FROM social_agent_replies r WHERE r.id=?))::text image`;
/** Byte-identical social-post revisions aggregate head shared by the work social
 * purchase image projection and the task purchase-source image projection. Only
 * the trailing WHERE/JOIN expression varies; the aggregate head, bind order and
 * lock semantics stay unchanged. */
export const socialPostRevisionsAgg=(tail:string)=>`(SELECT jsonb_agg(to_jsonb(r) ORDER BY r.id) FROM social_post_revisions r ${tail})`;
/** Byte-identical PMS post-source absence guard shared by the collaboration
 * source-authorization predicate (pms-source-authorization.ts) and the Social
 * source-access policy (social-source-policy.ts). Only the post alias and the
 * trailing condition vary; the fixed `ps.post_id=<post>.id AND
 * ps.source_kind='pms_record'` head, bind order and lock semantics stay. */
export const socialPostSourcePmsGuard=(post:string,tail:string)=>`NOT EXISTS(SELECT 1 FROM social_post_sources ps WHERE ps.post_id=${post}.id AND ps.source_kind='pms_record'${tail})`;
