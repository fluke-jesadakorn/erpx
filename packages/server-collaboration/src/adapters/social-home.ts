import{nativeSocialUploadReadRepository,socialUploadOwner}from '@erpx/server-collaboration/adapters/social-home-read';
export{socialUploadOwner};
import { cachedHomeRead } from "@erpx/server-work_projection/public/home-cache";
import 'server-only';
import { socialOrganizationAudienceName } from '@erpx/server-organization/public/collaboration-home-projections';
import { socialProjectAudienceName } from '@erpx/server-projects/public/collaboration-home-projections';
import { z } from 'zod';
import type { Actor } from '@erpx/server-collaboration/domain/social-home';
import type { StaffActor } from '@erpx/server-operations/public/staff-policy';
import type { PostRow } from '@erpx/server-collaboration/domain/social-source';
import { postDraftSchema, type PostDraft, type PostSource, type CommentRow, type PostPurpose, type PostContent } from '@erpx/server-collaboration/domain/social-home';
import type { Mention } from '@erpx/server-collaboration/domain/social-text';
import { readListPage } from '@erpx/server-work_projection/public/list-page';
import type { SqlDatabase, SqlStatement } from '@erpx/server-runtime/public/runtime/types';
import {assertion} from "@erpx/server-workflow/public/command-assertions";
import {executeCommand,replayCommand} from '@erpx/server-workflow/public/commands';
import { pmsSourcesSql } from '@erpx/server-pms/public/pms/source-policy';
import {sourceAccessSql} from '@erpx/server-collaboration/public/social-source-policy';
import {readablePost,sourceGuard} from '@erpx/server-collaboration/public/social-source';
import {getTask} from '@erpx/server-operations/public/staff-task-read';
import { getProject } from '@erpx/server-projects/public/staff-projects';
import { readablePmsSource, pmsSourceLink } from '@erpx/server-pms/public/pms/source';
import { notification, authorityGuard, requireCompanyAuthority } from '@erpx/server-operations/public/staff-policy';
import { validateMentions, textStatements } from '@erpx/server-collaboration/public/social-text';
import { BusinessRuleError } from '@erpx/server-shared/index';
import { ApiError } from '@erpx/server-runtime/public/http';
import { createSocialHomeApplication, type SocialHomeRepository } from '@erpx/server-collaboration/application/social-home';
import { nativeOwnerWriteDescriptor } from '@erpx/server-collaboration/adapters/native-owner-write';
import { collaborationHomeSources as pms } from '@erpx/server-pms/public/collaboration-home-sources';
import { collaborationHomeSources as organization } from '@erpx/server-organization/public/collaboration-home-sources';
import { collaborationHomeSources as projects } from '@erpx/server-projects/public/collaboration-home-sources';
import {socialProjectAudienceOptions} from '@erpx/server-projects/public/collaboration-home-sources';
import { collaborationHomeSources as identity_access } from '@erpx/server-identity_access/public/collaboration-home-sources';
import { collaborationHomeSources as files } from '@erpx/server-files/public/collaboration-home-sources';
import { collaborationHomeSources as agent } from '@erpx/server-agent/public/collaboration-home-sources';
import { collaborationHomeSources as operations } from '@erpx/server-operations/public/collaboration-home-sources';
import {handoverAcknowledgements} from '@erpx/server-operations/public/handover-acknowledgement-read';
import {workSocialReadScope,workSocialPresentation} from '@erpx/server-collaboration/public/work-social-read-policy';
import {withWorkSocialCommand} from '@erpx/server-collaboration/adapters/work-social-purchases';
import {socialFollowPostExists,messagingTeamMemberExists,publishedCommentRead,socialPostDraftById,socialPostTopicExists,socialCommentTopicExists,socialCommentAncestorStep,socialPostDraftExists} from './read-tail-sql';
const draftVersionGuardSql = socialPostDraftExists('version=? AND published_post_id IS NULL');
async function inward<T>(action: () => Promise<T>) {
    try {
        return await action();
    }
    catch (error) {
        if (error instanceof ApiError)
            throw new BusinessRuleError(error.status, error.code, error.message);
        throw error;
    }
}
async function exposed<T>(action: () => Promise<T>) {
    try {
        return await action();
    }
    catch (error) {
        if (error instanceof BusinessRuleError)
            throw new ApiError(error.status, error.code, error.message);
        throw error;
    }
}
function application(db: SqlDatabase,purchase:Awaited<ReturnType<typeof workSocialReadScope>>={prefix:'',where:'TRUE',args:[]},audienceProjects?:Awaited<ReturnType<typeof socialProjectAudienceOptions>>) {
    const repository: SocialHomeRepository<SqlStatement> = { async teams(companyId: string, userId: string) {
            return db.prepare(`SELECT t.id,t.name FROM messaging_teams t WHERE t.company_id=? AND NOT t.organization_managed
      AND ${messagingTeamMemberExists} ORDER BY t.name,t.id LIMIT 100`)
                .bind(companyId, userId).all<{
                id: string;
                name: string;
            }>();
        }, guardTeamAudience(key: string, value: "audience-team", id: string, companyId: string, userId: string) {
            return assertion(db, key, value, "EXISTS(SELECT 1 FROM messaging_teams t JOIN messaging_team_members m ON m.team_id=t.id WHERE t.id=? AND t.company_id=? AND NOT t.organization_managed AND m.user_id=?)", [id, companyId, userId]);
        }, draft(...args){return nativeSocialUploadReadRepository(db).draft(...args);}, publishedComment(...args){return nativeSocialUploadReadRepository(db).publishedComment(...args);}, editDraft(...args){return nativeSocialUploadReadRepository(db).draft(...args);}, async commentDraftExists(id: string) { return db.prepare("SELECT 1 FROM social_post_drafts WHERE id=? AND comment_parent_post_id IS NOT NULL").bind(id).first(); }, async richDraftExists(id: string) { return db.prepare("SELECT 1 FROM social_post_drafts WHERE id=? AND rich_content_json IS NOT NULL").bind(id).first(); }, guardDraftVersion(key: string, value: "draft-version", id: string, userId: string, expectedVersion: number) {
            return assertion(db, key, value, draftVersionGuardSql, [id, userId, expectedVersion]);
        }, updateDraft(body: string, branch: string | null, department: string | null, value: "company" | "branch" | "department" | "participants" | "team" | "project" | null, audienceId: string | null, value2: string, value3: string, purpose: "update" | "plan" | "blocker" | "handover" | "announcement", value4: string, value5: string | null, value6: string | null, now: number, id: string) {
            return nativeOwnerWriteDescriptor(db, 'SH_DRAFT_UPDATE', [body, branch, department, value, audienceId, value2, value3, purpose, value4, value5, value6, now, id]);
        }, prepareDraft(recordId: string, userId: string, companyId: string, branch: string | null, body: string, now: number, now2: number, department: string | null, value: "company" | "branch" | "department" | "participants" | "team" | "project" | null, audienceId: string | null, value2: string, value3: string, purpose: "update" | "plan" | "blocker" | "handover" | "announcement", value4: string, value5: string | null, value6: string | null) {
            return nativeOwnerWriteDescriptor(db, 'SH_DRAFT_PREPARE', [recordId, userId, companyId, branch, body, now, now2, department, value, audienceId, value2, value3, purpose, value4, value5, value6]);
        }, async publishDraft(id: string, companyId: string, userId: string) {
            return db
                .prepare(`${socialPostDraftById} AND author_id=?`)
                .bind(id, companyId, userId)
                .first<PostDraft>();
        }, guardPublishDraft(key: string, value: "draft", id: string, userId: string, version: number) {
            return assertion(db, key, value, draftVersionGuardSql, [id, userId, version]);
        }, preparePost(postId: string, userId: string, name: string, companyId: string, branch_id: string | null, body: string, key: string, now: number, now2: number, department_id: string | null, audience_kind: "company" | "branch" | "department" | "participants" | "team" | "project", audience_id: string | null, purpose: "update" | "plan" | "blocker" | "handover" | "announcement", value: string, content_json: string | null, rich_content_json: string | null) {
            return nativeOwnerWriteDescriptor(db, 'SH_POST_PREPARE', [postId, userId, name, companyId, branch_id, body, key, now, now2, department_id, audience_kind, audience_id, purpose, value, content_json, rich_content_json]);
        }, prepareParticipant(postId: string, userId: string) { return nativeOwnerWriteDescriptor(db, 'SH_PARTICIPANT_PREPARE', [postId, userId]); }, prepareSource(postId: string, ordinal: number, kind: "staff_task" | "staff_project" | "pms_record", id: string) {
            return nativeOwnerWriteDescriptor(db, 'SH_SOURCE_PREPARE', [postId, ordinal, kind, id]);
        }, bindPublishedDraft(postId: string, now: number, id: string) {
            return nativeOwnerWriteDescriptor(db, 'SH_DRAFT_BIND_PUBLISHED', [postId, now, id]);
        }, prepareAuthorFollow(userId: string, postId: string, now: number) {
            return nativeOwnerWriteDescriptor(db, 'SH_FOLLOW_PREPARE', [userId, postId, now]);
        }, async ancestors(postId: string, ids: string[]) {
            return db.prepare(`WITH RECURSIVE ancestors AS (
      SELECT parent.* FROM social_comments child JOIN social_comments parent
        ON parent.id=child.parent_comment_id AND parent.post_id=child.post_id
      WHERE child.post_id=? AND child.id IN (${ids.map(() => "?").join(",")}) AND child.state='published'
      UNION
      ${socialCommentAncestorStep}
    )
    SELECT a.id,a.post_id,a.body,a.author_id,a.author_name,a.created_at,a.mentions_json,a.rich_content_json,
      a.author_kind,a.parent_comment_id,parent.author_name AS parent_author_name
    FROM ancestors a LEFT JOIN social_comments parent ON parent.id=a.parent_comment_id AND parent.post_id=a.post_id
    WHERE a.state='published'`)
                .bind(postId, ...ids).all<CommentRow>();
        }, async posts(ids: string[], companyId: string, userId: string, now: number, now2: number, assignmentId: string) {
            return db.prepare(`${purchase.prefix}SELECT p.*,
    CASE WHEN p.audience_kind='team' THEN (SELECT t.name FROM messaging_teams t WHERE t.id=p.audience_id AND t.company_id=p.company_id)
      WHEN p.audience_kind='project' THEN ${socialProjectAudienceName()}
      ELSE ${socialOrganizationAudienceName()} END AS audience_name,
    (SELECT count(*) FROM social_post_participants pp WHERE pp.post_id=p.id AND pp.user_id<>p.author_id) AS audience_count
    FROM social_posts p WHERE p.id IN (${ids.map(() => '?').join(',')}) AND p.company_id=? AND p.state='published' AND ${sourceAccessSql("p", "?::text")} AND ${purchase.where}`)
                .bind(...purchase.args,...ids, companyId, userId, now, now2, assignmentId).all<PostRow & {
                audience_name: string | null;
                audience_count: number;
            }>();
        }, async comments(allowed: string[], commentLimit: number) {
            return db.prepare(`WITH RECURSIVE ranked AS (
        SELECT c.*,row_number() OVER(PARTITION BY c.post_id ORDER BY c.created_at DESC,c.id DESC) ordinal
        FROM social_comments c WHERE c.post_id IN (${allowed.map(() => '?').join(',')}) AND c.state='published'
      ), selected AS (
        SELECT * FROM ranked WHERE ordinal<=?
      ), ancestors AS (
        SELECT parent.* FROM selected child JOIN social_comments parent
          ON parent.id=child.parent_comment_id AND parent.post_id=child.post_id
        WHERE parent.state='published'
        UNION
        ${socialCommentAncestorStep}
      ), thread_comments AS (
        SELECT id,post_id,body,author_id,author_name,created_at,mentions_json,rich_content_json,author_kind,parent_comment_id FROM selected
        UNION
        SELECT id,post_id,body,author_id,author_name,created_at,mentions_json,rich_content_json,author_kind,parent_comment_id FROM ancestors
      )
      SELECT c.id,c.post_id,c.body,c.author_id,c.author_name,c.created_at,c.mentions_json,c.rich_content_json,c.author_kind,c.parent_comment_id,
        parent.author_name AS parent_author_name FROM thread_comments c
      LEFT JOIN social_comments parent ON parent.id=c.parent_comment_id AND parent.post_id=c.post_id
      ORDER BY c.created_at,c.id`)
                .bind(...allowed, commentLimit).all<CommentRow>();
        }, async counts(userId: string, userId2: string, allowed: string[]) {
            return db.prepare(`SELECT p.id,(SELECT count(*) FROM social_comments WHERE post_id=p.id AND state='published') AS commentCount,
      (SELECT count(*) FROM social_reactions WHERE post_id=p.id) AS appreciationCount,
      EXISTS(SELECT 1 FROM social_reactions WHERE post_id=p.id AND principal_id=?) AS appreciated,
      EXISTS(SELECT 1 FROM social_follows WHERE target_type='post' AND target_id=p.id AND principal_id=?) AS followed
      FROM social_posts p WHERE p.id IN (${allowed.map(() => '?').join(',')})`).bind(userId, userId2, ...allowed)
                .all<{
                id: string;
                commentCount: number;
                appreciationCount: number;
                appreciated: number;
                followed: number;
            }>();
        }, async origins(allowed: string[]) {
            return db.prepare(`SELECT post_id,task_id FROM social_task_origins WHERE post_id IN (${allowed.map(() => '?').join(',')})`).bind(...allowed).all<{
                post_id: string;
                task_id: string;
            }>();
        }, async replies(allowed: string[]) {
            return db.prepare(`SELECT post_id,id,owner_type,owner_id,state,error,error_code,actor_id,requester_name,attempt,('reply-' || id) AS reply_id FROM social_agent_replies WHERE post_id IN (${allowed.map(() => '?').join(',')}) ORDER BY created_at`).bind(...allowed)
                .all<{
                post_id: string;
                id: string;
                owner_type: "post" | "comment";
                owner_id: string;
                state: string;
                error: string | null;
                error_code: string | null;
                actor_id: string;
                requester_name: string | null;
                attempt: number;
                reply_id: string;
            }>();
        }, async sources(allowed: string[]) {
            return db.prepare(`SELECT post_id,source_kind,source_id,ordinal FROM social_post_sources WHERE post_id IN (${allowed.map(() => '?').join(',')}) ORDER BY post_id,ordinal`)
                .bind(...allowed).all<{
                post_id: string;
                source_kind: PostSource["kind"];
                source_id: string;
                ordinal: number;
            }>();
        }, async feedIds(companyId: string, userId: string, now: number, now2: number, assignmentId: string, branchId: string | null, branchId2: string | null, value: string | null, value2: string | null, value3: number, value4: number, value5: string, value6: 0 | 1, userId2: string, value7: string | null, value8: string | null, value9: string | null, value10: string | null, value11: string | null, value12: "update" | "plan" | "blocker" | "handover" | "announcement" | null, value13: "update" | "plan" | "blocker" | "handover" | "announcement" | null, value14: string | null, value15: string | null, value16: "conversations" | "milestones" | null, value17: "conversations" | "milestones" | null, value18: "conversations" | "milestones" | null, limit: number) {
            return db
                .prepare(`${purchase.prefix}SELECT p.id FROM social_posts p WHERE p.company_id=? AND p.state='published' AND p.linked_module IS NULL AND ${sourceAccessSql("p", "?::text")} AND ${purchase.where} AND (?::text IS NULL OR p.branch_id=? OR p.branch_id IS NULL) AND (?::text IS NULL OR p.branch_id=?) AND (p.created_at<? OR (p.created_at=? AND p.id<?)) AND (?=0 OR ${socialFollowPostExists}) AND (?::text IS NULL OR ${socialPostTopicExists('?')} OR ${socialCommentTopicExists('?')})
        AND (?::text IS NULL OR (p.audience_kind='team' AND p.audience_id=?))
        AND (?::text IS NULL OR p.purpose=?) AND (?::text IS NULL OR NOT EXISTS(SELECT 1 FROM social_post_sources s WHERE s.post_id=p.id AND s.source_kind='pms_record')
          OR EXISTS(SELECT 1 FROM social_post_sources s JOIN (${pmsSourcesSql}) pr ON pr.id=s.source_id
            WHERE s.post_id=p.id AND s.source_kind='pms_record' AND pr.company_id=p.company_id AND pr.property_id=?))
        AND (?::text IS NULL OR (?='milestones' AND p.kind='system') OR (?='conversations' AND p.kind<>'system'))
        ORDER BY p.created_at DESC,p.id DESC LIMIT ?`)
                .bind(...purchase.args,companyId, userId, now, now2, assignmentId, branchId, branchId2, value, value2, value3, value4, value5, value6, userId2, value7, value8, value9, value10, value11, value12, value13, value14, value15, value16, value17, value18, limit)
                .all<{
                id: string;
            }>();
        }, async revisionAuthor(id: string, companyId: string, userId: string, now: number, now2: number) {
            return db.prepare(`${purchase.prefix}SELECT p.author_id FROM social_posts p WHERE p.id=? AND p.company_id=? AND ${sourceAccessSql()} AND ${purchase.where}`)
                .bind(...purchase.args,id, companyId, userId, now, now2).first<{
                author_id: string;
            }>();
        }, async revisions(id: string) {
            return db.prepare(`SELECT prior_version,action,body,created_at FROM social_post_revisions
    WHERE post_id=? ORDER BY prior_version DESC`).bind(id)
                .all<{
                prior_version: number;
                action: "edit" | "retract";
                body: string;
                created_at: number;
            }>();
        }, async hiddenReplayAccess(id: string, companyId: string, userId: string, now: number, now2: number, value: string[], hasAssignment: boolean) { return db.prepare(`${purchase.prefix}SELECT 1 FROM social_posts p WHERE p.id=? AND p.company_id=? AND ${sourceAccessSql('p', hasAssignment ? '?::text' : undefined)} AND ${purchase.where}`).bind(...purchase.args,id, companyId, userId, now, now2, ...value).first(); }, async comment(commentId: string, id: string) {
            return db.prepare("SELECT author_id,author_kind,state,version,rich_content_json,mentions_json FROM social_comments WHERE id=? AND post_id=?")
                .bind(commentId, id).first<{
                author_id: string;
                author_kind: string;
                state: string;
                version: number;
                rich_content_json: string | null;
                mentions_json: string;
            }>();
        }, async ownAgentReply(id: string, userId: string, commentId: string) {
            return db.prepare("SELECT 1 FROM social_agent_replies WHERE post_id=? AND actor_id=? AND ('reply-' || id)=? AND state='completed'").bind(id, userId, commentId).first();
        }, guardCommentAuthor(key: string, value: "comment-author", commentId: string, id: string, userId: string, version: number) {
            return assertion(db, key, value, "EXISTS(SELECT 1 FROM social_comments WHERE id=? AND post_id=? AND author_id=? AND author_kind='person' AND state='published' AND version=?)", [commentId, id, userId, version]);
        }, clearCommentMentions(commentId: string) { return nativeOwnerWriteDescriptor(db, 'SH_COMMENT_MENTIONS_CLEAR', [commentId]); }, clearCommentTopics(commentId: string) { return nativeOwnerWriteDescriptor(db, 'SH_COMMENT_TOPICS_CLEAR', [commentId]); }, updateComment(body: string, value: string, value2: string, now: number, key: string, commentId: string, id: string, userId: string, version: number) {
            return nativeOwnerWriteDescriptor(db, 'SH_COMMENT_UPDATE', [body, value, value2, now, key, commentId, id, userId, version]);
        }, prepareAgentMention(commentId: string, kind: "record" | "person" | "agent", targetId: string, label: string, userId: string, now: number) {
            return nativeOwnerWriteDescriptor(db, 'SH_COMMENT_MENTION_PREPARE', [commentId, kind, targetId, label, userId, now]);
        }, guardCommentRemoval(key: string, value: "comment-author", commentId: string, id: string, version: number, userId: string, userId2: string) {
            return assertion(db, key, value, "EXISTS(SELECT 1 FROM social_comments c WHERE c.id=? AND c.post_id=? AND c.state='published' AND c.version=? AND (c.author_id=? OR EXISTS(SELECT 1 FROM social_agent_replies r WHERE r.post_id=c.post_id AND r.actor_id=? AND ('reply-' || r.id)=c.id AND r.state='completed')))", [commentId, id, version, userId, userId2]);
        }, removeComment(now: number, key: string, commentId: string, id: string, version: number) {
            return nativeOwnerWriteDescriptor(db, 'SH_COMMENT_REMOVE', [now, key, commentId, id, version]);
        }, prepareRevision(value: string, id: string, version: number, action: "edit" | "retract", body: string, content_json: string | null, rich_content_json: string | null, mentions_json: string, userId: string, now: number) {
            return nativeOwnerWriteDescriptor(db, 'SH_REVISION_PREPARE', [value, id, version, action, body, content_json, rich_content_json, mentions_json, userId, now]);
        }, retract(now: number, now2: number, id: string, userId: string, version: number) {
            return nativeOwnerWriteDescriptor(db, 'SH_POST_RETRACT', [now, now2, id, userId, version]);
        }, guardEditUpload(key: string, value: "edit-upload-draft", editDraftId: string, userId: string, companyId: string, id: string, version: number) {
            return assertion(db, key, value, socialPostDraftExists('company_id=? AND edit_post_id=? AND edit_post_version=? AND edit_saved_version IS NULL AND cancelled_at IS NULL'), [editDraftId, userId, companyId, id, version]);
        }, clearPostMentions(id: string) { return nativeOwnerWriteDescriptor(db, 'SH_POST_MENTIONS_CLEAR', [id]); }, clearPostTopics(id: string) { return nativeOwnerWriteDescriptor(db, 'SH_POST_TOPICS_CLEAR', [id]); }, updatePost(body: string, value: string, value2: string | null, value3: string, now: number, now2: number, id: string, userId: string, version: number) {
            return nativeOwnerWriteDescriptor(db, 'SH_POST_UPDATE', [body, value, value2, value3, now, now2, id, userId, version]);
        }, bindEditUpload(value: number, now: number, editDraftId: string) {
            return nativeOwnerWriteDescriptor(db, 'SH_EDIT_DRAFT_BIND', [value, now, editDraftId]);
        }, prepareMention(id: string, kind: "record" | "person" | "agent", targetId: string, label: string, userId: string, now: number) {
            return nativeOwnerWriteDescriptor(db, 'SH_POST_MENTION_PREPARE', [id, kind, targetId, label, userId, now]);
        }, prepareTopic(tag: string, userId: string, now: number) {
            return nativeOwnerWriteDescriptor(db, 'SH_TOPIC_PREPARE', [tag, userId, now]);
        }, preparePostTopic(id: string, tag: string) {
            return nativeOwnerWriteDescriptor(db, 'SH_POST_TOPIC_PREPARE', [id, tag]);
        }, prepareEditNotification(value: string, user: string, now: number, id: string, user2: string, now2: number, now3: number) {
            return nativeOwnerWriteDescriptor(db, 'SH_EDIT_NOTIFICATION', [value, user, now, id, user2, now2, now3], { predicate: sourceAccessSql() });
        }, guardCommentUpload(key: string, value: "comment-upload", commentDraftId: string, userId: string, id: string) { return assertion(db, key, value, socialPostDraftExists('comment_parent_post_id=? AND published_comment_id IS NULL AND cancelled_at IS NULL'), [commentDraftId, userId, id]); }, async parentComment(parentCommentId: string, id: string) {
            return db.prepare("SELECT id,author_id,parent_comment_id FROM social_comments WHERE id=? AND post_id=? AND state='published'")
                .bind(parentCommentId, id)
                .first<{
                id: string;
                author_id: string;
                parent_comment_id: string | null;
            }>();
        }, guardParentComment(key: string, value: "reply-parent", id: string, id2: string) {
            return assertion(db, key, value, `EXISTS(${publishedCommentRead})`, [id, id2]);
        }, prepareComment(value: string, id: string, userId: string, name: string, body: string, key: string, now: number, now2: number, value2: string, value3: string | null, value4: string | null, value5: string | null) {
            return nativeOwnerWriteDescriptor(db, 'SH_COMMENT_PREPARE', [value, id, userId, name, body, key, now, now2, value2, value3, value4, value5]);
        }, bindCommentDraft(value: string, now: number, commentDraftId: string) {
            return nativeOwnerWriteDescriptor(db, 'SH_COMMENT_DRAFT_BIND', [value, now, commentDraftId]);
        }, prepareReplyNotification(value: string, author_id: string, now: number, id: string, author_id2: string, now2: number, now3: number) {
            return nativeOwnerWriteDescriptor(db, 'SH_REPLY_NOTIFICATION', [value, author_id, now, id, author_id2, now2, now3], { predicate: sourceAccessSql() });
        }, prepareAppreciation(id: string, userId: string, now: number) {
            return nativeOwnerWriteDescriptor(db, 'SH_REACTION_PREPARE', [id, userId, now]);
        }, removeAppreciation(id: string, userId: string) {
            return nativeOwnerWriteDescriptor(db, 'SH_REACTION_REMOVE', [id, userId]);
        }, prepareFollow(userId: string, id: string, now: number) {
            return nativeOwnerWriteDescriptor(db, 'SH_FOLLOW_PREPARE', [userId, id, now]);
        }, removeFollow(userId: string, id: string) {
            return nativeOwnerWriteDescriptor(db, 'SH_FOLLOW_REMOVE', [userId, id]);
        }, prepareReport(value: string, id: string, userId: string, value2: string, now: number) {
            return nativeOwnerWriteDescriptor(db, 'SH_REPORT_PREPARE', [value, id, userId, value2, now]);
        }, moderate(now: number, id: string) {
            return nativeOwnerWriteDescriptor(db, 'SH_POST_MODERATE', [now, id]);
        }, ...pms(db), ...organization(db), ...projects(db), ...identity_access(db), ...files(db), ...agent(db), ...operations(db),...(audienceProjects?{projects:async()=>({results:audienceProjects})}:{}) };
    return createSocialHomeApplication<SqlStatement>({ repository, commands: { replay: (...args) => inward(() => replayCommand(db, ...args)), commit: (...args) => inward(() => executeCommand(db, ...args)) }, clock: { now: () => Date.now() }, identifiers: { next: () => crypto.randomUUID() }, commentsPage: (actor, id, cursor) => readListPage<CommentRow & {
            page_key: string;
        }>(db, {
            sql: "SELECT c.id,c.post_id,c.body,c.author_id,c.author_name,c.created_at,c.mentions_json,c.rich_content_json,c.author_kind,c.parent_comment_id,parent.author_name AS parent_author_name,c.created_at::text AS page_key FROM social_comments c LEFT JOIN social_comments parent ON parent.id=c.parent_comment_id AND parent.post_id=c.post_id WHERE c.post_id=? AND c.state='published'",
            args: [id], scope: [actor.companyId, actor.userId, id], cursor, limit: 20, descending: true,
        }), getTask: (...args) => inward(() => getTask(db, ...args)), getProject: (...args) => inward(() => getProject(db, ...args)), readablePmsSource: (...args) => inward(() => readablePmsSource(db, ...args)), pmsSourceLink, readablePost: (...args) => inward(() => readablePost(db, ...args)), sourceGuard: (...args) => sourceGuard(db, ...args), notification: async (...args) => await notification(db, ...args), authorityGuard: (...args) => authorityGuard(db, ...args), requireCompanyAuthority: (...args) => inward(() => requireCompanyAuthority(db, ...args)), validateMentions: (...args) => inward(() => validateMentions(db, ...args)), textStatements: (...args) => textStatements(db, ...args) });
}
export async function socialAudienceOptions(db: SqlDatabase, actor: Actor) {
  // Reauthorize actual Project rows before a cache hit; the exact current
  // display rows also partition cached options across membership/access changes.
  const audienceProjects=await socialProjectAudienceOptions(db,actor);
  return cachedHomeRead(actor, db, "audience", {projects:audienceProjects},
    () => exposed(() => application(db,undefined,audienceProjects).socialAudienceOptions(actor)),
    { complete: value => Array.isArray(value.audiences) && Array.isArray(value.branches) });
}

