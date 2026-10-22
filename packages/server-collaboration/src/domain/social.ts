import { z } from 'zod';
export const feedFilterSchema = z.enum(["all", "following", "team", "records"]);
export type FeedFilter = z.infer<typeof feedFilterSchema>;
export const audienceSchema = z.enum(["holding", "branch"]);
export const tagSchema = z.string().trim().toLowerCase().regex(/^[a-z0-9][a-z0-9_-]{0,39}$/);
export const mentionSchema = z.object({
    kind: z.enum(["person", "record"]),
    targetId: z.string().min(1).max(260),
    label: z.string().min(1).max(160),
});
export const createPostSchema = z.object({
    body: z.string().trim().min(1).max(3000),
    audience: audienceSchema,
    tags: z.array(tagSchema).max(5).default([]),
    linkedRecord: z.object({
        module: z.literal("people_payroll"),
        id: z.string().min(1).max(128),
    }).nullable().default(null),
    mentions: z.array(mentionSchema).max(20).default([]),
});
export const createCommentSchema = z.object({ body: z.string().trim().min(1).max(1000), mentions: z.array(mentionSchema).max(20).default([]) });
export const setActiveSchema = z.object({ active: z.boolean() });
export const createStorySchema = z.object({
    title: z.string().trim().min(1).max(80),
    body: z.string().trim().min(1).max(500),
    audience: audienceSchema.default("holding"),
    theme: z.enum(["sage", "sunset", "sky", "plum"]).default("sage"),
});
export type SocialComment = {
    id: string;
    postId: string;
    authorId: string;
    authorName: string;
    body: string;
    createdAt: number;
};
export type SocialPost = {
    id: string;
    kind: "team" | "record" | "system";
    authorId: string;
    authorName: string;
    body: string;
    audience: "Holding" | "Branch";
    branchId: string | null;
    linkedRecord: {
        module: string;
        id: string;
        label: string;
    } | null;
    version: number;
    createdAt: number;
    appreciationCount: number;
    commentCount: number;
    appreciated: boolean;
    followed: boolean;
    topics: string[];
    comments: SocialComment[];
};
