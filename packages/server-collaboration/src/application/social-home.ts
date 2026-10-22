import { z } from 'zod';
import type { WorkContext } from '@erpx/server-identity_access/public/authorization';
import type { StaffActor } from '@erpx/server-operations/public/staff-policy';
import type { PostRow } from '@erpx/server-collaboration/domain/social-source';
import type { Mention } from '@erpx/server-collaboration/domain/social-text';
import { hashtags } from '@erpx/server-collaboration/domain/social-text';
import { plainRichContent, validateRichContent, type RichContent, type RichNode } from "@erpx/feature-shared/lib/home-rich-content";
import { postContentSchema, postDraftSchema, postSourceSchema, contentBody, contentImages, storedContent, selectedAudience, audienceScope, commentView, type Actor, type PostAudience, type PostSource, type PostDraft, type PostContent, type PostPurpose, type CommentRow } from '@erpx/server-collaboration/domain/social-home';
import { BusinessRuleError, type Clock, type IdentifierSource } from '@erpx/server-shared/index';
import type { ListPage } from '@erpx/server-work_projection/public/list-page';
import type { getTask } from '@erpx/server-operations/public/staff-tasks';
import type { getProject } from '@erpx/server-projects/public/staff-projects';
import type { readablePmsSource } from '@erpx/server-pms/public/pms/source';
import type { pmsSourceLink } from '@erpx/server-pms/public/pms/source';
import type { readablePost } from '@erpx/server-collaboration/public/social-source';
import type { sourceGuard } from '@erpx/server-collaboration/public/social-source';
import type { notification } from '@erpx/server-operations/public/staff-policy';
import type { authorityGuard } from '@erpx/server-operations/public/staff-policy';
import type { requireCompanyAuthority } from '@erpx/server-operations/public/staff-policy';
import type { validateMentions } from '@erpx/server-collaboration/public/social-text';
import type { textStatements } from '@erpx/server-collaboration/public/social-text';
type Tail<T extends unknown[]> = T extends [
    unknown,
    ...infer R
] ? R : never;
export interface SocialHomeRepository<Effect> {
    guardPmsSource(key: string, value: string, user: string, id: string, companyId: string, value2: number | null, value3: number | null): Effect;
    branches(companyId: string, branchId: string | null, branchId2: string | null): Promise<{
        results: {
            id: string;
            display_name: string;
        }[];
    }>;
    teams(companyId: string, userId: string): Promise<{
        results: {
            id: string;
            name: string;
        }[];
    }>;
    projects(companyId: string, userId: string, userId2: string): Promise<{
        results: {
            id: string;
            title: string;
        }[];
    }>;
    guardOrganizationAudience(key: string, value: "audience", id: string, companyId: string, kind: "branch" | "department"): Effect;
    guardTeamAudience(key: string, value: "audience-team", id: string, companyId: string, userId: string): Effect;
    guardProjectAudience(key: string, value: "audience-project", id: string, companyId: string, userId: string, userId2: string): Effect;
    guardPersonAudience(key: string, value: string, userId: string, companyId: string, now: number, now2: number, departmentId: string | null, departmentId2: string | null, branchId: string | null, branchId2: string | null, branchId3: string | null): Effect;
    draft(id: string, companyId: string): Promise<PostDraft | null>;
    publishedComment(published_comment_id: string, comment_parent_post_id: string): Promise<Record<string, unknown> | null>;
    prepareCommentDraft(id: string, userId: string, companyId: string, branch_id: string | null, now: number, now2: number, postId: string): Effect;
    guardCommentDraft(key: string, value: "draft", id: string, userId: string): Effect;
    cancelCommentDraft(now: number, now2: number, id: string): Effect;
    revokeCommentFiles(now: number, id: string): Effect;
    expireCommentUploads(now: number, id: string): Effect;
    prepareEditDraft(id: string, userId: string, companyId: string, branch_id: string | null, now: number, now2: number, postId: string, expectedVersion: number): Effect;
    editDraft(id: string, companyId: string): Promise<PostDraft | null>;
    guardEditDraft(key: string, value: "draft", id: string, userId: string): Effect;
    cancelEditDraft(now: number, now2: number, id: string): Effect;
    revokeEditFiles(now: number, id: string): Effect;
    expireEditUploads(now: number, id: string): Effect;
    commentDraftExists(id: string): Promise<Record<string, unknown> | null>;
    richDraftExists(id: string): Promise<Record<string, unknown> | null>;
    guardDraftVersion(key: string, value: "draft-version", id: string, userId: string, expectedVersion: number): Effect;
    updateDraft(body: string, branch: string | null, department: string | null, value: "company" | "branch" | "department" | "participants" | "team" | "project" | null, audienceId: string | null, value2: string, value3: string, purpose: "update" | "plan" | "blocker" | "handover" | "announcement", value4: string, value5: string | null, value6: string | null, now: number, id: string): Effect;
    prepareDraft(recordId: string, userId: string, companyId: string, branch: string | null, body: string, now: number, now2: number, department: string | null, value: "company" | "branch" | "department" | "participants" | "team" | "project" | null, audienceId: string | null, value2: string, value3: string, purpose: "update" | "plan" | "blocker" | "handover" | "announcement", value4: string, value5: string | null, value6: string | null): Effect;
    revokeUnusedDraftFiles(now: number, recordId: string, keptAttachmentIds: string[]): Effect;
    expireUnusedDraftUploads(now: number, recordId: string): Effect;
    publishDraft(id: string, companyId: string, userId: string): Promise<PostDraft | null>;
    publishImages(ids: string[], id: string, companyId: string): Promise<{
        results: {
            id: string;
        }[];
    }>;
    guardPublishDraft(key: string, value: "draft", id: string, userId: string, version: number): Effect;
    guardPublishImage(key: string, value: string, attachmentId: string | null, id: string, companyId: string): Effect;
    guardNoPendingUploads(key: string, value: "uploads", id: string, now: number): Effect;
    preparePost(postId: string, userId: string, name: string, companyId: string, branch_id: string | null, body: string, key: string, now: number, now2: number, department_id: string | null, audience_kind: "company" | "branch" | "department" | "participants" | "team" | "project", audience_id: string | null, purpose: "update" | "plan" | "blocker" | "handover" | "announcement", value: string, content_json: string | null, rich_content_json: string | null): Effect;
    prepareParticipant(postId: string, userId: string): Effect;
    prepareSource(postId: string, ordinal: number, kind: "staff_task" | "staff_project" | "pms_record", id: string): Effect;
    bindPublishedDraft(postId: string, now: number, id: string): Effect;
    prepareAuthorFollow(userId: string, postId: string, now: number): Effect;
    ancestors(postId: string, ids: string[]): Promise<{
        results: CommentRow[];
    }>;
    posts(ids: string[], companyId: string, userId: string, now: number, now2: number, assignmentId: string): Promise<{
        results: (PostRow & {
            audience_name: string | null;
            audience_count: number;
        })[];
    }>;
    comments(allowed: string[], commentLimit: number): Promise<{
        results: CommentRow[];
    }>;
    images(allowed: string[], allowed2: string[]): Promise<{
        results: {
            post_id: string;
            id: string;
            original_name: string;
            media_type: string;
        }[];
    }>;
    counts(userId: string, userId2: string, allowed: string[]): Promise<{
        results: {
            id: string;
            commentCount: number;
            appreciationCount: number;
            appreciated: number;
            followed: number;
        }[];
    }>;
    origins(allowed: string[]): Promise<{
        results: {
            post_id: string;
            task_id: string;
        }[];
    }>;
    replies(allowed: string[]): Promise<{
        results: {
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
        }[];
    }>;
    sources(allowed: string[]): Promise<{
        results: {
            post_id: string;
            source_kind: PostSource["kind"];
            source_id: string;
            ordinal: number;
        }[];
    }>;
    aiName(companyId: string): Promise<{
        home_ai_name: string;
    } | null>;
    trackingState(task_id: string, companyId: string): Promise<string | null>;
    feedIds(companyId: string, userId: string, now: number, now2: number, assignmentId: string, branchId: string | null, branchId2: string | null, value: string | null, value2: string | null, value3: number, value4: number, value5: string, value6: 0 | 1, userId2: string, value7: string | null, value8: string | null, value9: string | null, value10: string | null, value11: string | null, value12: "update" | "plan" | "blocker" | "handover" | "announcement" | null, value13: "update" | "plan" | "blocker" | "handover" | "announcement" | null, value14: string | null, value15: string | null, value16: "conversations" | "milestones" | null, value17: "conversations" | "milestones" | null, value18: "conversations" | "milestones" | null, limit: number): Promise<{
        results: {
            id: string;
        }[];
    }>;
    revisionAuthor(id: string, companyId: string, userId: string, now: number, now2: number): Promise<{
        author_id: string;
    } | null>;
    revisions(id: string): Promise<{
        results: {
            prior_version: number;
            action: "edit" | "retract";
            body: string;
            created_at: number;
        }[];
    }>;
    hiddenReplayAccess(id: string, companyId: string, userId: string, now: number, now2: number, value: string[], hasAssignment: boolean): Promise<Record<string, unknown> | null>;
    comment(commentId: string, id: string): Promise<{
        author_id: string;
        author_kind: string;
        state: string;
        version: number;
        rich_content_json: string | null;
        mentions_json: string;
    } | null>;
    ownAgentReply(id: string, userId: string, commentId: string): Promise<Record<string, unknown> | null>;
    guardCommentAuthor(key: string, value: "comment-author", commentId: string, id: string, userId: string, version: number): Effect;
    clearCommentMentions(commentId: string): Effect;
    clearCommentTopics(commentId: string): Effect;
    updateComment(body: string, value: string, value2: string, now: number, key: string, commentId: string, id: string, userId: string, version: number): Effect;
    prepareAgentMention(commentId: string, kind: "record" | "person" | "agent", targetId: string, label: string, userId: string, now: number): Effect;
    guardCommentRemoval(key: string, value: "comment-author", commentId: string, id: string, version: number, userId: string, userId2: string): Effect;
    removeComment(now: number, key: string, commentId: string, id: string, version: number): Effect;
    prepareRevision(value: string, id: string, version: number, action: "edit" | "retract", body: string, content_json: string | null, rich_content_json: string | null, mentions_json: string, userId: string, now: number): Effect;
    retract(now: number, now2: number, id: string, userId: string, version: number): Effect;
    legacyImages(id: string, companyId: string): Promise<{
        results: {
            id: string;
        }[];
    }>;
    retainedImages(value: (string | null)[], id: string, id2: string, companyId: string): Promise<{
        results: {
            id: string;
        }[];
    }>;
    uploadedImages(value: (string | null)[], editDraftId: string | undefined, userId: string, companyId: string, id: string, version: number): Promise<{
        results: {
            id: string;
        }[];
    }>;
    guardRetainedImage(key: string, value: string, attachmentId: string | null, id: string, id2: string, companyId: string): Effect;
    guardUploadedImage(key: string, value: string, attachmentId: string | null, value2: string, userId: string, companyId: string, id: string, version: number): Effect;
    guardEditUpload(key: string, value: "edit-upload-draft", editDraftId: string, userId: string, companyId: string, id: string, version: number): Effect;
    clearPostMentions(id: string): Effect;
    clearPostTopics(id: string): Effect;
    updatePost(body: string, value: string, value2: string | null, value3: string, now: number, now2: number, id: string, userId: string, version: number): Effect;
    bindEditUpload(value: number, now: number, editDraftId: string): Effect;
    revokeUnusedPostFiles(now: number, id: string, id2: string, value: (string | null)[]): Effect;
    expireUnusedPostUploads(now: number, id: string, id2: string): Effect;
    prepareMention(id: string, kind: "record" | "person" | "agent", targetId: string, label: string, userId: string, now: number): Effect;
    prepareTopic(tag: string, userId: string, now: number): Effect;
    preparePostTopic(id: string, tag: string): Effect;
    prepareEditNotification(value: string, user: string, now: number, id: string, user2: string, now2: number, now3: number): Effect;
    commentImages(value: (string | null)[], commentDraftId: string, companyId: string): Promise<{
        results: {
            id: string;
        }[];
    }>;
    guardCommentUpload(key: string, value: "comment-upload", commentDraftId: string, userId: string, id: string): Effect;
    guardCommentImage(key: string, value: string, attachmentId: string | null, commentDraftId: string, companyId: string): Effect;
    parentComment(parentCommentId: string, id: string): Promise<{
        id: string;
        author_id: string;
        parent_comment_id: string | null;
    } | null>;
    guardParentComment(key: string, value: "reply-parent", id: string, id2: string): Effect;
    prepareComment(value: string, id: string, userId: string, name: string, body: string, key: string, now: number, now2: number, value2: string, value3: string | null, value4: string | null, value5: string | null): Effect;
    bindCommentDraft(value: string, now: number, commentDraftId: string): Effect;
    revokeUnusedCommentFiles(now: number, commentDraftId: string, value: (string | null)[]): Effect;
    prepareReplyNotification(value: string, author_id: string, now: number, id: string, author_id2: string, now2: number, now3: number): Effect;
    prepareAppreciation(id: string, userId: string, now: number): Effect;
    removeAppreciation(id: string, userId: string): Effect;
    prepareFollow(userId: string, id: string, now: number): Effect;
    removeFollow(userId: string, id: string): Effect;
    prepareReport(value: string, id: string, userId: string, value2: string, now: number): Effect;
    moderate(now: number, id: string): Effect;
}
export interface OwnerCommands<Effect> {
    replay<Result = Record<string, unknown>>(actor: StaffActor, key: string, operation: string, id: string, input: unknown): Promise<Result | null>;
    commit<Result>(actor: StaffActor, key: string, operation: string, input: unknown, id: string, version: number, result: Result, effects: Effect[], scope?: Pick<WorkContext, 'companyId' | 'branchId' | 'departmentId'>): Promise<Result>;
}
export interface SocialHomePorts<Effect> {
    repository: SocialHomeRepository<Effect>;
    commands: OwnerCommands<Effect>;
    clock: Clock;
    identifiers: IdentifierSource;
    commentsPage(actor: StaffActor, id: string, cursor?: string): Promise<ListPage<CommentRow & {
        page_key: string;
    }>>;
    getTask(...args: Tail<Parameters<typeof getTask>>): ReturnType<typeof getTask>;
    getProject(...args: Tail<Parameters<typeof getProject>>): ReturnType<typeof getProject>;
    readablePmsSource(...args: Tail<Parameters<typeof readablePmsSource>>): ReturnType<typeof readablePmsSource>;
    pmsSourceLink: typeof pmsSourceLink;
    readablePost(...args: Tail<Parameters<typeof readablePost>>): ReturnType<typeof readablePost>;
    sourceGuard(...args: Tail<Parameters<typeof sourceGuard>>): Effect;
    notification(...args: Tail<Parameters<typeof notification>>): Effect;
    authorityGuard(...args: Tail<Parameters<typeof authorityGuard>>): Effect;
    requireCompanyAuthority(...args: Tail<Parameters<typeof requireCompanyAuthority>>): ReturnType<typeof requireCompanyAuthority>;
    validateMentions(...args: Tail<Parameters<typeof validateMentions>>): ReturnType<typeof validateMentions>;
    textStatements(...args: Tail<Parameters<typeof textStatements>>): Promise<Effect[]>;
}
export function createSocialHomeApplication<Effect>(ports: SocialHomePorts<Effect>) {
    const r = ports.repository;
    function pmsSourceAssertions(key: string, actor: Actor, audience: PostAudience | null, sources: PostSource[]) {
        return sources.filter(source => source.kind === "pms_record").flatMap((source, index) => {
            const users = [...new Set([actor.userId, ...(audience?.kind === "participants" ? audience.userIds : [])])];
            return users.map((user, recipient) => r.guardPmsSource(key, `pms-source-${index}-${recipient}`, user, source.id, actor.companyId, source.version ?? null, source.version ?? null));
        });
    }
    async function socialAudienceOptions(actor: Actor) {
        const [branches, teams, projects] = await Promise.all([
            r.branches(actor.companyId, actor.branchId, actor.branchId),
            r.teams(actor.companyId, actor.userId),
            r.projects(actor.companyId, actor.userId, actor.userId),
        ]);
        const base = actor.assignmentScope === "holding"
            ? [{ value: "company", kind: "company" as const, label: "Everyone in the company" },
                ...branches.results.map(b => ({ value: `branch:${b.id}`, kind: "branch" as const, label: `Branch · ${b.display_name}` }))]
            : actor.assignmentScope === "department" && actor.departmentId
                ? [{ value: `department:${actor.departmentId}`, kind: "department" as const,
                        label: `Department · ${actor.scope.kind === "department" ? actor.scope.departmentName : "Your department"}` }]
                : branches.results.filter(b => b.id === actor.branchId).map(b => ({ value: `branch:${b.id}`, kind: "branch" as const, label: `Branch · ${b.display_name}` }));
        const within = actor.assignmentScope === "holding" ? "" : actor.assignmentScope === "department" ? " · in your department" : " · in your branch";
        return { branches: branches.results, projects: projects.results,
            audiences: [...base,
                { value: "participants", kind: "participants" as const, label: `Selected people${within}` },
                ...teams.results.map(t => ({ value: `team:${t.id}`, kind: "team" as const, label: `Team · ${t.name}${within}` })),
                ...projects.results.map(p => ({ value: `project:${p.id}`, kind: "project" as const, label: `Project · ${p.title}${within}` }))] };
    }
    function audienceAssertion(key: string, actor: Actor, audience: PostAudience | null) {
        if (!audience || audience.kind === "company")
            return [];
        if (audience.kind === "branch" || audience.kind === "department")
            return [r.guardOrganizationAudience(key, "audience", audience.id, actor.companyId, audience.kind)];
        if (audience.kind === "team")
            return [r.guardTeamAudience(key, "audience-team", audience.id, actor.companyId, actor.userId)];
        if (audience.kind === "project")
            return [r.guardProjectAudience(key, "audience-project", audience.id, actor.companyId, actor.userId, actor.userId)];
        return [...new Set([actor.userId, ...audience.userIds])].map((userId, index) => r.guardPersonAudience(key, `audience-person-${index}`, userId, actor.companyId, ports.clock.now(), ports.clock.now(), actor.departmentId, actor.departmentId, actor.branchId, actor.branchId, actor.branchId));
    }
    async function validatePostSources(actor: Actor, sources: PostSource[]) {
        if (new Set(sources.map(s => `${s.kind}:${s.id}`)).size !== sources.length)
            throw new BusinessRuleError(422, "SOURCE_DUPLICATE", "Choose each linked record once.");
        for (const source of sources) {
            if (source.kind === "staff_task")
                await ports.getTask(actor, source.id);
            else if (source.kind === "pms_record") {
                const item = await ports.readablePmsSource(actor, source.id);
                if (source.version && Number(item.version) !== source.version)
                    throw new BusinessRuleError(409, "HANDOVER_SOURCE_CHANGED", "Hotel work changed. Refresh before publishing.");
            }
            else
                await ports.getProject(actor, source.id);
        }
    }
    async function socialUploadOwner(actor: StaffActor, id: string, edit = false) {
        const draft = await r.draft(id, actor.companyId);
        if (!draft)
            throw new BusinessRuleError(404, "DRAFT_NOT_FOUND", "Draft unavailable.");
        if (draft.cancelled_at)
            throw new BusinessRuleError(404, "DRAFT_NOT_FOUND", "Upload draft unavailable.");
        if (draft.edit_post_id) {
            const post = await ports.readablePost(actor, draft.edit_post_id);
            if (edit && (draft.author_id !== actor.userId || draft.edit_saved_version !== null ||
                post.author_id !== actor.userId || post.kind !== "team" || post.linked_module))
                throw new BusinessRuleError(403, "POST_AUTHOR_REQUIRED", "Only the author can attach images to this post edit.");
            if (edit && post.version !== draft.edit_post_version)
                throw new BusinessRuleError(409, "POST_CHANGED", "This post changed. Reload it before attaching images.");
            return draft;
        }
        if (draft.comment_parent_post_id) {
            if (edit) {
                if (draft.author_id !== actor.userId || draft.published_comment_id)
                    throw new BusinessRuleError(403, "DRAFT_LOCKED", "This comment no longer accepts images.");
                await ports.readablePost(actor, draft.comment_parent_post_id);
            }
            else if (draft.published_comment_id) {
                await ports.readablePost(actor, draft.comment_parent_post_id);
                if (!await r.publishedComment(draft.published_comment_id, draft.comment_parent_post_id))
                    throw new BusinessRuleError(404, "COMMENT_UNAVAILABLE", "This image is no longer available.");
            }
            else if (draft.author_id !== actor.userId)
                throw new BusinessRuleError(404, "DRAFT_NOT_FOUND", "Upload draft unavailable.");
            return draft;
        }
        if (edit || !draft.published_post_id) {
            if (draft.author_id !== actor.userId || draft.published_post_id)
                throw new BusinessRuleError(403, "DRAFT_LOCKED", "Only the author can edit an unpublished draft.");
        }
        else
            await ports.readablePost(actor, draft.published_post_id);
        return draft;
    }
    async function createCommentUploadDraft(actor: StaffActor, postId: string, key: string) {
        const post = await ports.readablePost(actor, postId), id = `comment-upload-${key}`, now = ports.clock.now();
        return ports.commands.commit(actor, key, "social.comment.upload.create", { postId }, id, 1, { id }, [
            ports.sourceGuard(key, post, actor.userId, "source"),
            r.prepareCommentDraft(id, actor.userId, actor.companyId, post.branch_id, now, now, postId),
        ]);
    }
    async function cancelCommentUploadDraft(actor: StaffActor, id: string, key: string) {
        const prior = await ports.commands.replay<{id:string}>(actor,key,"social.comment.upload.cancel",id,{id});
        if (prior) return prior;
        const draft = await r.draft(id,actor.companyId);
        if (!draft || draft.author_id !== actor.userId || draft.cancelled_at !== null || draft.published_post_id || draft.published_comment_id || draft.edit_post_id || !draft.comment_parent_post_id)
            throw new BusinessRuleError(404, "DRAFT_NOT_FOUND", "Comment upload draft unavailable.");
        const now = ports.clock.now();
        return ports.commands.commit(actor, key, "social.comment.upload.cancel", { id }, id, draft.version + 1, { id }, [
            r.guardCommentDraft(key, "draft", id, actor.userId),
            r.cancelCommentDraft(now, now, id),
            r.revokeCommentFiles(now, id),
            r.expireCommentUploads(now, id),
        ]);
    }
    async function createPostEditUploadDraft(actor: StaffActor, postId: string, expectedVersion: number, key: string) {
        const post = await ports.readablePost(actor, postId);
        if (post.author_id !== actor.userId || post.kind !== "team" || post.linked_module)
            throw new BusinessRuleError(403, "POST_AUTHOR_REQUIRED", "Only the author can edit this team post.");
        if (post.version !== expectedVersion)
            throw new BusinessRuleError(409, "POST_CHANGED", "This post changed. Reload it before editing.");
        const id = `post-edit-upload-${key}`, now = ports.clock.now();
        return ports.commands.commit(actor, key, "social.post.edit-upload.create", { postId, expectedVersion }, id, 1, { id, version: 1 }, [
            ports.sourceGuard(key, post, actor.userId, "source"),
            r.prepareEditDraft(id, actor.userId, actor.companyId, post.branch_id, now, now, postId, expectedVersion),
        ]);
    }
    async function cancelPostEditUploadDraft(actor: StaffActor, id: string, key: string) {
        const prior = await ports.commands.replay<{id:string}>(actor,key,"social.post.edit-upload.cancel",id,{id});
        if (prior) return prior;
        const draft = await r.editDraft(id, actor.companyId);
        if (!draft || !draft.edit_post_id || draft.edit_saved_version !== null || draft.cancelled_at !== null || draft.published_post_id || draft.published_comment_id || draft.comment_parent_post_id)
            throw new BusinessRuleError(404, "UPLOAD_DRAFT_UNAVAILABLE", "Post edit upload draft unavailable.");
        if (draft.author_id !== actor.userId)
            throw new BusinessRuleError(404, "UPLOAD_DRAFT_UNAVAILABLE", "Post edit upload draft unavailable.");
        const now = ports.clock.now();
        return ports.commands.commit(actor, key, "social.post.edit-upload.cancel", { id }, id, draft.version + 1, { id }, [
            r.guardEditDraft(key, "draft", id, actor.userId),
            r.cancelEditDraft(now, now, id),
            r.revokeEditFiles(now, id),
            r.expireEditUploads(now, id),
        ]);
    }
    async function savePostDraft(actor: Actor, id: string | null, input: z.infer<typeof postDraftSchema>, key: string) {
        const recordId = id ?? `post-draft-${key}`, now = ports.clock.now();
        const existing = id ? await socialUploadOwner(actor, id, true) : null;
        if (existing?.published_post_id || existing?.edit_post_id || id && await r.commentDraftExists(id))
            throw new BusinessRuleError(404, "DRAFT_NOT_FOUND", "This is a comment upload draft.");
        if (id && input.richContent === undefined && await r.richDraftExists(id))
            throw new BusinessRuleError(422, "RICH_CONTENT_REQUIRED", "Reload this draft in the advanced editor before saving changes.");
        const content = input.content;
        let rich: RichContent | null = null;
        if (input.richContent !== undefined) {
            try {
                const parsed = validateRichContent(input.richContent, 30000);
                rich = parsed.content;
                if (parsed.body !== input.body || JSON.stringify(parsed.mentions) !== JSON.stringify(input.mentions ?? []))
                    throw Error("Rich text and mentions differ from the message.");
                if (content && rich.blocks.length !== content.length)
                    throw Error("Rich text blocks differ from post content.");
                if (content && rich.blocks.some((block, i) => block.type !== content[i].type || (block.type === "image" && content[i].type === "image" && (block.slotId !== content[i].slotId || block.attachmentId !== content[i].attachmentId || block.width !== content[i].width))))
                    throw Error("Rich images differ from post content.");
            }
            catch (error) {
                throw new BusinessRuleError(422, "RICH_CONTENT_INVALID", error instanceof Error ? error.message : "Invalid rich content.");
            }
        }
        if (content) {
            if (contentBody(content) !== input.body)
                throw new BusinessRuleError(422, "POST_CONTENT_CHANGED", "The post text changed. Review the draft and try again.");
            const images = contentImages(content);
            if (images.length > 10)
                throw new BusinessRuleError(422, "ATTACHMENT_LIMIT", "Posts accept up to ten images.");
            if (new Set(images.map((image) => image.slotId)).size !== images.length ||
                new Set(images.filter((image) => image.attachmentId).map((image) => image.attachmentId)).size !== images.filter((image) => image.attachmentId).length)
                throw new BusinessRuleError(422, "POST_IMAGE_DUPLICATE", "Each post image needs a unique slot and file.");
        }
        const mentions = await ports.validateMentions(actor, input.body, input.mentions);
        const keptAttachmentIds = content ? contentImages(content).flatMap((image) => image.attachmentId ? [image.attachmentId] : []) : [];
        const audience = selectedAudience(actor, input);
        const { branch, department, audienceId, participants } = audienceScope(actor, audience);
        const sources = input.sources ?? [];
        await validatePostSources(actor, sources);
        const purpose = input.purpose ?? "update";
        return ports.commands.commit(actor, key, "social.draft.save", input, recordId, id ? input.expectedVersion + 1 : 1, { id: recordId, version: id ? input.expectedVersion + 1 : 1 }, [
            ...audienceAssertion(key, actor, audience),
            ...pmsSourceAssertions(key, actor, audience, sources),
            ...(id
                ? [
                    r.guardDraftVersion(key, "draft-version", id, actor.userId, input.expectedVersion),
                ]
                : []),
            id
                ? r.updateDraft(input.body, branch, department, audience?.kind ?? null, audienceId, JSON.stringify(participants), JSON.stringify(sources), purpose, JSON.stringify(mentions), content ? JSON.stringify(content) : null, rich ? JSON.stringify(rich) : null, now, id)
                : r.prepareDraft(recordId, actor.userId, actor.companyId, branch, input.body, now, now, department, audience?.kind ?? null, audienceId, JSON.stringify(participants), JSON.stringify(sources), purpose, JSON.stringify(mentions), content ? JSON.stringify(content) : null, rich ? JSON.stringify(rich) : null),
            ...(content ? [
                r.revokeUnusedDraftFiles(now, recordId, keptAttachmentIds),
                r.expireUnusedDraftUploads(now, recordId),
            ] : []),
        ]);
    }
    async function publishPost(actor: Actor, id: string, version: number, key: string) {
        const draft = await r.publishDraft(id, actor.companyId, actor.userId);
        if (!draft)
            throw new BusinessRuleError(404, "DRAFT_NOT_FOUND", "Draft unavailable.");
        if (draft.comment_parent_post_id)
            throw new BusinessRuleError(404, "DRAFT_NOT_FOUND", "This is a comment upload draft.");
        if (draft.edit_post_id)
            throw new BusinessRuleError(404, "DRAFT_NOT_FOUND", "This is a post edit upload draft.");
        if (draft.published_post_id) {
            await ports.readablePost(actor, draft.published_post_id);
            return { id: draft.published_post_id, version: 1 };
        }
        if (!draft.audience_kind)
            throw new BusinessRuleError(422, "AUDIENCE_REQUIRED", "Choose who can see this post before publishing.");
        const audience: PostAudience = draft.audience_kind === "company"
            ? { kind: "company" }
            : draft.audience_kind === "participants"
                ? { kind: "participants", userIds: JSON.parse(draft.participants_json) as string[] }
                : { kind: draft.audience_kind, id: draft.audience_kind === "branch" ? draft.branch_id!
                        : draft.audience_kind === "department" ? draft.department_id! : draft.audience_id! };
        audienceScope(actor, audience);
        const sources = z.array(postSourceSchema).max(5).parse(JSON.parse(draft.sources_json));
        await validatePostSources(actor, sources);
        const content = storedContent(draft.content_json);
        const images = content ? contentImages(content) : [];
        if (!draft.body.trim() && !images.length)
            throw new BusinessRuleError(422, "POST_BODY_REQUIRED", "Add a message before publishing.");
        if (content && contentBody(content) !== draft.body)
            throw new BusinessRuleError(422, "POST_CONTENT_CHANGED", "The post text changed. Review the draft and try again.");
        if (images.length > 10)
            throw new BusinessRuleError(422, "ATTACHMENT_LIMIT", "Posts accept up to ten images.");
        if (images.some((image) => !image.attachmentId))
            throw new BusinessRuleError(422, "POST_IMAGE_PENDING", "Finish uploading every image before publishing.");
        if (images.length) {
            const ids = images.map((image) => image.attachmentId!);
            const found = (await r.publishImages(ids, id, actor.companyId)).results;
            if (found.length !== ids.length)
                throw new BusinessRuleError(422, "POST_IMAGE_UNAVAILABLE", "An image is unavailable. Reattach it before publishing.");
        }
        const mentions = await ports.validateMentions(actor, draft.body, JSON.parse(draft.mentions_json));
        const postId = `post-${id}`, now = ports.clock.now();
        return ports.commands.commit(actor, key, "social.post.publish", { id, version }, postId, 1, { id: postId, version: 1 }, [
            r.guardPublishDraft(key, "draft", id, actor.userId, version),
            ...audienceAssertion(key, actor, audience),
            ...pmsSourceAssertions(key, actor, audience, sources),
            ...(content ? images.map((image, index) => r.guardPublishImage(key, `image-${index}`, image.attachmentId, id, actor.companyId)) : [r.guardNoPendingUploads(key, "uploads", id, now)]),
            r.preparePost(postId, actor.userId, actor.name, actor.companyId, draft.branch_id, draft.body, key, now, now, draft.department_id, draft.audience_kind, draft.audience_id, draft.purpose, JSON.stringify(mentions), draft.content_json, draft.rich_content_json),
            ...(audience.kind === "participants" ? [...new Set([actor.userId, ...audience.userIds])].map(userId => r.prepareParticipant(postId, userId)) : []),
            ...sources.map((source, ordinal) => r.prepareSource(postId, ordinal, source.kind, source.id)),
            ...await ports.textStatements(actor, postId, "post", postId, draft.body, mentions),
            r.bindPublishedDraft(postId, now, id),
            r.prepareAuthorFollow(actor.userId, postId, now),
        ], { companyId: actor.companyId, branchId: draft.branch_id, departmentId: draft.department_id });
    }
    async function postComments(actor: StaffActor, id: string, cursor?: string) {
        await ports.readablePost(actor, id);
        const page = await ports.commentsPage(actor, id, cursor);
        const ancestors = await commentAncestors(id, page.items);
        return { ...page, items: [...page.items, ...ancestors].map(commentView) };
    }
    async function commentAncestors(postId: string, seeds: CommentRow[]) {
        const ids = [...new Set(seeds.filter(comment => comment.parent_comment_id).map(comment => comment.id))];
        if (!ids.length)
            return [] as CommentRow[];
        return (await r.ancestors(postId, ids)).results;
    }
    async function postViews(actor: StaffActor, ids: string[], commentLimit: number) {
        if (!ids.length)
            return [];
        const now = ports.clock.now();
        const posts = (await r.posts(ids, actor.companyId, actor.userId, now, now, actor.assignmentId)).results;
        if (!posts.length)
            return [];
        const allowed = posts.map(p => p.id);
        const [comments, files, counts, origins, replies, sources, aiSettings] = await Promise.all([
            r.comments(allowed, commentLimit),
            r.images(allowed, allowed),
            r.counts(actor.userId, actor.userId, allowed),
            r.origins(allowed),
            r.replies(allowed),
            r.sources(allowed),
            r.aiName(actor.companyId),
        ]);
        const byId = new Map(posts.map(p => [p.id, p]));
        return Promise.all(ids.filter(id => byId.has(id)).map(async (id) => {
            const post = byId.get(id)!, origin = origins.results.find(o => o.post_id === id);
            let task: null | {
                id: string;
                state: string;
            } = null, trackingState: string | null = null;
            if (origin) {
                try {
                    const t = await ports.getTask(actor, origin.task_id);
                    task = { id: t.id, state: t.state };
                }
                catch (e) {
                    if (!(e instanceof BusinessRuleError) || ![403, 404].includes(e.status))
                        throw e;
                }
                if (post.author_id === actor.userId)
                    trackingState = await r.trackingState(origin.task_id, actor.companyId);
            }
            const count = counts.results.find(c => c.id === id)!;
            const content = storedContent(post.content_json);
            const referencedImages = content ? new Set(contentImages(content).map(image => image.attachmentId)) : null;
            const within = post.department_id ? " · in department" : post.branch_id ? " · in branch" : "";
            const audienceLabel = post.audience_kind === "participants" ? `Selected people (${post.audience_count})${within}`
                : post.audience_kind === "team" ? `Team · ${post.audience_name ?? "Team"}${within}`
                    : post.audience_kind === "project" ? `Project · ${post.audience_name ?? "Project"}${within}`
                        : post.department_id ? `Department · ${post.audience_name ?? "Your department"}`
                            : post.branch_id ? `Branch · ${post.audience_name ?? "Branch"}` : "Everyone in the company";
            const linkedSources = await Promise.all(sources.results.filter(s => s.post_id === id).map(async (s) => {
                try {
                    if (s.source_kind === "pms_record") {
                        const source = await ports.readablePmsSource(actor, s.source_id);
                        return { kind: s.source_kind, available: true as const, id: source.id, ...ports.pmsSourceLink(source) };
                    }
                    if (s.source_kind === "staff_task") {
                        const task = await ports.getTask(actor, s.source_id);
                        return { kind: s.source_kind, available: true as const, id: task.id, title: task.title,
                            sourceKind: "staff_task", version: task.version, label: "Task", href: `/staff-tasks/${encodeURIComponent(task.id)}` };
                    }
                    const project = await ports.getProject(actor, s.source_id);
                    return { kind: s.source_kind, available: true as const, id: project.id, title: project.title,
                        sourceKind: "staff_project", version: project.version, label: "Project", href: `/projects/${encodeURIComponent(project.id)}` };
                }
                catch (error) {
                    if (!(error instanceof BusinessRuleError) || ![403, 404].includes(error.status))
                        throw error;
                    return { kind: s.source_kind, available: false as const };
                }
            }));
            return { ...post, audienceLabel, aiName: aiSettings?.home_ai_name ?? "AI", content, richContent: post.rich_content_json ? JSON.parse(post.rich_content_json) as RichContent : null, mentions: JSON.parse(post.mentions_json ?? '[]') as Mention[],
                linkedSources,
                comments: comments.results.filter(c => c.post_id === id).map(commentView),
                files: files.results.filter(f => f.post_id === id && (!referencedImages || referencedImages.has(f.id))), agentReplies: replies.results.filter(r => r.post_id === id),
                appreciationCount: count.appreciationCount, appreciated: count.appreciated, followed: count.followed, commentCount: count.commentCount,
                task, trackingState, hasTask: !!origin };
        }));
    }
    async function postView(actor: StaffActor, id: string) {
        const post = (await postViews(actor, [id], 20))[0];
        if (!post)
            throw new BusinessRuleError(404, 'POST_NOT_FOUND', 'This post is unavailable.');
        return post;
    }
    async function homePosts(actor: StaffActor, filter: {
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
        const now = ports.clock.now();
        const limit = Math.max(1, Math.min(filter.limit ?? 20, 20));
        const result = await r.feedIds(actor.companyId, actor.userId, now, now, actor.assignmentId, actor.branchId, actor.branchId, filter.branchId ?? null, filter.branchId ?? null, filter.before ?? now + 1, filter.before ?? now + 1, filter.beforeId ?? "", filter.following ? 1 : 0, actor.userId, filter.tag ?? null, filter.tag ?? null, filter.tag ?? null, filter.teamId ?? null, filter.teamId ?? null, filter.purpose ?? null, filter.purpose ?? null, filter.propertyId ?? null, filter.propertyId ?? null, filter.stream ?? null, filter.stream ?? null, filter.stream ?? null, limit);
        return postViews(actor, result.results.map(row => row.id), 2);
    }
    async function postRevisionHistory(actor: StaffActor, id: string) {
        const now = ports.clock.now();
        const post = await r.revisionAuthor(id, actor.companyId, actor.userId, now, now);
        if (!post || post.author_id !== actor.userId)
            throw new BusinessRuleError(404, "POST_NOT_FOUND", "This post is unavailable.");
        const rows = await r.revisions(id);
        return rows.results;
    }
    async function postAction(actor: Actor, id: string, input: {
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
    }, key: string): Promise<{
        id: string;
        version?: number;
    }> {
        const operation = `social.post.${input.action}`;
        const replayed = await ports.commands.replay<{
            id: string;
            version?: number;
        }>(actor, key, operation, id, input);
        if (replayed) {
            if (input.action === 'retract')
                await postRevisionHistory(actor, id);
            else if (input.action === 'hide') {
                await ports.requireCompanyAuthority(actor, ['holding_owner']);
                const now = ports.clock.now();
                if (!await r.hiddenReplayAccess(id, actor.companyId, actor.userId, now, now, (actor.assignmentId ? [actor.assignmentId] : []), Boolean(actor.assignmentId)))
                    throw new BusinessRuleError(404, 'POST_NOT_FOUND', 'This post is unavailable.');
            }
            else
                await ports.readablePost(actor, id);
            return replayed;
        }
        const post = await ports.readablePost(actor, id), now = ports.clock.now(), steps = [ports.sourceGuard(key, post, actor.userId, "source")];
        if (input.action === "delete_comment" || input.action === "edit_comment") {
            if (!input.commentId)
                throw new BusinessRuleError(422, "COMMENT_ID_REQUIRED", "Choose a comment to delete.");
            const comment = await r.comment(input.commentId, id);
            if (!comment || comment.state !== "published")
                throw new BusinessRuleError(404, "COMMENT_NOT_FOUND", "This comment is unavailable.");
            const ownAiReply = comment.author_id !== actor.userId && await r.ownAgentReply(id, actor.userId, input.commentId);
            if (comment.author_id !== actor.userId && (input.action === "edit_comment" || !ownAiReply))
                throw new BusinessRuleError(403, "COMMENT_AUTHOR_REQUIRED", "Only the author can delete this comment.");
            if (input.action === "edit_comment") {
                if (comment.author_kind === "agent")
                    throw new BusinessRuleError(403, "AI_COMMENT_EDIT_FORBIDDEN", "AI replies cannot be edited.");
                const body = z.string().trim().min(1).max(10000).parse(input.body);
                let rich: RichContent;
                try {
                    const parsed = validateRichContent(input.richContent, 10000);
                    if (parsed.body !== body || JSON.stringify(parsed.mentions) !== JSON.stringify(input.mentions ?? []))
                        throw Error("Rich text and mentions differ from the message.");
                    rich = parsed.content;
                }
                catch (error) {
                    throw new BusinessRuleError(422, "RICH_CONTENT_INVALID", error instanceof Error ? error.message : "Invalid rich content.");
                }
                const previousImages = (comment.rich_content_json ? (JSON.parse(comment.rich_content_json) as RichContent).blocks : [])
                    .filter(block => block.type === "image");
                if (JSON.stringify(rich.blocks.filter(block => block.type === "image")) !== JSON.stringify(previousImages))
                    throw new BusinessRuleError(422, "COMMENT_IMAGES_CHANGED", "Comment images cannot be changed while editing text.");
                const mentions = await ports.validateMentions(actor, body, input.mentions);
                const agentMentions = mentions.filter(mention => mention.kind === "agent");
                const previousAgentMentions = (JSON.parse(comment.mentions_json) as Mention[]).filter(mention => mention.kind === "agent");
                if (JSON.stringify(agentMentions) !== JSON.stringify(previousAgentMentions))
                    throw new BusinessRuleError(422, "COMMENT_AI_MENTION_EDIT", "AI requests cannot be changed while editing a comment.");
                steps.push(r.guardCommentAuthor(key, "comment-author", input.commentId, id, actor.userId, comment.version));
                steps.push(r.clearCommentMentions(input.commentId));
                steps.push(r.clearCommentTopics(input.commentId));
                steps.push(r.updateComment(body, JSON.stringify(rich), JSON.stringify(mentions), now, key, input.commentId, id, actor.userId, comment.version));
                steps.push(...await ports.textStatements(actor, id, "comment", input.commentId, body, mentions.filter(mention => mention.kind !== "agent")));
                for (const mention of agentMentions)
                    steps.push(r.prepareAgentMention(input.commentId, mention.kind, mention.targetId, mention.label, actor.userId, now));
                return ports.commands.commit(actor, key, operation, input, id, post.version + 1, { id: input.commentId, version: comment.version + 1 }, steps);
            }
            steps.push(r.guardCommentRemoval(key, "comment-author", input.commentId, id, comment.version, actor.userId, actor.userId));
            steps.push(r.removeComment(now, key, input.commentId, id, comment.version));
            return ports.commands.commit(actor, key, operation, input, id, post.version + 1, { id: input.commentId }, steps);
        }
        if (input.action === "edit" || input.action === "retract") {
            if (post.author_id !== actor.userId || post.kind !== "team" || post.linked_module)
                throw new BusinessRuleError(403, "POST_AUTHOR_REQUIRED", "Only the author can change this team post.");
            // AI replies and other comments advance the parent version; deletion uses
            // the latest row read above so an older feed snapshot can still be removed.
            if (input.action === "edit" && input.expectedVersion !== post.version)
                throw new BusinessRuleError(409, "POST_CHANGED", "This post changed. Reload it before trying again.");
            const content = storedContent(post.content_json);
            if (input.action === "edit" && post.rich_content_json && input.richContent === undefined)
                throw new BusinessRuleError(422, "RICH_CONTENT_REQUIRED", "Reload this post in the advanced editor before changing its text.");
            steps.push(r.prepareRevision(`revision-${key}`, id, post.version, input.action, post.body, post.content_json, post.rich_content_json, post.mentions_json, actor.userId, now));
            if (input.action === "retract") {
                steps.push(r.retract(now, now, id, actor.userId, post.version));
            }
            else {
                const body = z.string().max(30000).parse(input.body);
                const editedContent = postContentSchema.parse(input.content ?? [
                    { type: "text" as const, text: body },
                    ...(content ? contentImages(content) : []),
                ]);
                if (!body.trim() && !contentImages(editedContent).length)
                    throw new BusinessRuleError(422, "POST_BODY_REQUIRED", "Add a message or image.");
                if (contentBody(editedContent) !== body)
                    throw new BusinessRuleError(422, "POST_CONTENT_CHANGED", "The post text changed. Review the edit and try again.");
                const editedImages = contentImages(editedContent);
                if (editedImages.length > 10)
                    throw new BusinessRuleError(422, "ATTACHMENT_LIMIT", "Posts accept up to ten images.");
                if (editedImages.some(image => !image.attachmentId) ||
                    new Set(editedImages.map(image => image.slotId)).size !== editedImages.length ||
                    new Set(editedImages.map(image => image.attachmentId)).size !== editedImages.length)
                    throw new BusinessRuleError(422, "POST_IMAGE_INVALID", "Each post image needs a unique uploaded file.");
                let rich: RichContent | null = null;
                if (input.richContent !== undefined) {
                    try {
                        const parsed = validateRichContent(input.richContent, 30000);
                        rich = parsed.content;
                        if (parsed.body !== body || JSON.stringify(parsed.mentions) !== JSON.stringify(input.mentions ?? []))
                            throw Error("Rich text and mentions differ from the message.");
                        if (rich.blocks.length !== editedContent.length || rich.blocks.some((block, index) => {
                            const ordered = editedContent[index];
                            return block.type !== ordered.type || block.type === "image" && ordered.type === "image" &&
                                (block.slotId !== ordered.slotId || block.attachmentId !== ordered.attachmentId ||
                                    block.name !== ordered.name || block.alt !== ordered.alt || block.width !== ordered.width);
                        }))
                            throw Error("Rich text and post content differ.");
                    }
                    catch (e) {
                        throw new BusinessRuleError(422, "RICH_CONTENT_INVALID", e instanceof Error ? e.message : "Invalid rich content.");
                    }
                }
                else if (post.rich_content_json) {
                    throw new BusinessRuleError(422, "RICH_CONTENT_REQUIRED", "Reload this post in the editor before changing its content.");
                }
                const mentions = await ports.validateMentions(actor, body, input.mentions);
                if (!rich) {
                    let offset = 0;
                    rich = { version: 1, blocks: editedContent.map(block => {
                            if (block.type === "image")
                                return { type: "image" as const, slotId: block.slotId, attachmentId: block.attachmentId,
                                    name: block.name, alt: block.alt, width: block.width };
                            const local = mentions.filter(mention => mention.start >= offset && mention.end <= offset + block.text.length)
                                .map(mention => ({ ...mention, start: mention.start - offset, end: mention.end - offset }));
                            offset += block.text.length + 1;
                            return { type: "text" as const, doc: (plainRichContent(block.text, local).blocks[0] as {
                                    doc: RichNode;
                                }).doc };
                        }) };
                }
                const currentImageIds = new Set(content
                    ? contentImages(content).flatMap(image => image.attachmentId ? [image.attachmentId] : [])
                    : (await r.legacyImages(id, actor.companyId)).results.map(image => image.id));
                const retained = editedImages.filter(image => currentImageIds.has(image.attachmentId!));
                const newlyUploaded = editedImages.filter(image => !currentImageIds.has(image.attachmentId!));
                if (newlyUploaded.length && !input.editDraftId)
                    throw new BusinessRuleError(422, "POST_IMAGE_UPLOAD_REQUIRED", "Upload new images before saving this edit.");
                let editDraft: PostDraft | null = null;
                if (input.editDraftId) {
                    editDraft = await socialUploadOwner(actor, input.editDraftId, true);
                    if (editDraft.edit_post_id !== id || editDraft.edit_post_version !== post.version || editDraft.edit_saved_version !== null)
                        throw new BusinessRuleError(422, "POST_IMAGE_UPLOAD_UNAVAILABLE", "These uploaded images belong to a different post edit.");
                    if (!newlyUploaded.length)
                        throw new BusinessRuleError(422, "POST_IMAGE_UPLOAD_UNUSED", "Remove the unused image upload and try again.");
                }
                const existingFound = retained.length ? (await r.retainedImages(retained.map(image => image.attachmentId), id, id, actor.companyId)).results : [];
                if (existingFound.length !== retained.length)
                    throw new BusinessRuleError(422, "POST_IMAGE_UNAVAILABLE", "An existing image is no longer available.");
                const uploadedFound = newlyUploaded.length && editDraft ? (await r.uploadedImages(newlyUploaded.map(image => image.attachmentId), input.editDraftId, actor.userId, actor.companyId, id, post.version)).results : [];
                if (uploadedFound.length !== newlyUploaded.length)
                    throw new BusinessRuleError(422, "POST_IMAGE_UNAVAILABLE", "An uploaded image is unavailable. Upload it again.");
                const priorMentions = JSON.parse(post.mentions_json ?? "[]") as Mention[];
                const newRecipients: string[] = [];
                for (const mention of mentions) {
                    if (mention.kind !== "person" || priorMentions.some(previous => previous.kind === "person" && previous.targetId === mention.targetId))
                        continue;
                    await ports.readablePost({ companyId: actor.companyId, userId: mention.targetId }, id);
                    if (!newRecipients.includes(mention.targetId))
                        newRecipients.push(mention.targetId);
                }
                const tags = [...new Set(hashtags(body).filter(tag => !mentions.some(mention => tag.start < mention.end && tag.end > mention.start)).map(tag => tag.tag))];
                steps.push(...(retained.map((image, index) => r.guardRetainedImage(key, `existing-image-${index}`, image.attachmentId, id, id, actor.companyId))), ...(newlyUploaded.map((image, index) => r.guardUploadedImage(key, `new-image-${index}`, image.attachmentId, input.editDraftId!, actor.userId, actor.companyId, id, post.version))), ...(editDraft && input.editDraftId ? [r.guardEditUpload(key, "edit-upload-draft", input.editDraftId, actor.userId, actor.companyId, id, post.version)] : []), r.clearPostMentions(id), r.clearPostTopics(id), r.updatePost(body, JSON.stringify(editedContent), rich ? JSON.stringify(rich) : null, JSON.stringify(mentions), now, now, id, actor.userId, post.version), ...(editDraft && input.editDraftId ? [r.bindEditUpload(post.version + 1, now, input.editDraftId)] : []), r.revokeUnusedPostFiles(now, id, id, editedImages.map(image => image.attachmentId)), r.expireUnusedPostUploads(now, id, id), ...mentions.map(mention => r.prepareMention(id, mention.kind, mention.targetId, mention.label, actor.userId, now)), ...tags.flatMap(tag => [
                    r.prepareTopic(tag, actor.userId, now),
                    r.preparePostTopic(id, tag),
                ]), ...newRecipients.map(user => r.prepareEditNotification(ports.identifiers.next(), user, now, id, user, now, now)));
            }
        }
        else if (input.action === "comment") {
            const body = z.string().max(10000).parse(input.body);
            let rich: RichContent | null = null;
            if (input.richContent !== undefined) {
                try {
                    const parsed = validateRichContent(input.richContent, 10000);
                    rich = parsed.content;
                    if (parsed.body !== body || JSON.stringify(parsed.mentions) !== JSON.stringify(input.mentions ?? []))
                        throw Error("Rich text and mentions differ from the message.");
                }
                catch (e) {
                    throw new BusinessRuleError(422, "RICH_CONTENT_INVALID", e instanceof Error ? e.message : "Invalid rich content.");
                }
            }
            const images = rich?.blocks.filter(block => block.type === "image") ?? [];
            if (!body.trim() && !images.length)
                throw new BusinessRuleError(422, "COMMENT_BODY_REQUIRED", "Add a message or image.");
            if (images.length > 10 || new Set(images.map(image => image.attachmentId)).size !== images.length || images.some(image => !image.attachmentId))
                throw new BusinessRuleError(422, "COMMENT_IMAGES_INVALID", "Finish uploading up to ten unique images.");
            if (images.length && !input.commentDraftId)
                throw new BusinessRuleError(422, "COMMENT_UPLOAD_REQUIRED", "Upload images before replying.");
            if (input.commentDraftId) {
                const upload = await socialUploadOwner(actor, input.commentDraftId, true);
                if (upload.comment_parent_post_id !== id)
                    throw new BusinessRuleError(404, "COMMENT_UPLOAD_UNAVAILABLE", "Comment images belong to another conversation.");
                const found = images.length ? (await r.commentImages(images.map(image => image.attachmentId), input.commentDraftId, actor.companyId)).results : [];
                if (found.length !== images.length)
                    throw new BusinessRuleError(422, "COMMENT_IMAGE_UNAVAILABLE", "An image is unavailable. Upload it again.");
                steps.push(r.guardCommentUpload(key, "comment-upload", input.commentDraftId, actor.userId, id));
                for (const image of images)
                    steps.push(r.guardCommentImage(key, `image-${image.slotId}`, image.attachmentId, input.commentDraftId, actor.companyId));
            }
            const mentions = await ports.validateMentions(actor, body, input.mentions);
            const parent = input.parentCommentId
                ? await r.parentComment(input.parentCommentId, id)
                : null;
            if (input.parentCommentId && !parent)
                throw new BusinessRuleError(422, "REPLY_TARGET_UNAVAILABLE", "Choose a visible comment to reply to.");
            if (parent)
                steps.push(r.guardParentComment(key, "reply-parent", parent.id, id));
            steps.push(r.prepareComment(`comment-${key}`, id, actor.userId, actor.name, body, key, now, now, JSON.stringify(mentions), parent?.id ?? null, rich ? JSON.stringify(rich) : null, input.commentDraftId ?? null), ...await ports.textStatements(actor, id, "comment", `comment-${key}`, body, mentions));
            if (input.commentDraftId)
                steps.push(r.bindCommentDraft(`comment-${key}`, now, input.commentDraftId));
            if (input.commentDraftId)
                steps.push(r.revokeUnusedCommentFiles(now, input.commentDraftId, images.map(image => image.attachmentId)));
            if (parent && parent.author_id !== actor.userId)
                steps.push(r.prepareReplyNotification(ports.identifiers.next(), parent.author_id, now, id, parent.author_id, now, now));
            for (const [i, user] of [...new Set(input.mentionUserIds ?? [])]
                .slice(0, 10)
                .entries()) {
                await ports.readablePost({ companyId: actor.companyId, userId: user }, id);
                steps.push(ports.sourceGuard(key, { ...post, version: post.version + 1 }, user, `mention-${i}`), ports.notification(post, user, "post", id, post.version + 1, "You were mentioned in a conversation"));
            }
        }
        else if (input.action === "appreciate")
            steps.push(input.active
                ? r.prepareAppreciation(id, actor.userId, now)
                : r.removeAppreciation(id, actor.userId));
        else if (input.action === "follow")
            steps.push(input.active
                ? r.prepareFollow(actor.userId, id, now)
                : r.removeFollow(actor.userId, id));
        else if (input.action === "report")
            steps.push(r.prepareReport(`report-${key}`, id, actor.userId, z.string().trim().min(1).max(2000).parse(input.body), now));
        else {
            await ports.requireCompanyAuthority(actor, ["holding_owner"]);
            steps.push(ports.authorityGuard(key, actor, ["holding_owner"]), r.moderate(now, id));
        }
        return ports.commands.commit(actor, key, operation, input, id, post.version + 1, input.action === "edit" || input.action === "retract"
            ? { id, version: post.version + 1 }
            : { id }, steps);
    }
    return { socialAudienceOptions, socialUploadOwner, createCommentUploadDraft, cancelCommentUploadDraft, createPostEditUploadDraft, cancelPostEditUploadDraft, savePostDraft, publishPost, postComments, postViews, postView, homePosts, postRevisionHistory, postAction };
}
