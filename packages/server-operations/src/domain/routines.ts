import { z } from "zod";
import { type RoutineRule } from "@erpx/feature-work_projection/lib/routine-calendar";
import type { RecordScope } from "@erpx/server-operations/public/staff-policy";
import { taskFieldsSchema } from '@erpx/server-operations/domain/staff-tasks';
export const templateSchema = taskFieldsSchema
    .extend({
    published: z.boolean().default(true),
    expectedVersion: z.number().int().positive().default(1),
})
    .refine((v) => v.startApprovers.length + v.completionApprovers.length > 0, {
    message: "Configure at least one start or completion approver.",
});
export const scheduleSchema = z
    .object({
    frequency: z.enum(["daily", "weekly", "monthly"]),
    timezone: z
        .string()
        .max(80)
        .default("Asia/Bangkok")
        .refine((v) => {
        try {
            new Intl.DateTimeFormat("en", { timeZone: v });
            return true;
        }
        catch {
            return false;
        }
    }, "Choose a valid IANA timezone."),
    occurrenceTime: z
        .string()
        .regex(/^([01]\d|2[0-3]):[0-5]\d$/)
        .default("09:00"),
    startDate: z.iso.date(),
    endDate: z.iso.date().nullable().default(null),
    weekday: z.number().int().min(0).max(6).nullable().default(null),
    monthDay: z.number().int().min(1).max(31).nullable().default(null),
    dueOffsetHours: z.number().int().min(0).max(8760).default(24),
    expectedVersion: z.number().int().positive().default(1),
})
    .refine((v) => !v.endDate || v.endDate >= v.startDate, {
    message: "End date must follow start date.",
});
export type TemplateRow = RecordScope & {
    id: string;
    title: string;
    description: string;
    creator_user_id: string;
    assignee_user_id: string;
    project_id: string | null;
    start_approvers_json: string;
    completion_approvers_json: string;
    published: number;
    version: number;
    created_at: number;
    updated_at: number;
};
export type ScheduleRow = RoutineRule & {
    id: string;
    template_id: string;
    company_id: string;
    due_offset_hours: number;
    state: "active" | "paused" | "ended";
    next_occurrence_at: number | null;
    paused_at: number | null;
    last_generated_at: number | null;
    last_error: string | null;
    version: number;
    created_at: number;
    updated_at: number;
};
export const eventScope = (r: RecordScope) => ({
    companyId: r.company_id,
    branchId: r.branch_id,
    departmentId: r.department_id,
});
