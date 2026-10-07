import { z } from 'zod';
export const idSchema = z.string().min(1).max(160);
export const membersSchema = z
    .array(idSchema)
    .max(100)
    .transform((v) => [...new Set(v)].sort());
export const roomSchema = z.object({
    kind: z.enum(["direct", "group"]),
    name: z.string().trim().max(120).default(""),
    members: membersSchema,
});
export const teamSchema = z.object({
    name: z.string().trim().min(1).max(120),
    members: membersSchema,
});
export const channelSchema = teamSchema.extend({ private: z.boolean() });
export const manageSchema = teamSchema.extend({
    expectedVersion: z.number().int().positive(),
});
export const messageSchema = z
    .object({
    body: z.string().trim().max(5000),
    files: z.array(z.string().uuid()).max(10).default([]),
})
    .refine((v) => v.body.length > 0 || v.files.length > 0, "Write a message or attach a file.")
    .refine((v) => new Set(v.files).size === v.files.length, "Duplicate attachments.");
