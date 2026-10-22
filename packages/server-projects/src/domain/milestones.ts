import { z } from 'zod';
export const projectMilestoneSchema = z.object({
    title: z.string().trim().min(2).max(200),
    description: z.string().trim().max(10000).default(""),
    dueAt: z.iso.datetime({ offset: true }),
    ownerUserId: z.string().min(1).max(160),
    parentMilestoneId: z.string().min(1).max(200).nullable().optional(),
    ordinalPosition: z.number().int().min(0).max(1000000).optional(),
    dependsOnMilestoneIds: z.array(z.string().min(1).max(200)).max(50).refine((ids) => new Set(ids).size === ids.length, "Prerequisite milestones must be unique.").optional(),
    expectedVersion: z.number().int().positive().default(1),
    reason: z.string().trim().min(3).max(2000).default("Milestone details reviewed by the project owner."),
}).strict();
export const projectMilestoneTransitionSchema = z.object({
    action: z.enum(["start", "complete", "reopen", "cancel"]),
    expectedVersion: z.number().int().positive(),
    reason: z.string().trim().min(3).max(2000),
}).strict();
export type ProjectMilestoneRow = {
    id: string;
    company_id: string;
    project_id: string;
    parent_milestone_id: string | null;
    ordinal_position: number;
    title: string;
    description: string;
    due_at: number;
    owner_user_id: string;
    owner_name: string | null;
    state: "planned" | "in_progress" | "completed" | "cancelled";
    is_overdue?: boolean;
    version: number;
    created_by: string;
    completed_at: number | null;
    created_at: number;
    updated_at: number;
    depends_on_ids: string[];
    depends_on_titles: string[];
    blocked_by_titles: string[];
    blocked_by_count: number;
};
export type MilestoneInput = z.infer<typeof projectMilestoneSchema>;
export type MilestoneEventAction = "created" | "revised" | "started" | "completed" | "reopened" | "cancelled";
