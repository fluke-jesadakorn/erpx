import { z } from "zod";
import type { WorkContext } from "@/server/modules/identity_access/public/authorization";
import type { WorkItem } from "@/lib/work_projection/work-item";
import type { SupportRow as Row } from "./support-facts";
import { hasPermission } from "@/lib/identity_access/permissions";
const reason = z.string().trim().min(3).max(2000), version = z.number().int().positive();
export const supportSessionRequestSchema = z.object({ ticketId: z.string().min(1), ticketVersion: version, requestedFor: z.string().min(1), expiresAt: z.number().int().positive(), reason });
export const supportSessionActionSchema = z.discriminatedUnion('action', [
    z.object({ action: z.enum(['approve', 'reject', 'revoke']), expectedVersion: version, reason }),
    z.object({ action: z.literal('issue-capability'), expectedVersion: version, code: z.enum(['customer_order_context', 'ticket_evidence_metadata']), reason }),
    z.object({ action: z.literal('revoke-capability'), expectedVersion: version, capabilityId: z.string().min(1), expectedCapabilityVersion: version, reason }),
]);
export type SupportSessionRequest = z.infer<typeof supportSessionRequestSchema>;
export type SupportSessionAction = z.infer<typeof supportSessionActionSchema>;
export type SupportSession = Row & {
    id: string;
    status: string;
    requested_for: string;
    requested_for_name: string | null;
    created_by: string;
    reviewed_by: string | null;
    version: number;
    expires_at: number;
    updated_at: number;
    title: string;
    can_review: number;
    target_eligible: number;
};
export type SupportContextRow = Row & {
    ticket_id: string;
    order_number: string;
    order_kind: string;
    order_state: string;
    due_date: string;
    currency_code: string;
};
export function supportSessionProjection(a: WorkContext, rows: SupportSession[], now: () => number) {
    const instant = now();
    return rows.map(r => {
        const expired = Number(r.expires_at) <= instant, eligible = Number(r.can_review) > 0 && hasPermission(a, 'support.manage');
        const canReview = r.status === 'pending' && !expired && r.created_by !== a.userId && r.requested_for !== a.userId && eligible;
        const canManage = r.status === 'approved' && !expired && eligible;
        return { ...r, effective_status: ['pending', 'approved'].includes(r.status) && expired ? 'expired' : r.status,
            canReview, canManage, canApprove: canReview, canIssueCapability: canManage };
    });
}
export type SupportSessionView = ReturnType<typeof supportSessionProjection>[number];
export function supportSessionWorkItems(a: WorkContext, rows: SupportSessionView[]): WorkItem[] { return rows.map(r => { const pending = r.status === 'pending', active = r.effective_status === 'approved', canAct = pending ? Boolean(r.canReview) : active ? Boolean(r.canManage) : false; return { id: String(r.id), type: 'support_session', actionKind: pending && canAct ? 'approval' : 'work', title: `Temporary support access · ${r.title}`, detail: `${r.requested_for_name ?? r.requested_for} · expires ${new Date(Number(r.expires_at)).toISOString()}`, href: `/support/sessions/${encodeURIComponent(String(r.id))}`, status: String(r.effective_status), updatedAt: Number(r.updated_at), dueAt: Number(r.expires_at), queue: ['rejected', 'revoked', 'expired'].includes(String(r.effective_status)) ? 'completed' : canAct ? 'todo' : 'waiting', canAct, responsibleQueue: pending ? 'Independent Support manager' : 'Support manager', nextAction: pending ? 'Review requested customer access' : 'Issue or revoke an explicit capability', blocker: Number(r.target_eligible) ? null : 'Requested support colleague is no longer eligible; revoke this session', relationships: [r.created_by === a.userId ? 'requested' : r.requested_for === a.userId ? 'assigned' : canAct ? 'approval' : 'team'] }; }); }
