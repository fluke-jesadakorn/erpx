import { z } from 'zod';
import type { ProjectGraph } from '@erpx/server-projects/domain/graph';
export const version = z.number().int().positive(), id = z.string().min(1).max(200), reason = z.string().trim().min(3).max(2000);
export const projectStructureCommandSchema = z.discriminatedUnion('action', [
    z.object({ action: z.literal('assign-task'), expectedVersion: version, taskId: id, taskVersion: version, milestoneId: id, milestoneVersion: version, bindingVersion: z.number().int().nonnegative().default(0), reason }),
    z.object({ action: z.literal('unassign-task'), expectedVersion: version, taskId: id, taskVersion: version, bindingVersion: version, reason }),
    z.object({ action: z.literal('link-actual'), expectedVersion: version, milestoneId: id, milestoneVersion: version, sourceKind: z.enum(['work_log', 'cost_allocation']), sourceId: id, sourceVersion: version, reason }),
    z.object({ action: z.literal('prepare-handoff'), expectedVersion: version, recipientId: id, evidence: z.string().trim().min(3).max(10000), reason }),
    z.object({ action: z.enum(['accept-handoff', 'reject-handoff', 'cancel-handoff', 'reopen']), expectedVersion: version, handoffId: id, handoffVersion: version, reason }),
]);
export type HandoffRow = {
    id: string;
    company_id: string;
    project_id: string;
    source_project_version: number;
    recipient_id: string;
    status: 'pending' | 'accepted' | 'rejected' | 'cancelled';
    version: number;
    snapshot_json: string;
    snapshot_sha256: string;
    evidence: string;
    prepared_by: string;
    prepared_at: number;
    decided_by: string | null;
    decided_at: number | null;
    decision_reason: string | null;
};
export type ProjectHandoffView = Omit<HandoffRow, 'snapshot_json'> & {
    snapshot: ProjectGraph;
};
