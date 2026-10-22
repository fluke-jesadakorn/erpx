import { projectReadProjection as binding } from '@erpx/server-projects/public/project-read-projections'; /** Native joined facts preserve the caller's transaction and original SQL snapshot. */
export const projectReadProjection = {
    "unfinishedTasks": "NOT EXISTS(SELECT 1 FROM staff_tasks WHERE project_id=? AND state NOT IN ('completed','cancelled'))",
    "unassignedTasks": `NOT EXISTS(SELECT 1 FROM staff_tasks t WHERE t.project_id=? AND t.state<>'cancelled' AND ${binding.hasTaskBinding})`
} as const;
