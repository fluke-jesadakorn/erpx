/** Collaboration read-tail fragments shared across owner adapters. Text only: binds and locks are unchanged. */
export const socialTaskOriginPostExists=`EXISTS(SELECT 1 FROM social_task_origins origin WHERE origin.task_id=t.id AND origin.post_id=post.id)`;
/** Byte-identical task-origin by-key read shared by the task, beta and people
 * entrypoint readers. Only the projected column and key column vary; the fixed
 * table head, single bind and lock semantics stay unchanged. */
export const socialTaskOriginBy=(cols:string,keyCol:string)=>`SELECT ${cols} FROM social_task_origins WHERE ${keyCol}=?`;
/** Byte-identical Task-origin existence guard shared by the queued read sources,
 * Task read policy and purchase source readers. Only the quantifier, origin alias
 * and joined tail vary; the fixed task-origin existence head, bind order and
 * lock semantics stay unchanged. The r14 socialTaskOriginPostExists helper
 * stays retained. */
export const socialTaskOriginGuard=(quantifier:string,alias:string,tail:string)=>`${quantifier}SELECT 1 FROM social_task_origins ${alias}${tail}`;
/** Byte-identical room/team member aggregate head shared by the attachment
 * purchase witness, the messaging purchase image and the messaging attachment
 * reader. Only the member table and the WHERE scope vary; the fixed aggregate
 * head, bind order and lock semantics stay unchanged. */
export const messagingMembersAgg=(table:string,where:string)=>`(SELECT jsonb_agg(to_jsonb(m) ORDER BY m.user_id) FROM ${table} m WHERE ${where})`;
/** Retained room wrapper: renders the room-member scope through
 * messagingMembersAgg so consumers and bind order stay unchanged. */
export const messagingRoomMembersAgg=messagingMembersAgg('messaging_room_members','m.room_id=r.id');
export const socialFollowPostExists=(targetType:string,targetId:string)=>`EXISTS(SELECT 1 FROM social_follows f WHERE f.principal_id=? AND f.target_type=${targetType} AND f.target_id=${targetId})`;
/** Byte-identical team/room member existence guard shared by the team access,
 * room access and Social team readers. Only the closed team/room scope and the
 * group reference vary; the fixed alias, single user bind and lock semantics
 * stay unchanged. */
export const messagingMemberExists=(table:'messaging_team_members'|'messaging_room_members',groupCol:'team_id'|'room_id',groupRef:string)=>`EXISTS(SELECT 1 FROM ${table} m WHERE m.${groupCol}=${groupRef} AND m.user_id=?)`;
/** Byte-identical team-member existence guard shared by the team owner projection,
 * the room access policy and the command member assertion. Only the alias, team
 * reference and user reference vary; the empty-alias branch renders the original
 * unqualified guard, and bind order and lock semantics stay unchanged. */
export const messagingTeamMemberGuard=(alias:string,teamRef:string,userRef:string)=>`EXISTS(SELECT 1 FROM messaging_team_members${alias?` ${alias}`:''} WHERE ${alias?`${alias}.`:''}team_id=${teamRef} AND ${alias?`${alias}.`:''}user_id=${userRef})`;
/** Byte-identical managed/team-member access guard shared by the single-team and
 * team-list readers in messaging.ts. Only the leading id predicate, the managed
 * team SQL and the trailing ORDER BY vary; the fixed company scope, bind order
 * and lock semantics stay unchanged. */
export const messagingTeamAccessSql=(prefix:string,managedSql:string,suffix:string)=>`SELECT t.* FROM messaging_teams t WHERE ${prefix}t.company_id=? AND ((NOT t.organization_managed AND ${messagingMemberExists('messaging_team_members','team_id','t.id')}) OR (t.organization_managed AND ${managedSql}))${suffix}`;
export const legacyNativeUnion=`(SELECT * FROM legacy_rows UNION ALL SELECT * FROM native_rows)`;
export const workforceActorScopeColumns=`(SELECT ?::text company_id,?::text branch_id,?::text department_id)`;
/** Retained team wrapper: renders the team-member scope through
 * messagingMembersAgg so consumers and bind order stay unchanged. */
export const messagingTeamMembersAgg=(teamId:string)=>messagingMembersAgg('messaging_team_members',`m.team_id=${teamId}`);
/** Byte-identical messaging team image subquery shared by the attachment purchase
 * witness, the private attachment reader and the messaging purchase image. Only
 * the team/company references and the `-'version'` redaction vary; the fixed
 * `messaging_teams t` head, bind order and lock semantics stay unchanged. */
