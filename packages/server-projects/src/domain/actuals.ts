import { z } from 'zod';
import type { ProjectRow } from '@erpx/server-projects/domain/staff-projects';
export const projectWorkLogSchema = z.object({
    workDate: z.string().date(),
    minutes: z.number().int().min(1).max(1440),
    expectedVersion: z.number().int().positive().optional(),
}).strict();
export const projectWorkLogDecisionSchema = z.object({
    action: z.enum(["approve", "reject"]),
    expectedVersion: z.number().int().positive(),
    reason: z.string().trim().min(3).max(2000),
}).strict();
export const projectCostAllocationSchema = z.object({
    projectId: z.string().min(1).max(200),
    sourceLineId: z.string().min(1).max(200),
    amount: z.string().regex(/^\d+(\.\d{1,2})?$/),
    expectedVersion: z.number().int().positive().optional(),
}).strict();
export const projectCostAllocationDecisionSchema = z.object({
    action: z.enum(["approve", "reject"]),
    expectedVersion: z.number().int().positive(),
    reason: z.string().trim().min(3).max(2000),
}).strict();
export type ProjectWorkLogRow = {
    id: string;
    project_id: string;
    user_id: string;
    work_date: string;
    minutes: number;
    state: "submitted" | "approved" | "rejected";
    version: number;
    reviewed_by: string | null;
    rejection_reason: string;
    created_at: number;
    updated_at: number;
    is_actor: boolean;
    can_review: boolean;
};
export type ProjectCostAllocationRow = {
    id: string;
    project_id: string;
    journal_line_id: string;
    amount_minor: string;
    amount: string;
    state: string;
    version: number;
    created_by: string;
    approved_by: string | null;
    rejection_reason: string;
    created_at: number;
    updated_at: number;
    journal_id: string;
    journal_number: string;
    entry_date: string;
    currency: string;
    journal_status: string;
    account_code: string;
    account_name: string;
    can_review: boolean;
    is_creator: boolean;
};
export type ProjectCostSourceRow = {
    source_line_id: string;
    journal_id: string;
    journal_number: string;
    entry_date: string;
    currency: string;
    account_id: string;
    account_code: string;
    account_name: string;
    description: string;
    debit_minor: string;
    remaining_minor: string;
    debit: string;
    remaining: string;
};
export type ProjectFinanceScope = ProjectRow & {
    branch_id: string | null;
    department_id: string | null;
};
