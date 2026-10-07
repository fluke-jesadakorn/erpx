import { samePerson } from "@/lib/iam";
import { z } from "zod";
import type { ApprovalView } from "@/server/modules/workflow/public/approvals";
import type { RecordScope, StaffActor } from "@/server/modules/operations/public/staff-policy";
export const users = z.array(z.string().min(1).max(160)).max(20);
export const taskFieldsSchema = z.object({
    title: z.string().trim().min(2).max(200),
    description: z.string().trim().max(10000).default(""),
    assigneeUserId: z.string().min(1),
    projectId: z.string().min(1).nullable().default(null),
    startApprovers: users.default([]),
    completionApprovers: users.default([]),
});
export const taskSchema = taskFieldsSchema
    .extend({
    dueAt: z.iso.datetime({ offset: true }).nullable().default(null),
    expectedVersion: z.number().int().positive().default(1),
    reason: z.string().trim().max(2000).default(""),
})
    .refine((v) => !!v.dueAt || v.startApprovers.length + v.completionApprovers.length === 0, {
    message: "Set a due date for work that requires approval.",
    path: ["dueAt"],
});
export const projectSchema = z.object({
    title: z.string().trim().min(2).max(200),
    description: z.string().trim().max(10000).default(""),
    memberUserIds: z.array(z.string().min(1).max(160)).max(200).default([]),
    expectedVersion: z.number().int().positive().default(1),
});
export const taskActionSchema = z.object({
    action: z.enum([
        "submit",
        "start",
        "save-progress",
        "complete",
        "approve",
        "reject",
        "cancel",
    ]),
    expectedVersion: z.number().int().positive(),
    note: z.string().trim().max(10000).default(""),
    attachmentSetIds: z.array(z.string().min(1)).max(1).default([]),
});
export const taskDelegationSchema = z.object({
    delegateUserId: z.string().min(1),
    expiresAt: z.iso.datetime({ offset: true }),
    expectedVersion: z.number().int().positive(),
    reason: z.string().trim().min(3).max(2000),
});
export const taskDelegationRevokeSchema = z.object({
    delegationId: z.string().min(1),
    expectedVersion: z.number().int().positive(),
    reason: z.string().trim().min(3).max(2000),
});
export type TaskRow = RecordScope & {
    source_post_id?: string | null;
    id: string;
    title: string;
    description: string;
    creator_user_id: string;
    assignee_user_id: string;
    assignee_name: string;
    creator_name: string;
    project_id: string | null;
    project_title: string | null;
    project_owner_id: string | null;
    due_at: number | null;
    completion_note: string;
    start_approvers_json: string;
    completion_approvers_json: string;
    state: string;
    blocked_reason: string | null;
    version: number;
    revision: number;
    updated_at: number;
    template_id: string | null;
    template_version: number | null;
    schedule_id: string | null;
    occurrence_at: number | null;
    active_delegation_id: string | null;
    active_delegate_user_id: string | null;
    active_delegate_name: string | null;
    active_delegate_expires_at: number | null;
    active_delegate_reason?: string | null;
    active_delegate_version?: number | null;
};
export function isOrdinaryTask(task: TaskRow) {
    return task.template_id === null && task.schedule_id === null &&
        (JSON.parse(task.start_approvers_json) as string[]).length === 0 &&
        (JSON.parse(task.completion_approvers_json) as string[]).length === 0;
}
export type TaskDetail = {
    task: TaskRow;
    approval: ApprovalView;
    permittedActions: string[];
    blockedReason: string | null;
    delegation: {
        id: string;
        delegate_user_id: string;
        delegate_name: string;
        expires_at: number;
        reason: string;
        version: number;
    } | null;
    delegationHistory: {
        id: string;
        delegate_name: string;
        delegator_name: string;
        starts_at: number;
        expires_at: number;
        reason: string;
        status: "active" | "expired" | "revoked";
        revoked_by_name: string | null;
        revoked_at: number | null;
        revoke_reason: string | null;
    }[];
    attachments: {
        id: string;
        original_name: string;
        bytes: number;
        media_type: string;
    }[];
};
export type ProjectRow = RecordScope & {
    id: string;
    title: string;
    description: string;
    owner_user_id: string;
    lifecycle_status: 'active' | 'handoff_pending' | 'completed' | 'cancelled';
    version: number;
    updated_at: number;
};
export function canConfigureTask(actor: StaffActor, task: TaskRow) {
    return (samePerson(actor, task.creator_user_id) ||
        samePerson(actor, task.project_owner_id));
}
export function isActiveDelegate(actor: StaffActor, task: TaskRow) {
    return Boolean(task.active_delegate_user_id &&
        samePerson(actor, task.active_delegate_user_id));
}
export type TaskWorkState = Pick<TaskDetail, "approval" | "permittedActions" | "blockedReason">;
