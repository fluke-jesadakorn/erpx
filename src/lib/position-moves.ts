import { z } from "zod";

const id = z.string().min(1).max(180);
export const positionMoveInput = z.object({
  positionId: id,
  expectedVersion: z.number().int().positive(),
  targetKind: z.enum(["department", "level"]),
  targetId: id,
  effectiveAt: z.number().int().positive(),
  timezone: z.string().max(80).refine(value => {
    try { new Intl.DateTimeFormat("en", { timeZone: value }); return true; } catch { return false; }
  }, "Choose a valid timezone.").default("Asia/Bangkok"),
  managerAssignments: z.array(z.object({ engagementId: id, managerPersonId: id.nullable() })).max(10000).default([]),
  reportsToPositionId: id.nullable().optional(),
  reason: z.string().trim().min(3).max(1000),
}).refine(value => new Set(value.managerAssignments.map(item => item.engagementId)).size === value.managerAssignments.length,
  "Choose each employee's manager once.");
export const positionMoveSubmitInput = positionMoveInput.safeExtend({ previewToken: z.string().min(1), expiresAt: z.number().int().positive() });
export type PositionMoveInput = z.infer<typeof positionMoveInput>;
export type PositionMovePlacement = { branchId: string | null; departmentId: string | null; levelId: string | null; managerPersonId: string | null };
export type PositionMovePolicyImpact = {
  connectionId: string; gpoId: string; userFilterGroupId: string; computerTargetOuId: string;
  beforeSources: { ruleId: string; kind: "level" | "department" | "position"; id: string }[];
  afterSources: PositionMovePolicyImpact["beforeSources"];
};
export type PositionMovePreview = {
  previewToken: string; expiresAt: number; ready: boolean; blockers: string[];
  before: { branchId: string | null; departmentId: string | null; levelId: string; reportsToPositionId: string | null };
  after: PositionMovePreview["before"];
  people: { engagementId: string; personId: string; name: string; before: PositionMovePlacement; after: PositionMovePlacement; leaveEffectiveFrom: string | null; leaveNote: string }[];
  policyChanges: { added: PositionMovePolicyImpact[]; removed: PositionMovePolicyImpact[]; retained: PositionMovePolicyImpact[] };
  affectedCount: number; effectiveAt: number; timezone: string;
};
export type PositionMoveRecord = {
  id: string; company_id: string; position_id: string; status: "scheduled" | "applied" | "needs_review" | "cancelled";
  version: number; effective_at: number; definition: PositionMoveInput; preview: PositionMovePreview;
  created_by: string; created_at: number; updated_at: number; applied_at: number | null; last_error: string | null;
};

export function positionMoveDate(timestamp: number, timezone: string) {
  const parts = new Intl.DateTimeFormat("en-US", { timeZone: timezone, year: "numeric", month: "2-digit", day: "2-digit" }).formatToParts(new Date(timestamp));
  return ["year", "month", "day"].map(key => parts.find(part => part.type === key)!.value).join("-");
}
