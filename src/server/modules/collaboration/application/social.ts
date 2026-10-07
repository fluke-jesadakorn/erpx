import { z } from 'zod';
import type { WorkContext } from '../../identity_access/public/authorization';
import { BusinessRuleError, type Clock, type IdentifierSource } from '../../shared';
import { createPostSchema, createCommentSchema, createStorySchema, mentionSchema, type FeedFilter, type SocialPost } from '../domain/social';
type Row = Record<string, unknown>;
export interface SocialRepository<Effect> {
    priorReceipt(commandId: string): Promise<{
        request_hash: string;
        result_json: string | null;
    } | null>;
    activeMentionAccount(targetId: string, now: number, now2: number): Promise<Record<string, unknown> | null>;
    prepareMention(ownerType: "post" | "comment", ownerId: string, kind: "person" | "record", targetId: string, label: string, userId: string, now: number): Effect;
    preparePostReceipt(commandId: string, value: "social.post.publish", userId: string, requestHash: string, id: string, value2: string, now: number): Effect;
    preparePost(id: string, value: "record" | "team", userId: string, name: string, companyId: string, branchId: string | null, value2: null, body: string, value3: "people_payroll" | null, value4: string | null, linkedLabel: string | null, value5: "published", value6: 1, commandId: string, now: number, now2: number, value7: "branch" | "company"): Effect;
    prepareTopic(tag: string, userId: string, now: number): Effect;
    preparePostTopic(id: string, tag: string): Effect;
    guardPublishedPost(commandId: string, value: "publish", id: string, commandId2: string, companyId: string): Effect;
    preparePostAudit(value: string, commandId: string, userId: string, assignmentId: string, value2: "social.post.publish", value3: "social_post", id: string, value4: 1, value5: "Publish workplace update", now: number): Effect;
    preparePostOutbox(value: string, commandId: string, value2: "social.post.published", value3: "social_post", id: string, value4: 1, value5: string, now: number): Effect;
    clearPostAssertions(commandId: string): Effect;
    prepareCommentReceipt(commandId: string, value: "social.comment.publish", userId: string, requestHash: string, id: string, value2: string, now: number): Effect;
    prepareComment(id: string, postId: string, userId: string, name: string, body: string, commandId: string, now: number, now2: number): Effect;
    guardPublishedComment(commandId: string, value: "comment", id: string, postId: string, commandId2: string): Effect;
    prepareCommentAudit(value: string, commandId: string, userId: string, assignmentId: string, value2: "social.comment.publish", value3: "social_post", postId: string, version: unknown, value4: "Comment on workplace update", now: number): Effect;
    prepareCommentOutbox(value: string, commandId: string, value2: "social.comment.published", value3: "social_post", postId: string, version: unknown, value4: string, now: number): Effect;
    clearCommentAssertions(commandId: string): Effect;
    prepareAppreciation(postId: string, userId: string, now: number): Effect;
    prepareAppreciationRemoval(postId: string, userId: string): Effect;
    prepareFollow(userId: string, postId: string, now: number): Effect;
    prepareFollowRemoval(userId: string, postId: string): Effect;
    prepareFlagReceipt(commandId: string, operation: "social.post.appreciate" | "social.post.follow", userId: string, requestHash: string, postId: string, value: string, now: number): Effect;
    prepareFlagAudit(value: string, commandId: string, userId: string, assignmentId: string, operation: "social.post.appreciate" | "social.post.follow", value2: "social_post", postId: string, version: unknown, value3: string, now: number): Effect;
    clearFlagAssertions(commandId: string): Effect;
    prepareStoryReceipt(commandId: string, value: "social.story.publish", userId: string, requestHash: string, id: string, value2: string, now: number): Effect;
    prepareStory(id: string, userId: string, name: string, companyId: string, branchId: string | null, departmentId: string | null, title: string, body: string, theme: "sage" | "sunset" | "sky" | "plum", commandId: string, now: number, now2: number): Effect;
    guardPublishedStory(commandId: string, value: "story", id: string, commandId2: string): Effect;
    prepareStoryAudit(value: string, commandId: string, userId: string, assignmentId: string, value2: "social.story.publish", value3: "social_story", id: string, value4: 1, value5: "Publish workplace story", now: number): Effect;
    prepareStoryOutbox(value: string, commandId: string, value2: "social.story.published", value3: "social_story", id: string, value4: 1, value5: string, now: number): Effect;
    clearStoryAssertions(commandId: string): Effect;
    prepareArchiveReceipt(commandId: string, value: "social.story.archive", userId: string, requestHash: string, storyId: string, value2: string, now: number): Effect;
    prepareArchiveStory(nextVersion: number, commandId: string, now: number, storyId: string, userId: string, expectedVersion: number, companyId: string, branchId: string | null, branchId2: string | null, departmentId: string | null, departmentId2: string | null): Effect;
    guardArchivedStory(commandId: string, value: "archive-story", storyId: string, nextVersion: number, commandId2: string): Effect;
    prepareArchiveAudit(value: string, commandId: string, userId: string, assignmentId: string, value2: "social.story.archive", value3: "social_story", storyId: string, nextVersion: number, value4: "Archive workplace story", now: number): Effect;
    clearArchiveAssertions(commandId: string): Effect;
}
export interface SocialPorts<Effect> {
    repository: SocialRepository<Effect>;
    clock: Clock;
    identifiers: IdentifierSource;
    sha256(text: string): Promise<string>;
    commit(effects: Effect[]): Promise<unknown>;
    feed(actor: WorkContext, filter: FeedFilter): Promise<{
        rows: Row[];
        comments: {
            results: Row[];
        }[];
    }>;
    stories(actor: WorkContext): Promise<{
        results: Row[];
    }>;
    visiblePost(actor: WorkContext, id: string): Promise<{
        id: string;
        version: number;
    } | null>;
    mentionPrincipal(id: string): Promise<{
        display_name: string;
    } | null>;
    linkedLeave(holdingId: string, id: string, actor: WorkContext): Promise<{
        number: string;
    }>;
    getRecord(actor: WorkContext, moduleId: string, id: string): Promise<{
        title: string;
    }>;
    flagGuard(key: string, type: 'reaction' | 'follow', post: string, user: string, active: boolean): Effect;
}
export function createSocialApplication<Effect>(ports: SocialPorts<Effect>) {
    const r = ports.repository;
    async function listFeed(holdingId: string, context: WorkContext, filter: FeedFilter = "all") {
        const { rows, comments } = await ports.feed(context, filter);
        return rows.map((row, index): SocialPost => ({
            id: String(row.id),
            kind: row.kind as SocialPost["kind"],
            authorId: String(row.author_id),
            authorName: String(row.author_name),
            body: String(row.body),
            audience: row.branch_id ? "Branch" : "Holding",
            branchId: row.branch_id ? String(row.branch_id) : null,
            linkedRecord: row.linked_record_id ? {
                module: String(row.linked_module), id: String(row.linked_record_id), label: String(row.linked_record_label),
            } : null,
            version: Number(row.version),
            createdAt: Number(row.created_at),
            appreciationCount: Number(row.appreciation_count),
            commentCount: Number(row.comment_count),
            appreciated: Boolean(row.appreciated),
            followed: Boolean(row.followed),
            topics: String(row.topic_list).split(",").filter(Boolean),
            comments: ((comments[index]?.results ?? []) as Record<string, unknown>[]).map((comment) => ({
                id: String(comment.id), postId: String(comment.post_id), authorId: String(comment.author_id),
                authorName: String(comment.author_name), body: String(comment.body), createdAt: Number(comment.created_at),
            })),
        }));
    }
    async function listStories(holdingId: string, context: WorkContext) {
        const result = await ports.stories(context);
        return result.results.map((row) => ({
            id: String(row.id), authorId: String(row.author_id), authorName: String(row.author_name),
            title: String(row.title), body: String(row.body), theme: String(row.theme), version: Number(row.version),
            audience: row.branch_id ? "Branch" : "Holding", createdAt: Number(row.created_at),
        }));
    }
    async function existingReceipt(commandId: string, requestHash: string) {
        const row = await r.priorReceipt(commandId);
        if (!row)
            return null;
        if (row.request_hash !== requestHash)
            throw new BusinessRuleError(409, "IDEMPOTENCY_CONFLICT", "This command key was used with different input.");
        return row.result_json ? JSON.parse(row.result_json) : { commandId };
    }
    async function mentionStatements(ownerType: "post" | "comment", ownerId: string, mentions: z.infer<typeof mentionSchema>[], context: WorkContext, now: number) {
        const unique = [...new Map(mentions.map(item => [`${item.kind}:${item.targetId}`, item])).values()];
        const resolved: typeof unique = [];
        for (const mention of unique) {
            if (mention.kind === "person") {
                const found = await r.activeMentionAccount(mention.targetId, now, now);
                if (!found)
                    throw new BusinessRuleError(404, "MENTION_TARGET_NOT_FOUND", "That person is not available in this Holding.");
                const principal = await ports.mentionPrincipal(mention.targetId);
                if (!principal)
                    throw new BusinessRuleError(404, "MENTION_TARGET_NOT_FOUND", "That person is unavailable.");
                resolved.push({ ...mention, label: principal.display_name });
            }
            else {
                const [moduleId, ...idParts] = mention.targetId.split("/");
                const record = await ports.getRecord(context, moduleId, idParts.join("/"));
                resolved.push({ ...mention, label: record.title });
            }
        }
        return resolved.map(mention => r.prepareMention(ownerType, ownerId, mention.kind, mention.targetId, mention.label, context.userId, now));
    }
    async function createPost(holdingId: string, input: z.infer<typeof createPostSchema>, commandId: string, context: WorkContext) {
        const requestHash = await ports.sha256(JSON.stringify({ operation: "social.post.publish", input }));
        const prior = await existingReceipt(commandId, requestHash);
        if (prior)
            return prior;
        if (input.audience === "holding" && context.assignmentScope !== "holding")
            throw new BusinessRuleError(403, "AUDIENCE_FORBIDDEN", "Your current role cannot share with the whole company.");
        if (input.audience === "branch" && (context.assignmentScope !== "branch" || !context.branchId))
            throw new BusinessRuleError(403, "AUDIENCE_FORBIDDEN", "Choose a branch assignment to share with that branch.");
        let linkedLabel: string | null = null;
        if (input.linkedRecord) {
            const record = await ports.linkedLeave(holdingId, input.linkedRecord.id, context);
            linkedLabel = `${record.number} · Leave request`;
        }
        const id = ports.identifiers.next();
        const now = ports.clock.now();
        const branchId = input.audience === "branch" ? context.branchId : null;
        const tags = [...new Set([
                ...input.tags,
                ...Array.from(input.body.matchAll(/#([a-z0-9][a-z0-9_-]{0,39})/gi), (match) => match[1].toLowerCase()),
            ])].slice(0, 5);
        const result = { id, state: "published", version: 1, commandId };
        const mentions = await mentionStatements("post", id, input.mentions, context, now);
        const statements = [
            r.preparePostReceipt(commandId, "social.post.publish", context.userId, requestHash, id, JSON.stringify(result), now),
            r.preparePost(id, input.linkedRecord ? "record" : "team", context.userId, context.name, context.companyId, branchId, null, input.body, input.linkedRecord?.module ?? null, input.linkedRecord?.id ?? null, linkedLabel, "published", 1, commandId, now, now, input.audience === "holding" ? "company" : "branch"),
            ...tags.flatMap((tag) => [
                r.prepareTopic(tag, context.userId, now),
                r.preparePostTopic(id, tag),
            ]),
            ...mentions,
            r.guardPublishedPost(commandId, "publish", id, commandId, context.companyId),
            r.preparePostAudit(ports.identifiers.next(), commandId, context.userId, context.assignmentId, "social.post.publish", "social_post", id, 1, "Publish workplace update", now),
            r.preparePostOutbox(ports.identifiers.next(), commandId, "social.post.published", "social_post", id, 1, JSON.stringify({ ...result, companyId: context.companyId, branchId }), now),
            r.clearPostAssertions(commandId),
        ];
        try {
            await ports.commit(statements);
            return result;
        }
        catch {
            const committed = await existingReceipt(commandId, requestHash);
            if (committed)
                return committed;
            throw new BusinessRuleError(409, "PUBLISH_CONFLICT", "The update could not be published. Refresh and try again.");
        }
    }
    async function visiblePost(id: string, context: WorkContext) {
        const post = await ports.visiblePost(context, id);
        if (!post)
            throw new BusinessRuleError(404, "POST_NOT_FOUND", "The update is unavailable.");
        return post;
    }
    async function createComment(holdingId: string, postId: string, input: z.infer<typeof createCommentSchema>, commandId: string, context: WorkContext) {
        const post = await visiblePost(postId, context);
        const requestHash = await ports.sha256(JSON.stringify({ operation: "social.comment.publish", postId, input }));
        const prior = await existingReceipt(commandId, requestHash);
        if (prior)
            return prior;
        const id = ports.identifiers.next();
        const now = ports.clock.now();
        const result = { id, postId, authorId: context.userId, authorName: context.name, body: input.body, createdAt: now, commandId };
        const mentions = await mentionStatements("comment", id, input.mentions, context, now);
        try {
            await ports.commit([
                r.prepareCommentReceipt(commandId, "social.comment.publish", context.userId, requestHash, id, JSON.stringify(result), now),
                r.prepareComment(id, postId, context.userId, context.name, input.body, commandId, now, now),
                ...mentions,
                r.guardPublishedComment(commandId, "comment", id, postId, commandId),
                r.prepareCommentAudit(ports.identifiers.next(), commandId, context.userId, context.assignmentId, "social.comment.publish", "social_post", postId, post.version, "Comment on workplace update", now),
                r.prepareCommentOutbox(ports.identifiers.next(), commandId, "social.comment.published", "social_post", postId, post.version, JSON.stringify(result), now),
                r.clearCommentAssertions(commandId),
            ]);
            return result;
        }
        catch {
            const committed = await existingReceipt(commandId, requestHash);
            if (committed)
                return committed;
            throw new BusinessRuleError(409, "COMMENT_CONFLICT", "The comment could not be published.");
        }
    }
    async function setPostFlag(holdingId: string, postId: string, active: boolean, commandId: string, context: WorkContext, type: "reaction" | "follow") {
        const post = await visiblePost(postId, context);
        const operation = type === "reaction" ? "social.post.appreciate" : "social.post.follow";
        const requestHash = await ports.sha256(JSON.stringify({ operation, postId, active }));
        const prior = await existingReceipt(commandId, requestHash);
        if (prior)
            return prior;
        const now = ports.clock.now();
        const result = { postId, active, commandId };
        const mutation = type === "reaction"
            ? active
                ? r.prepareAppreciation(postId, context.userId, now)
                : r.prepareAppreciationRemoval(postId, context.userId)
            : active
                ? r.prepareFollow(context.userId, postId, now)
                : r.prepareFollowRemoval(context.userId, postId);
        try {
            await ports.commit([
                r.prepareFlagReceipt(commandId, operation, context.userId, requestHash, postId, JSON.stringify(result), now),
                mutation,
                ports.flagGuard(commandId, type, postId, context.userId, active),
                r.prepareFlagAudit(ports.identifiers.next(), commandId, context.userId, context.assignmentId, operation, "social_post", postId, post.version, active ? `Enable ${type}` : `Disable ${type}`, now),
                r.clearFlagAssertions(commandId),
            ]);
            return result;
        }
        catch {
            const committed = await existingReceipt(commandId, requestHash);
            if (committed)
                return committed;
            throw new BusinessRuleError(409, "SOCIAL_ACTION_CONFLICT", "The update changed before this action completed.");
        }
    }
    async function createStory(holdingId: string, input: z.infer<typeof createStorySchema>, commandId: string, context: WorkContext) {
        if (input.audience === "branch" && !context.branchId)
            throw new BusinessRuleError(422, "BRANCH_REQUIRED", "Select a branch assignment to publish to a branch.");
        const requestHash = await ports.sha256(JSON.stringify({ operation: "social.story.publish", input }));
        const prior = await existingReceipt(commandId, requestHash);
        if (prior)
            return prior;
        const id = ports.identifiers.next();
        const now = ports.clock.now();
        const branchId = input.audience === "branch" ? context.branchId : null;
        const result = { id, state: "published", version: 1, commandId };
        try {
            await ports.commit([
                r.prepareStoryReceipt(commandId, "social.story.publish", context.userId, requestHash, id, JSON.stringify(result), now),
                r.prepareStory(id, context.userId, context.name, context.companyId, branchId, context.departmentId, input.title, input.body, input.theme, commandId, now, now),
                r.guardPublishedStory(commandId, "story", id, commandId),
                r.prepareStoryAudit(ports.identifiers.next(), commandId, context.userId, context.assignmentId, "social.story.publish", "social_story", id, 1, "Publish workplace story", now),
                r.prepareStoryOutbox(ports.identifiers.next(), commandId, "social.story.published", "social_story", id, 1, JSON.stringify({ ...result, companyId: context.companyId, branchId }), now),
                r.clearStoryAssertions(commandId),
            ]);
            return result;
        }
        catch {
            const committed = await existingReceipt(commandId, requestHash);
            if (committed)
                return committed;
            throw new BusinessRuleError(409, "STORY_CONFLICT", "The story could not be published.");
        }
    }
    async function archiveStory(holdingId: string, storyId: string, expectedVersion: number, commandId: string, context: WorkContext) {
        const requestHash = await ports.sha256(JSON.stringify({ operation: "social.story.archive", storyId, expectedVersion }));
        const prior = await existingReceipt(commandId, requestHash);
        if (prior)
            return prior;
        const now = ports.clock.now();
        const nextVersion = expectedVersion + 1;
        const result = { id: storyId, state: "archived", version: nextVersion, commandId };
        try {
            await ports.commit([
                r.prepareArchiveReceipt(commandId, "social.story.archive", context.userId, requestHash, storyId, JSON.stringify(result), now),
                r.prepareArchiveStory(nextVersion, commandId, now, storyId, context.userId, expectedVersion, context.companyId, context.branchId, context.branchId, context.departmentId, context.departmentId),
                r.guardArchivedStory(commandId, "archive-story", storyId, nextVersion, commandId),
                r.prepareArchiveAudit(ports.identifiers.next(), commandId, context.userId, context.assignmentId, "social.story.archive", "social_story", storyId, nextVersion, "Archive workplace story", now),
                r.clearArchiveAssertions(commandId),
            ]);
            return result;
        }
        catch {
            const committed = await existingReceipt(commandId, requestHash);
            if (committed)
                return committed;
            throw new BusinessRuleError(409, "STORY_CONFLICT", "Only the author can archive the current story.");
        }
    }
    const setAppreciation = (holdingId: string, postId: string, active: boolean, commandId: string, context: WorkContext) => setPostFlag(holdingId, postId, active, commandId, context, 'reaction');
    const setFollow = (holdingId: string, postId: string, active: boolean, commandId: string, context: WorkContext) => setPostFlag(holdingId, postId, active, commandId, context, 'follow');
    return { listFeed, listStories, createPost, createComment, setAppreciation, setFollow, createStory, archiveStory };
}
