import { z } from 'zod';
export const mentionsSchema = z.array(z.object({ kind: z.enum(["person", "record", "agent"]), targetId: z.string().min(1).max(260), label: z.string().min(1).max(160), start: z.number().int().min(0), end: z.number().int().positive() })).max(20);
export { ASSISTANT_ID, normalizeTag, hashtags, activeToken, validMentionSpan, editMentions } from "@erpx/server-collaboration/domain/social-text-tokens";
export type { Mention, Suggestion } from "@erpx/server-collaboration/domain/social-text-tokens";
