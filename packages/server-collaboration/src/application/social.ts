import type { WorkContext } from '@erpx/server-identity_access/public/authorization';
import type { FeedFilter, SocialPost } from '@erpx/server-collaboration/domain/social';
type Row = Record<string, unknown>;
export interface SocialPorts {
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
export function createSocialApplication(ports: SocialPorts) {
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
    return { listFeed, listStories };
}