export function savePostDraft(db: SqlDatabase, actor: Actor, id: string | null, input: z.infer<typeof postDraftSchema>, key: string) { return withWorkSocialCommand(db,actor,{key,operation:'social.draft.save',type:'draft',id:id??`post-draft-${key}`,sources:input.sources,audienceProjectId:input.audience?.kind==='project'?input.audience.id:null,purpose:'new-work'},tx=>exposed(() => application(tx).savePostDraft(actor, id, input, key))); }
export function publishPost(db: SqlDatabase, actor: Actor, id: string, version: number, key: string) { return withWorkSocialCommand(db,actor,{key,operation:'social.post.publish',type:'draft',id,purpose:'new-work',publishedDraftReplay:true},tx=>exposed(() => application(tx).publishPost(actor, id, version, key))); }
export function postComments(db: SqlDatabase, actor: StaffActor, id: string, cursor?: string) { return exposed(async() => application(db,await workSocialReadScope(db,actor)).postComments(actor, id, cursor)); }
async function presentPosts<T extends {id:string;purpose:string;audience_kind:string|null}>(db:SqlDatabase,actor:StaffActor,posts:T[]):Promise<Array<T&{purchase?:import("@erpx/server-collaboration/domain/social-presentation").SocialPurchase;handoverReceipt?:import("@erpx/server-operations/public/handover").HandoverAcknowledgement|null}>>{const ids=posts.filter(post=>post.purpose==='handover'&&post.audience_kind==='participants').map(post=>post.id),[view,receipts]=await Promise.all([workSocialPresentation(db,actor,posts.map(post=>post.id)),handoverAcknowledgements(db,actor,ids)]),byId=new Map(receipts.map(receipt=>[receipt.postId,receipt]));return posts.map(post=>({...post,...(ids.includes(post.id)?{handoverReceipt:byId.get(post.id)??null}:{}),...(view.posts?{purchase:view.posts[post.id]??{newWork:false,settleExisting:false,history:false,revision:'unavailable',createTask:false}}:{})}));}
export function socialPresentation(db:SqlDatabase,actor:StaffActor){return workSocialPresentation(db,actor);}
export function postViews(db: SqlDatabase, actor: StaffActor, ids: string[], commentLimit: number) { return exposed(async() => presentPosts(db,actor,await application(db,await workSocialReadScope(db,actor)).postViews(actor, ids, commentLimit))); }
export function postView(db: SqlDatabase, actor: StaffActor, id: string) { return exposed(async() => (await presentPosts(db,actor,[await application(db,await workSocialReadScope(db,actor)).postView(actor, id)]))[0]); }
export async function homePosts(db: SqlDatabase, actor: StaffActor, filter: {
    tag?: string;
    following?: boolean;
    branchId?: string;
    teamId?: string;
    before?: number;
    beforeId?: string;
    limit?: number;
    propertyId?: string;
    purpose?: PostPurpose;
    stream?: "conversations" | "milestones";
} = {}) {
  const purchase=await workSocialReadScope(db,actor),load = () => exposed(async() => presentPosts(db,actor,await application(db,purchase).homePosts(actor, filter)));
  // Original cache revisions predate Commercial source history. Enforced reads
  // execute their current statement gate rather than returning a stale private feed.
  if(purchase.prefix)return load();
  // Some semantic source readers pass only a StaffActor. They cannot share a workspace cache.
  if (!("holdingId" in actor) || !("scope" in actor) || !("modules" in actor)) return load();
  const selection = { tag: filter.tag ?? null, following: !!filter.following,
    branchId: filter.branchId ?? null, teamId: filter.teamId ?? null, before: filter.before ?? null,
    beforeId: filter.beforeId ?? "", limit: Math.max(1, Math.min(filter.limit ?? 20, 20)),
    propertyId: filter.propertyId ?? null, purpose: filter.purpose ?? null, stream: filter.stream ?? null };
  return cachedHomeRead(actor as Actor, db, "feed", selection, load,
    { complete: value => Array.isArray(value) && value.every(post => post.linkedSources.every(source => source.available)) });
}
export function postRevisionHistory(db: SqlDatabase, actor: StaffActor, id: string) { return exposed(async() => application(db,await workSocialReadScope(db,actor)).postRevisionHistory(actor, id)); }
export function postAction(db: SqlDatabase, actor: Actor, id: string, input: {
    action: "comment" | "delete_comment" | "edit_comment" | "appreciate" | "follow" | "report" | "hide" | "edit" | "retract";
    body?: string;
    content?: PostContent;
    expectedVersion?: number;
    active?: boolean;
    mentionUserIds?: string[];
    mentions?: Mention[];
    richContent?: unknown;
    parentCommentId?: string;
    commentId?: string;
    commentDraftId?: string;
    editDraftId?: string;
}, key: string) { const purpose=input.action==='edit'||input.action==='edit_comment'||(input.action==='appreciate'||input.action==='follow')&&input.active?'new-work':input.action==='comment'?'settle-existing':'history';return withWorkSocialCommand(db,actor,{key,operation:`social.post.${input.action}`,type:'post',id,purpose},async tx=>exposed(() => workSocialReadScope(tx,actor).then(scope=>application(tx,scope).postAction(actor, id, input, key)))); }
export { postContentSchema, postPurposeSchema, postAudienceSchema, postSourceSchema, postDraftSchema } from '@erpx/server-collaboration/domain/social-home';
export type { PostContent, PostPurpose, PostAudience, PostSource, PostDraft } from '@erpx/server-collaboration/domain/social-home';
export type HomePost = Awaited<ReturnType<typeof postView>>;
export type { PostRow } from '@erpx/server-collaboration/domain/social-source';