export const messagingTeamImage=(teamRef:string,companyRef:string,redact=false)=>`(SELECT to_jsonb(t)${redact?`-'version'`:''} FROM messaging_teams t WHERE t.id=${teamRef} AND t.company_id=${companyRef})`;
export const workSocialPostsAgg=`(SELECT jsonb_agg(to_jsonb(p) ORDER BY p.id) FROM posts p)`;
export const workSocialCommentsAgg=`(SELECT jsonb_agg(to_jsonb(c) ORDER BY c.id) FROM social_comments c JOIN posts p ON p.id=c.post_id)`;
export const workSocialSourcesAgg=`(SELECT jsonb_agg(to_jsonb(s) ORDER BY s.post_id,s.ordinal) FROM social_post_sources s JOIN posts p ON p.id=s.post_id)`;
export const requestedSourcesJsonb=`(SELECT kind,id FROM jsonb_to_recordset(?::jsonb) AS r(kind text,id text))`;
export const jsonbTextIds=`(SELECT jsonb_array_elements_text(?::jsonb))`;
export const workforceActorScopeExists=(where:string)=>`EXISTS(SELECT 1 FROM ${workforceActorScopeColumns} workforce_actor_scope WHERE ${where})`;
export const messagingRoomTeamId=`(SELECT team_id FROM messaging_rooms WHERE id=?)`;
/** Byte-identical AI-usage read head shared by the agent/hospitality projection
 * and the monthly-count readers. Only the trailing predicate varies; the fixed
 * `social_ai_usage` table head, bind order and lock semantics stay unchanged. */
export const socialAiUsageRead=(cols:string,tail='')=>`SELECT ${cols} FROM social_ai_usage${tail}`;
/** Retained projection wrapper: renders through socialAiUsageRead so consumers
 * and zero-bind semantics stay unchanged. */
export const socialAiUsageProjection=(cols:string)=>socialAiUsageRead(cols);
/** Retained monthly-count wrapper: renders through socialAiUsageRead so the
 * company/month bind order and lock semantics stay unchanged. */
export const socialAiMonthlyUsageCount=(cols:string)=>socialAiUsageRead(cols,' WHERE company_id=? AND month=?');
/** Byte-identical published-comment read shared by the agent, Social home and
 * Social home-read call sites. Only the projected column list varies; the fixed
 * `id=? AND post_id=? AND state='published'` prefix, bind order and lock
 * semantics stay unchanged. */
export const publishedCommentRead=(cols:string)=>`SELECT ${cols} FROM social_comments WHERE id=? AND post_id=? AND state='published'`;
/** Byte-identical by-table Social target image / draft identity head shared by
 * the purchase target, attachment root, draft-policy, home and handover readers.
 * Only the table, projection, alias, WHERE qualifier, whitespace gap and trailing
 * predicate vary; the fixed `id=? AND company_id=?` binds and locks stay unchanged. */
export const socialTargetImage=(table:string,cols:string,alias='',whereAlias='',gap=' ',tail='')=>`SELECT ${cols} FROM ${table}${alias?` ${alias}`:''}${gap}WHERE ${whereAlias?`${whereAlias}.`:''}id=? AND ${whereAlias?`${whereAlias}.`:''}company_id=?${tail}`;
/** Byte-identical draft identity read shared by the Social home, purchase, draft
 * policy and handover readers. Only the projected column list, whitespace gap and
 * trailing predicate vary; the fixed `id=? AND company_id=?` prefix, bind order
 * and lock semantics stay unchanged. */
export const socialPostDraftById=(cols:string,gap=' ',tail='')=>socialTargetImage('social_post_drafts',cols,'','',gap,tail);
export const socialPurchaseActorFeature=(predicate:string)=>`SELECT 1 FROM social_purchase_actor WHERE ${predicate}`;
export const socialPurchaseActorExists=(predicate:string)=>`EXISTS(${socialPurchaseActorFeature(predicate)})`;
export const socialPostByIdPredicate=(predicate:string)=>`SELECT 1 FROM social_posts p WHERE p.id=? AND p.company_id=? AND ${predicate}`;
/** Byte-identical recipient/participant existence guard shared by the handover
 * receipts projection and the Work package-obligation handover facts. Only the
 * trailing predicate varies; the fixed `recipient.post_id=p.id AND
 * recipient.user_id<>p.author_id` head, bind order and lock semantics stay. */
