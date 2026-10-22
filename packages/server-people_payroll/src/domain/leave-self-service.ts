import {leaveCreateSchema} from "@erpx/feature-people_payroll/lib/leave-self-service";import {leaveEvidencePurposes} from '@erpx/server-people_payroll/domain/leave-evidence';
import {z} from 'zod';import type {LeaveEvidencePolicy,LeaveEvidencePurpose} from '@erpx/server-people_payroll/domain/leave-evidence';export type Policy = {
  assignment_id: string;
  policy_version_id: string;
  policy_name: string;
  policy_version: number;
  timezone: string;
  workweek_json: number[] | string;
  allow_negative: boolean;
  holiday_calendar_id: string | null;
};
export type LeaveType = {
  id: string;
  code: string;
  name: string;
  description: string;
  tracked: boolean;
  evidence_after_units: number | null;
  evidence_note: string;
  evidence_min_units: number | null;
  evidence_purpose: LeaveEvidencePurpose | null;
  evidence_min_count: number;
  evidence_due_stage: "before_submit" | "before_final_approval";
  evidence_grace_days: number;
  rule_evidence_note: string;
  evidence_privacy: "requester_approvers_hr";
  grant_kind: "annual" | "period" | "accrual" | "untracked";
  grant_units: number;
  accrual_units: number;
  accrual_period: string | null;
  carryover_limit: number | null;
  employee_eligible: boolean;
  contractor_eligible: boolean;
};
export function evidencePolicy(type: LeaveType, units: number): LeaveEvidencePolicy {
  const minimumUnits = type.evidence_min_units === null
      ? null
      : Number(type.evidence_min_units),
    minimumCount = Number(type.evidence_min_count),
    required = Boolean(
      type.evidence_purpose &&
        minimumUnits !== null &&
        units >= minimumUnits &&
        minimumCount > 0,
    );
  return {
    required,
    minimumUnits,
    purposeKey: type.evidence_purpose,
    minimumCount,
    dueStage: type.evidence_due_stage,
    graceDays: Number(type.evidence_grace_days),
    privacy: type.evidence_privacy,
    note: type.rule_evidence_note,
  };
}
export function workweek(value: number[] | string) {
  const parsed = typeof value === "string" ? JSON.parse(value) : value;
  return Array.isArray(parsed) && parsed.every((x) => Number.isInteger(x) && x >= 1 && x <= 7)
    ? parsed
    : [1, 2, 3, 4, 5];
}
export const leaveListFilterSchema = z.enum([
  "all",
  "drafts",
  "review",
  "approved",
  "closed",
]);
export const leaveListPageSchema = z.object({
  filter: leaveListFilterSchema.default("all"),
  limit: z.coerce.number().int().min(1).max(50).default(10),
  cursor: z.string().max(512).optional(),
});
export const leaveCursorSchema = z.object({
  at: z.number().int().nonnegative(),
  id: z.string().min(1).max(180),
});
export type LeaveListRow = {
  id: string;
  number: string;
  start_date: string;
  end_date: string | null;
  start_segment: string | null;
  end_segment: string | null;
  calculated_units: number | null;
  days: number;
  state: string;
  cancellation_state: string | null;
  version: number;
  created_at: number;
  updated_at: number;
  decision_note: string | null;
  leave_type_name: string | null;
};
export const leaveMonthSchema = z
  .string()
  .regex(/^\d{4}-(0[1-9]|1[0-2])$/, "Choose a valid calendar month.");
export type LeaveCalendarMonth = {
  month: string;
  requests: LeaveListRow[];
  holidays: { holiday_date: string; name: string }[];
};
export const leaveEditSchema=leaveCreateSchema.extend({expectedVersion:z.number().int().positive()});
export const leavePolicyRuleSchema = z.object({
  leaveTypeId: z.string().min(1),
  grantKind: z.enum(["annual","period","accrual","untracked"]),
  grantUnits: z.number().min(0).default(0),
  accrualUnits: z.number().min(0).default(0),
  accrualPeriod: z.enum(["monthly","quarterly","annual"]).nullable().default(null),
  carryoverLimit: z.number().min(0).nullable().default(null),
  employeeEligible:z.boolean().default(true),
  contractorEligible:z.boolean().default(false),
  evidencePurpose: z.enum(leaveEvidencePurposes).nullable().default(null),
  evidenceMinUnits: z.number().positive().max(365).nullable().default(null),
  evidenceMinCount: z.number().int().min(0).max(100).default(0),
  evidenceDueStage: z.enum(["before_submit","before_final_approval"]).default("before_final_approval"),
  evidenceGraceDays: z.number().int().min(0).max(365).default(0),
  evidenceNote: z.string().trim().max(500).default(""),
}).superRefine((rule,ctx)=>{
  const configured=rule.evidencePurpose!==null||rule.evidenceMinUnits!==null||rule.evidenceMinCount>0;
  if(configured && (!rule.evidencePurpose||rule.evidenceMinUnits===null||rule.evidenceMinCount<1))
    ctx.addIssue({code:"custom",message:"Evidence rules need a purpose, minimum leave units, and at least one required file."});
});
export const leaveAdminSchema = z.discriminatedUnion("resource", [
  z.object({ resource: z.literal("type"), code: z.string().min(1).max(40), name: z.string().min(1).max(100), description: z.string().max(500).default(""), tracked: z.boolean().default(true), evidenceAfterUnits: z.number().positive().nullable().default(null), evidenceNote: z.string().max(500).default("") }),
  z.object({ resource: z.literal("calendar"), name: z.string().min(1).max(100), timezone: z.string().min(1).max(80) }),
  z.object({ resource: z.literal("policy"), policyKey: z.string().min(1).max(80), version: z.number().int().positive(), name: z.string().min(1).max(100), effectiveFrom: z.iso.date(), effectiveTo: z.iso.date().nullable().default(null), timezone: z.string().min(1).max(80), workweek: z.array(z.number().int().min(1).max(7)).min(1), allowNegative: z.boolean().default(false), rules: z.array(leavePolicyRuleSchema).min(1) }),
  z.object({ resource: z.literal("holiday"), calendarId: z.string().min(1), date: z.iso.date(), name: z.string().min(1).max(100) }),
  z.object({ resource: z.literal("assignment"), engagementId: z.string().min(1), policyVersionId: z.string().min(1), calendarId: z.string().min(1).nullable().default(null), effectiveFrom: z.iso.date(), effectiveTo: z.iso.date().nullable().default(null) }),
  z.object({resource:z.literal("assignment_end"),id:z.string().min(1),expectedVersion:z.number().int().positive(),effectiveTo:z.iso.date()}),
  z.object({ resource: z.literal("adjustment"), engagementId: z.string().min(1), leaveTypeId: z.string().min(1), units: z.number().min(-365).max(365).refine((x) => x !== 0), effectiveDate: z.iso.date(), note: z.string().min(1).max(500) }),
]);
