import {z} from 'zod';import type {RecordScope} from '@erpx/server-operations/public/staff-policy';import type {ApprovalView} from '@erpx/server-workflow/public/approvals';export const leaveDraftSchema = z.object({
  startDate: z.iso.date(),
  days: z.number().int().min(1).max(365),
  handoverNote: z.string().trim().max(2048),
  expectedVersion: z.number().int().positive(),
  saveOnly: z.boolean().default(false),
});
export const leaveDecisionSchema = z.object({
  expectedVersion: z.number().int().positive(),
  note: z.string().trim().max(2048).default(""),
  override: z.boolean().default(false),
});
export const newLeaveSchema = leaveDraftSchema.omit({
  expectedVersion: true,
  saveOnly: true,
});
export const leaveLifecycleSchema = leaveDecisionSchema.extend({
  action: z.enum(["reject", "cancel"]),
});
export type LeaveRequest = RecordScope & {
  id: string;
  number: string;
  employee_id: string;
  person_id: string | null;
  engagement_id: string | null;
  start_date: string;
  days: number;
  handover_note: string;
  state: string;
  version: number;
  created_by: string;
  prepared_by: string | null;
  decided_by: string | null;
  decision_note: string | null;
  updated_at: number;
  leave_type_id?: string | null;
  leave_type_name?: string | null;
  end_date?: string | null;
  calculated_units?: number | null;
  cancellation_state?: string | null;
  approval: ApprovalView;
  cancellationApproval?: ApprovalView;
  start_segment?: "full"|"first_half"|"second_half";
  end_segment?: "full"|"first_half"|"second_half";
  calculation_snapshot?: string;
  policy_version_id?: string | null;
  evidence_policy_snapshot?: unknown;
  evidence_due_at?: number | null;
  evidence_version?: number;
  submitted_at?: number | null;
  permittedActions: string[];
};
export const eventScope = (row: RecordScope) => ({
  companyId: row.company_id,
  branchId: row.branch_id,
  departmentId: row.department_id,
});
