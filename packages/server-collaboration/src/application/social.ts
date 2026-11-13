import { z } from 'zod';
import type { WorkContext } from '@erpx/server-identity_access/public/authorization';
import { BusinessRuleError, type Clock, type IdentifierSource } from '@erpx/server-shared/index';
import { createStorySchema, type FeedFilter, type SocialPost } from '@erpx/server-collaboration/domain/social';
type Row = Record<string, unknown>;
export interface SocialRepository<Effect> {
    priorReceipt(commandId: string): Promise<{
        request_hash: string;
        result_json: string | null;
    } | null>;
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
    return { listFeed, listStories, createStory, archiveStory };
}
