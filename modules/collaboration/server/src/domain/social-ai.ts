import { z } from 'zod';
export const aiRequestSchema = z.object({
    kind: z.enum(["summary", "task"]),
    commentIds: z.array(z.string().min(1).max(160)).max(20).default([]),
});
export const aiResultSchema = z
    .object({
    summary: z.string().max(3000),
    title: z.string().max(200).nullable(),
    description: z.string().max(6000).nullable(),
    assigneeUserId: z.string().max(160).nullable(),
    reviewerUserId: z.string().max(160).nullable(),
    dueAt: z.iso.datetime({ offset: true }).nullable(),
    missing: z.array(z.string().max(200)).max(10),
    citations: z.array(z.string().max(160)).min(1).max(21),
})
    .strict();
export type AiResult = z.infer<typeof aiResultSchema>;
export const jsonSchema = {
    type: "object",
    additionalProperties: false,
    properties: {
        summary: { type: "string" },
        title: { type: ["string", "null"] },
        description: { type: ["string", "null"] },
        assigneeUserId: { type: ["string", "null"] },
        reviewerUserId: { type: ["string", "null"] },
        dueAt: { type: ["string", "null"] },
        missing: { type: "array", items: { type: "string" } },
        citations: { type: "array", items: { type: "string" } },
    },
    required: [
        "summary",
        "title",
        "description",
        "assigneeUserId",
        "reviewerUserId",
        "dueAt",
        "missing",
        "citations",
    ],
};
