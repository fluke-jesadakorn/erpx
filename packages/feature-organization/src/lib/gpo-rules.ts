import { z } from "zod";

const id = z.string().min(1).max(500);
export const gpoRuleTarget = z.object({ kind: z.enum(["level", "department", "position"]), id });
export type GpoRuleTarget = z.infer<typeof gpoRuleTarget>;
export const gpoRuleInput = z.object({
  id: id.optional(), target: z.object({ kind: z.literal("position"), id }), expectedVersion: z.number().int().nonnegative(),
  connectionId: id, gpoId: id, userFilterGroupId: id, computerTargetOuId: id,
  reason: z.string().trim().min(3).max(1000),
});
export const gpoRuleRemoval = z.object({ id, expectedVersion: z.number().int().positive(), reason: z.string().trim().min(3).max(1000) });
export type GpoRuleInput = z.infer<typeof gpoRuleInput>;
export type GpoRuleRemoval = z.infer<typeof gpoRuleRemoval>;
export type GpoRule = {
  id: string; target: GpoRuleTarget; targetName: string; version: number; revision: number;
  connectionId: string; gpoId: string; userFilterGroupId: string; computerTargetOuId: string;
  active: boolean; directoryStatus: "pending" | "directory_applied" | "blocked";
  directoryError: string | null; legacyPositionId: string | null;
};
export type EffectiveGpoRule = GpoRule & { inherited: boolean };
export type GpoRulePreview = {
  token: string; expiresAt: number; target: GpoRuleTarget; targetName: string;
  positions: { id: string; title: string }[];
  holders: { personId: string; engagementId: string; name: string; accountIds: string[] }[];
  changes: { add: string[]; remove: string[]; retained: string[] };
  inheritedByFuturePositions: boolean; endpointStatus: "unconfirmed";
};
export type GpoRulesOverview = {
  rules: GpoRule[];
  effectiveByPosition: Record<string, EffectiveGpoRule[]>;
  tasks: { positionId: string; positionName: string; version: number; status: "pending" | "blocked" | "completed" | "not_required"; reason: string | null; mappingCount: number; verifiedCount: number }[];
  jobs: { id: string; rule_id: string | null; position_id: string; person_id: string; engagement_id: string | null; account_id: string | null; connection_id: string; revision: number; desired_action: "add" | "remove"; status: string; version: number; last_error: string | null; directory_observed_at: number | null; endpoint_observed_at: number | null }[];
  canManage: boolean;
};
