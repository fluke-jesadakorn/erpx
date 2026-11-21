import type { ProjectGraph } from '@erpx/server-projects/domain/graph';
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
