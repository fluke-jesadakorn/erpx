import {z} from 'zod';
import {assistantPage} from "@erpx/feature-work_projection/lib/ai/workspace-assistant";
export const reason = z.string().trim().min(3).max(3000),
  version = z.number().int().positive();
export const sourcePath = z.string().max(300).transform((path, ctx) => {
  try { return path === "/" ? path : assistantPage(path).path; }
  catch { ctx.addIssue({ code: "custom", message: "Choose an available assistant source path." }); return z.NEVER; }
});
export const scopePath = sourcePath.refine(path => !path.includes("?"), "An assignment scope is a page path.");
export const agentSchema = z.object({
  name: z.string().trim().min(2).max(120),
  purpose: reason,
});
export const versionSchema = z.object({
  agentId: z.string().min(1),
  prompt: reason,
  allowedSources: z.array(sourcePath).min(1).max(30),
});
export const poolSchema = z.object({
  name: z.string().trim().min(2).max(120),
  userIds: z.array(z.string().min(1)).min(1).max(100),
});
export const assignSchema = z
  .object({
    agentVersionId: z.string().min(1),
    poolId: z.string().min(1).optional(),
    userId: z.string().min(1).optional(),
    scopePath,
  })
  .refine((x) => Boolean(x.poolId) !== Boolean(x.userId));
export const actionSchema = z.object({
  action: z.enum(["approve", "reject", "retire", "revoke"]),
  expectedVersion: version,
  reason,
});
export type AssistantGovernance = {
  sessionId: string;
  assignmentId: string | null;
  assignmentVersion: number | null;
  agentVersionId: string | null;
  agentVersionRevision: number | null;
  catalogVersion: number | null;
  poolId: string | null;
  poolVersion: number | null;
  prompt: string | null;
  allowedSources: string[];
};