export const socialPostRecipientExists=(tail:string)=>`EXISTS(SELECT 1 FROM social_post_participants recipient WHERE recipient.post_id=p.id AND recipient.user_id<>p.author_id AND ${tail})`;
/** Byte-identical handover acknowledgement/recipient join head shared by the
 * `acknowledged` and `acknowledgedCount` projections in handover-receipts.ts.
 * Only the projected expression and trailing predicate vary; the fixed join,
 * post/version references, bind order and lock semantics stay unchanged. */
export const socialHandoverAckJoin=(projection:string,tail:string)=>`SELECT ${projection} FROM social_handover_acknowledgements ack JOIN social_post_participants recipient ON recipient.post_id=ack.post_id AND recipient.user_id=ack.user_id WHERE ack.post_id=p.id AND ack.post_version=p.version${tail}`;
export const messagingMemberIds=(table:string,column:string)=>`SELECT user_id FROM ${table} WHERE ${column}=?`;
export const socialPostTopicExists=(tag:string)=>`EXISTS(SELECT 1 FROM social_post_topics pt WHERE pt.post_id=p.id AND pt.tag=${tag})`;
export const socialCommentTopicExists=(tag:string)=>`EXISTS(SELECT 1 FROM social_comment_topics ct JOIN social_comments c ON c.id=ct.comment_id WHERE c.post_id=p.id AND c.state='published' AND ct.tag=${tag})`;
export const socialCommentAncestorStep=`SELECT parent.* FROM ancestors child JOIN social_comments parent ON parent.id=child.parent_comment_id AND parent.post_id=child.post_id WHERE parent.state='published'`;
/** Byte-identical Social comment column head shared by the home comments CTE
 * and the paginated comments-page reader. Only the qualified column list is
 * parameterized; each caller's tail, table, binds and locks stay local. */
export const socialCommentHead=(cols:string)=>`SELECT ${cols},`;
/** Byte-identical Social comment projection shared by the `selected` and
 * `ancestors` UNION arms of the home comments CTE. Only the closed source
 * table varies; the column list, bind order and other arms stay local. */
export const socialCommentProjection=(cols:string,table:'selected'|'ancestors')=>`SELECT ${cols} FROM ${table}`;
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
/** Byte-identical absent-social-post guard shared by the notice-source and
 * social-agent-reply arms of work-package-obligations.ts. Only the closed id and
 * company column references vary; the fixed `social_posts p` head, zero binds
 * and lock semantics stay unchanged. */
export const absentSocialPostGuard=(idRef:string,companyRef:string)=>`NOT EXISTS(SELECT 1 FROM social_posts p WHERE p.id=${idRef} AND p.company_id=${companyRef})`;
/** Byte-identical Social draft-field existence guard shared by the retained comment
 * and rich draft readers in social-home.ts. Only the trailing field predicate
 * varies; the fixed `id=?` prefix, bind order and lock semantics stay unchanged. */
export const socialPostDraftFieldGuard=(tail:string)=>`SELECT 1 FROM social_post_drafts WHERE id=? AND ${tail}`;
/** Byte-identical Social agent-reply state guard shared by the failed-reply and
 * current-lease readers in social-agent.ts. Only the trailing state predicate
 * varies; the fixed `id=?` prefix, bind order and lock semantics stay unchanged. */
export const socialAgentReplyStateGuard=(tail:string)=>`EXISTS(SELECT 1 FROM social_agent_replies WHERE id=? AND ${tail})`;
/** Byte-identical owner/version existence guard shared by the team-owner,
 * owner-version and attachment readers. Only the table, guarded column and
 * trailing predicate vary; the fixed `id=? AND company_id=?` prefix, bind
 * order and lock semantics stay unchanged. */
export const ownerVersionGuard=(table:string,col:string,tail:string)=>`EXISTS(SELECT 1 FROM ${table} WHERE id=? AND company_id=? AND ${col}=?${tail})`;
/** Retained r26 messaging wrapper: renders the `owner_id` column through
 * ownerVersionGuard so existing consumers and bind order stay unchanged. */
export const messagingOwnerGuard=(table:string,tail:string)=>ownerVersionGuard(table,'owner_id',tail);
/** Byte-identical AI-phase image projection shared by the raw and redacted
 * readers in work-social-purchases.ts. Only the per-field `-ARRAY[...]`
 * redaction suffixes vary; the query heads, bind order and lock semantics
 * stay unchanged. */
