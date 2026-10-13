import { z } from "zod";
import type { WorkContext } from "@/server/modules/identity_access/public/authorization";
import type { WorkItem } from "@/lib/work_projection/work-item";
import { hasPermission } from "@/lib/identity_access/permissions";
export type SupportAlert = {
    id: string;
    ticket_id: string;
    title: string;
    source_version: number;
    breached_at: number;
    detected_at: number;
    version: number;
    acknowledged_by: string | null;
    acknowledged_at: number | null;
    reason: string | null;
    company_id: string;
    branch_id: string;
    department_id: string | null;
    can_ack: number;
    recipients: number;
};
export const supportAlertAcknowledgeSchema = z.object({ expectedVersion: z.number().int().positive(), reason: z.string().trim().min(3).max(2000) });
export type SupportAlertAcknowledge = z.infer<typeof supportAlertAcknowledgeSchema>;
export function supportAlertWorkItems(a: WorkContext, rows: SupportAlert[]): WorkItem[] {
    return rows.map(r => {
        const done = !!r.acknowledged_by, canAct = !done && hasPermission(a, 'support.manage') && Number(r.can_ack) > 0;
        return {
            id: r.id, type: 'support_sla_alert', actionKind: 'work', title: `Target breached · ${r.title}`, detail: 'Record who will follow up; the breach and ticket state are retained.', href: '/support/sla',
            status: done ? 'acknowledged' : 'open', updatedAt: Number(r.acknowledged_at ?? r.detected_at), dueAt: Number(r.breached_at), queue: done ? 'completed' : canAct ? 'todo' : 'waiting', canAct,
            responsibleQueue: 'Eligible Support manager', nextAction: done ? 'Follow the recorded recovery plan on the source case' : 'Take responsibility and record the follow-up plan',
            blocker: !done && !Number(r.recipients) ? 'No eligible Support manager: administrator coverage is required' : null, relationships: [canAct ? 'approval' : 'team'],
        };
    });
}
