import type { StaffActor } from '@erpx/server-operations/public/staff-policy';
import type { PostRow } from '@erpx/server-collaboration/domain/social-source';
import { BusinessRuleError, type Clock } from '@erpx/server-shared/index';
export interface SocialSourceRepository<Effect> {
    publishedPost(id: string, companyId: string, userId: string, now: number, now2: number, value: string[]): Promise<PostRow | null>;
    guardPublishedSource(key: string, step: string, id: string, company_id: string, version: number, user: string, now: number, now2: number): Effect;
    selectedComments(id: string, ids: string[]): Promise<{
        results: {
            id: string;
            body: string;
            author_name: string;
            version: number;
        }[];
    }>;
}
export interface SocialSourcePorts<Effect> {
    repository: SocialSourceRepository<Effect>;
    clock: Clock;
    sha256(value: string): Promise<string>;
}
export function createSocialSourceApplication<Effect>(ports: SocialSourcePorts<Effect>) {
    const r = ports.repository;
    async function readablePost(actor: Pick<StaffActor, "userId" | "companyId"> & Partial<Pick<StaffActor, "assignmentId">>, id: string) {
        const now = ports.clock.now();
        const row = await r.publishedPost(id, actor.companyId, actor.userId, now, now, (actor.assignmentId ? [actor.assignmentId] : []));
        if (!row)
            throw new BusinessRuleError(404, "POST_NOT_FOUND", "This post is unavailable.");
        return row;
    }
    function sourceGuard(key: string, post: PostRow, user: string, step: string) {
        const now = ports.clock.now();
        return r.guardPublishedSource(key, step, post.id, post.company_id, post.version, user, now, now);
    }
    async function sourceSnapshot(actor: Pick<StaffActor, "userId" | "companyId">, id: string, commentIds: string[]) {
        const post = await readablePost(actor, id);
        if (post.linked_module)
            throw new BusinessRuleError(422, "SOURCE_UNSUPPORTED", "Create a standalone team post for this workflow. Linked business records keep their existing permissions.");
        const ids = [...new Set(commentIds)].sort();
        if (ids.length > 20)
            throw new BusinessRuleError(422, "SOURCE_LIMIT", "Select up to 20 comments.");
        const comments = ids.length
            ? (await r.selectedComments(id, ids)).results
            : [];
        if (comments.length !== ids.length)
            throw new BusinessRuleError(409, "SOURCE_CHANGED", "A selected comment is unavailable. Review the conversation again.");
        const sources = [
            {
                id: post.id,
                body: post.body,
                author: post.author_name,
                version: post.version,
            },
            ...comments.map((c) => ({
                id: c.id,
                body: c.body,
                author: c.author_name,
                version: c.version,
            })),
        ];
        const serialized = JSON.stringify(sources);
        if (new TextEncoder().encode(serialized).length > 18000)
            throw new BusinessRuleError(422, "SOURCE_LIMIT", "Select fewer comments.");
        return { post, sources, hash: await ports.sha256(serialized), commentIds: ids };
    }
    return { readablePost, sourceGuard, sourceSnapshot };
}
