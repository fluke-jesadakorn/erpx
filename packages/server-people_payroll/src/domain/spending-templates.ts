import {z} from 'zod';export const spendingTemplateSchema = z.object({
  expectedVersion: z.number().int().positive(),
  reason: z.string().trim().min(3).max(1000),
  exceptions: z.array(z.object({
    positionId: z.string().min(1).nullable(), requestKind: z.enum(["expense","purchase","payment"]).nullable(),
    action: z.enum(["skip_supervisor","named_supervisor"]), supervisorUserId: z.string().min(1).nullable(),
    reason: z.string().trim().min(3).max(1000),
  })).max(100),
});
export type SpendingTemplateInput=z.infer<typeof spendingTemplateSchema>;