export const socialAiPhaseImageSql=(draft:string,usage:string,reply:string)=>`SELECT jsonb_build_object('draft',(SELECT to_jsonb(d)${draft} FROM social_ai_drafts d WHERE d.id=?),'usage',(SELECT to_jsonb(u)${usage} FROM social_ai_usage u WHERE u.id=?),'reply',(SELECT to_jsonb(r)${reply} FROM social_agent_replies r WHERE r.id=?))::text image`;
/** Byte-identical social-post revisions aggregate head shared by the work social
 * purchase image projection, the task purchase-source image projection, the work
 * social attachment witness and the task purchase-source file rows. Only the row
 * alias and the trailing WHERE/JOIN expression vary; the aggregate head, bind
 * order and lock semantics stay unchanged. */
export const socialPostRevisionsAggBy=(alias:string,tail:string)=>`(SELECT jsonb_agg(to_jsonb(${alias}) ORDER BY ${alias}.id) FROM social_post_revisions ${alias} ${tail})`;
/** Retained r28 wrapper: renders the `r` alias through socialPostRevisionsAggBy
 * so existing consumers and bind order stay unchanged. */
export const socialPostRevisionsAgg=(tail:string)=>socialPostRevisionsAggBy('r',tail);
/** Retained r37 wrapper: renders the `revision` alias through
 * socialPostRevisionsAggBy so existing consumers and bind order stay unchanged. */
export const socialPostRevisionsFileAgg=(tail:string)=>socialPostRevisionsAggBy('revision',tail);
/** Byte-identical PMS post-source absence guard shared by the collaboration
 * source-authorization predicate (pms-source-authorization.ts) and the Social
 * source-access policy (social-source-policy.ts). Only the post alias and the
 * trailing condition vary; the fixed `ps.post_id=<post>.id AND
 * ps.source_kind='pms_record'` head, bind order and lock semantics stay. */
export const socialPostSourcePmsGuard=(post:string,tail:string)=>`NOT EXISTS(SELECT 1 FROM social_post_sources ps WHERE ps.post_id=${post}.id AND ps.source_kind='pms_record'${tail})`;
/** Byte-identical PMS source-record existence guard shared by the work social
 * source predicates, the product source entry and the draft capabilities reader.
 * Only the fact rows, alias and source/company references vary; the fixed `id`
 * fact column, bind order and lock semantics stay unchanged. */
export const socialPmsSourceFactExists=(rows:string,alias:string,sourceRef:string,companyRef:string)=>`EXISTS(SELECT 1 FROM (${rows}) ${alias} WHERE ${alias}.id=${sourceRef} AND ${alias}.company_id=${companyRef})`;
/** Byte-identical messaging room-by-id read shared by the room assertion in
 * messaging.ts and the room reader in messaging-read.ts. Only the projected
 * column list varies; the fixed `id=? AND company_id=? AND <access>` predicate,
 * bind order and lock semantics stay unchanged. */
export const messagingRoomByIdSql=(cols:string,access:string)=>`SELECT ${cols} FROM messaging_rooms r WHERE r.id=? AND r.company_id=? AND ${access}`;
/** Byte-identical latest-message tail shared by the overview preview and updated-at
 * projections in messaging.ts. Only the projected expression varies; the fixed
 * table head, room reference, ordering, limit and lock semantics stay unchanged,
 * and the caller's COALESCE wrapper stays at the call site. */
export const messagingLatestMessage=(projection:string)=>`(SELECT ${projection} FROM messaging_messages msg WHERE msg.room_id=r.id ORDER BY msg.seq DESC LIMIT 1)`;
/** Byte-identical Social agent-reply identity read shared by the request and
 * retry readers in social-agent.ts. Only the projected column list varies; the
 * fixed `id=? AND company_id=? AND actor_id=?` prefix, bind order and lock
 * semantics stay unchanged. */
export const socialAgentReplyById=(cols:string)=>`SELECT ${cols} FROM social_agent_replies WHERE id=? AND company_id=? AND actor_id=?`;
/** Retained r33 attachment wrapper: renders the `version` column through
 * ownerVersionGuard so existing consumers and bind order stay unchanged. */
