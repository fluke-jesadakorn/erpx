import { z } from 'zod';
import type { WorkContext } from '../../identity_access/public/authorization';
import { mentionsSchema } from './social-text';
import type { Mention } from './social-text';
import type { RichContent } from '@/lib/home-rich-content';
import { BusinessRuleError } from '../../shared';
export type Actor = WorkContext;
export const postContentBlockSchema = z.discriminatedUnion("type", [
    z.object({ type: z.literal("text"), text: z.string().max(30000) }),
    z.object({
        type: z.literal("image"),
        slotId: z.string().min(1).max(160),
        attachmentId: z.string().min(1).max(160).nullable(),
        name: z.string().min(1).max(255),
        alt: z.string().max(500).optional(),
        width: z.number().int().min(25).max(100).optional(),
    }),
]);
export const postContentSchema = z.array(postContentBlockSchema).max(100);
export type PostContent = z.infer<typeof postContentSchema>;
export const postPurposeSchema = z.enum(["update", "plan", "blocker", "handover", "announcement"]);
export type PostPurpose = z.infer<typeof postPurposeSchema>;
export const postAudienceSchema = z.discriminatedUnion("kind", [
    z.object({ kind: z.literal("company") }),
    z.object({ kind: z.literal("branch"), id: z.string().min(1).max(160) }),
    z.object({ kind: z.literal("department"), id: z.string().min(1).max(160) }),
    z.object({ kind: z.literal("participants"), userIds: z.array(z.string().min(1).max(160)).min(1).max(20) }),
    z.object({ kind: z.literal("team"), id: z.string().min(1).max(160) }),
    z.object({ kind: z.literal("project"), id: z.string().min(1).max(160) }),
]);
export type PostAudience = z.infer<typeof postAudienceSchema>;
export const postSourceSchema = z.object({
    kind: z.enum(["staff_task", "staff_project", "pms_record"]),
    id: z.string().min(1).max(160),
    version: z.number().int().positive().optional(),
});
export type PostSource = z.infer<typeof postSourceSchema>;
export function contentBody(content: PostContent) {
    const text = content.filter((block) => block.type === "text").map((block) => block.text);
    return text.some((value) => value.trim()) ? text.join("\n") : "";
}
export function contentImages(content: PostContent) {
    return content.filter((block) => block.type === "image");
}
export function storedContent(json: string | null | undefined): PostContent | null {
    return json ? postContentSchema.parse(JSON.parse(json)) : null;
}
export const postDraftSchema = z.object({
    body: z.string().max(30000),
    content: postContentSchema.optional(),
    richContent: z.unknown().optional(),
    mentions: mentionsSchema.optional(),
    audience: postAudienceSchema.nullable().optional(),
    sources: z.array(postSourceSchema).max(5).optional(),
    // Existing API clients supplied branchId. A supplied value still denotes an
    // explicit branch/company choice; new composers send audience instead.
    branchId: z.string().max(160).nullable().optional(),
    purpose: postPurposeSchema.optional(),
    expectedVersion: z.number().int().positive().default(1),
});
export type PostDraft = {
    id: string;
    author_id: string;
    company_id: string;
    branch_id: string | null;
    audience_kind: PostAudience["kind"] | null;
    audience_id: string | null;
    participants_json: string;
    sources_json: string;
    purpose: PostPurpose;
    body: string;
    department_id: string | null;
    version: number;
    published_post_id: string | null;
    mentions_json: string;
    content_json: string | null;
    rich_content_json: string | null;
    comment_parent_post_id: string | null;
    published_comment_id: string | null;
    cancelled_at: number | null;
    edit_post_id: string | null;
    edit_post_version: number | null;
    edit_saved_version: number | null;
};
export function selectedAudience(actor: Actor, input: z.infer<typeof postDraftSchema>): PostAudience | null {
    if (input.audience !== undefined)
        return input.audience;
    if (input.branchId === undefined)
        return null;
    if (input.branchId === null)
        return { kind: "company" };
    if (actor.departmentId) {
        if (input.branchId !== actor.branchId)
            throw new BusinessRuleError(403, "AUDIENCE_FORBIDDEN", "Your current role cannot share with this branch.");
        return { kind: "department", id: actor.departmentId };
    }
    return { kind: "branch", id: input.branchId };
}
export function audienceScope(actor: Actor, audience: PostAudience | null) {
    if (!audience)
        return { branch: null, department: null, audienceId: null, participants: [] as string[] };
    if (audience.kind === "company") {
        if (actor.assignmentScope !== "holding")
            throw new BusinessRuleError(403, "AUDIENCE_FORBIDDEN", "Your current role cannot share with the whole company.");
        return { branch: null, department: null, audienceId: null, participants: [] as string[] };
    }
    if (audience.kind === "branch") {
        if (actor.assignmentScope !== "holding" &&
            !(actor.assignmentScope === "branch" && actor.branchId === audience.id))
            throw new BusinessRuleError(403, "AUDIENCE_FORBIDDEN", "Your current role cannot share with this branch.");
        return { branch: audience.id, department: null, audienceId: null, participants: [] as string[] };
    }
    if (audience.kind === "department") {
        if (actor.assignmentScope !== "department" || actor.departmentId !== audience.id)
            throw new BusinessRuleError(403, "AUDIENCE_FORBIDDEN", "Your current role cannot share with this department.");
        return { branch: actor.branchId, department: audience.id, audienceId: null, participants: [] as string[] };
    }
    const participants = audience.kind === "participants"
        ? [...new Set(audience.userIds.filter(id => id !== actor.userId))].sort() : [];
    if (audience.kind === "participants" && !participants.length)
        throw new BusinessRuleError(422, "AUDIENCE_REQUIRED", "Select at least one other person.");
    return { branch: actor.assignmentScope === "holding" ? null : actor.branchId,
        department: actor.assignmentScope === "department" ? actor.departmentId : null,
        audienceId: audience.kind === "team" || audience.kind === "project" ? audience.id : null,
        participants };
}
export type CommentRow = {
    id: string;
    post_id: string;
    body: string;
    author_id: string;
    author_name: string;
    created_at: number;
    mentions_json: string;
    rich_content_json: string | null;
    author_kind: "person" | "agent";
    parent_comment_id: string | null;
    parent_author_name: string | null;
};
export const commentView = (c: CommentRow) => ({ ...c, mentions: JSON.parse(c.mentions_json) as Mention[], richContent: c.rich_content_json ? JSON.parse(c.rich_content_json) as RichContent : null });
