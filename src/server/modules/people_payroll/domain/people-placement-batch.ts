import {z} from 'zod';import type {LifecycleInput,LifecyclePreview} from '@/lib/people-lifecycle';export const id = z.string().min(1).max(180);
export const legacy = z.object({ id, expectedVersion: z.number().int().positive() }).strict();
export const placementBatchInput = z.object({
  effectiveAt: z.number().int().positive(),
  timezone: z.string().min(1).max(80).refine(value => { try { new Intl.DateTimeFormat("en", { timeZone: value }); return true; } catch { return false; } }),
  reason: z.string().trim().min(3).max(1000),
  items: z.array(z.object({
    engagementId: id,
    positionId: id,
    managerPersonId: id.nullable(),
    acceptRecommendedLeave: z.boolean(),
    supersedeLegacyCase: legacy.optional(),
  }).strict()).min(1).max(20),
}).strict().superRefine((value, context) => {
  const seen = new Set<string>();
  value.items.forEach((item, index) => {
    if (seen.has(item.engagementId)) context.addIssue({ code: "custom", path: ["items", index, "engagementId"], message: "Choose each employee once." });
    seen.add(item.engagementId);
  });
});
export type PlacementBatchInput = z.infer<typeof placementBatchInput>;
export type Prepared = { engagementId: string; input?: LifecycleInput; error?: string };
export type BatchItemPreview = { engagementId: string; ready: boolean; blockers: string[]; token?: string; legacyCaseToSupersede?: LifecyclePreview["legacyCaseToSupersede"]; affectedPeople?: number };