export const attachmentVersionGuard=(table:string,tail:string)=>ownerVersionGuard(table,'version',tail);
/** Byte-identical command-receipt by command-id read shared across the replay and
 * receipt-image readers in work-social-purchases.ts. Only the projected column
 * list and table alias vary; the fixed `command_id=?` predicate, bind order and
 * lock semantics stay unchanged. */
export const commandReceiptById=(cols:string,alias='')=>`SELECT ${cols} FROM command_receipts${alias ? ` ${alias}` : ''} WHERE command_id=?`;
/** Byte-identical staff-notifications legacy CTE body shared by the list and page
 * readers. Callers pass only fixed owner-local SQL fragments: the native rowset,
 * clock, projected columns, partition guard and the ordered purchase-scope
 * predicates. The fixed `company_id=? AND user_id=?` binds, `legacy_rows`
 * identity, union and lock semantics stay at the call sites. */
export const staffNotificationCteBody=(nativeSql:string,clock:string,cols:string,partition:string,whereFragments:string[])=>`WITH native_rows AS (${nativeSql}),legacy_rows AS (${clock}SELECT ${cols},created_at::text AS page_key FROM staff_notifications WHERE company_id=? AND user_id=? AND ${partition} AND ${whereFragments.join(' AND ')} `;
/** Byte-identical settlement source row read shared by the three Work
 * package-obligation provenance keys. Only the source table varies; the
 * star-projection head, zero binds and lock semantics stay unchanged. */
export const settlementSourceRows=(table:string)=>`SELECT * FROM ${table}`;
/** Byte-identical AI-draft identity read shared by the social AI prior-draft
 * repository and the social-task AI provider reader. Only the projected column
 * list varies; the fixed `id=? AND actor_id=?` prefix, bind order and lock
 * semantics stay unchanged. */
export const socialAiDraftById=(cols:string)=>`SELECT ${cols} FROM social_ai_drafts WHERE id=? AND actor_id=?`;
/** Byte-identical readable-post head shared by the Social source published-post
 * reader and the Social home revision-author reader. Only the projected column
 * list and trailing access predicate vary; the `p.id=? AND p.company_id=?`
 * prefix, bind order and lock semantics stay at the call sites. */
export const readablePostHead=(cols:string,tail:string)=>`SELECT p.${cols} FROM social_posts p WHERE p.id=? AND p.company_id=? AND ${tail}`;
/** Byte-identical queued source-absence guard shared by the two queued Task
 * read-source readers. Only the joined source rows and the trailing readable
 * predicate vary; the fixed `social_post_sources requested` head, `post_id`
 * reference, bind order and lock semantics stay at the call sites. */
export const queuedSourceAbsentGuard=(joinSql:string,tail:string)=>`NOT EXISTS(SELECT 1 FROM social_post_sources requested JOIN ${joinSql} WHERE requested.post_id=p.id AND ${tail})`;
/** Byte-identical Social purchase actor CTE shared by the selected/current
 * scope readers and the queued reply authority reader. Callers pass only their
 * original body fragment; clock and authority predicates, binds and lock
 * semantics stay at the call sites. */
export const socialPurchaseActorCte=(body:string)=>`WITH social_purchase_actor AS MATERIALIZED(SELECT ${body}) `;
/** Byte-identical requested-source CTE head shared by the Social draft-policy
 * capabilities reader and the work Social source-facts reader. Only the
 * projected body varies; the `requestedSourcesJsonb` CTE, JSON bind order and
 * lock semantics stay unchanged. */
export const requestedSourcesSelect=(body:string)=>`WITH requested AS${requestedSourcesJsonb} SELECT ${body}`;
/** Byte-identical settlement-source existence head shared by the Work
 * package-obligation notice-source and action-card readers and the queued Task
 * source reader. Only the source rowset, alias, alias gap and trailing predicate
 * vary; the fixed source-existence head, zero binds, quantifiers at the call
 * sites and lock semantics stay unchanged. */
export const operationsSourceExists=(rows:string,alias:string,gap:string,tail:string)=>`SELECT 1 FROM (${rows})${gap}${alias}${tail}`;
/** Byte-identical Social post-id head shared by the profile-posts, PMS home
 * read-facts and Social home feed-id readers. Only the trailing predicate varies;
 * the CTE prefix, bind order, ORDER/LIMIT and locks stay at the call sites. */
export const socialPostIdHead=(tail:string)=>`SELECT p.id FROM social_posts p WHERE ${tail}`;
