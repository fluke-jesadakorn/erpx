import { z } from "zod";
import { sourceRecordInput } from "@/server/modules/shared";
import type { WorkContext } from "@/server/modules/identity_access/public/authorization";
import type { WorkItem } from "@/lib/work-item";
import { hasPermission } from "@/lib/permissions";
import { samePerson } from "@/lib/iam";
import type { SupportRow as Row } from "./support-facts";
import type { SupportClock } from "./support-clock";
const reason = z.string().trim().min(3).max(4000), version = z.number().int().positive(), id = z.string().min(1);
export const supportCreateSchema = z.union([
    z.object({ kind: z.literal('general'), title: z.string().trim().min(3).max(160), issue: reason }),
    z.object({ kind: z.literal('source'), source: sourceRecordInput.extend({ kind: z.enum(['pos_sale', 'pms_record', 'asset']), version }), title: z.string().trim().min(3).max(160), issue: reason }),
    z.object({ orderId: id, sourceVersion: version, title: z.string().trim().min(3).max(160), issue: reason, ownerId: id, dueDate: z.iso.date() }),
]);
export const supportChangeSchema = z.discriminatedUnion('action', [
    z.object({ action: z.enum(['start', 'wait-customer', 'resume', 'resolve', 'reject-resolution', 'reopen', 'cancel']), expectedVersion: version, reason }),
    z.object({ action: z.literal('reassign'), expectedVersion: version, reason, ownerId: id, dueDate: z.iso.date() }),
    z.object({ action: z.literal('triage'), expectedVersion: version, reason, ownerId: id, dueDate: z.iso.date() }),
    z.object({ action: z.literal('confirm-general'), expectedVersion: version, reason }),
    z.object({ action: z.literal('confirm'), expectedVersion: version, reason, noteId: id, noteVersion: version }),
    z.object({ action: z.literal('edit-report'), expectedVersion: version, title: z.string().trim().min(3).max(160), issue: reason, reason }),
    z.object({ action: z.literal('withdraw-report'), expectedVersion: version, reason }),
    z.object({ action: z.literal('add-information'), expectedVersion: version, reason })
]);
export type SupportCreate = z.infer<typeof supportCreateSchema>;
export type SupportChange = z.infer<typeof supportChangeSchema>;
export const supportClosed = (status: unknown) => ['closed', 'cancelled'].includes(String(status));
export function supportWorkItems(a: WorkContext, rows: Row[]): WorkItem[] {
    return rows.map(r => {
        const c = r.sla_clock_json ? JSON.parse(String(r.sla_clock_json)) as SupportClock : null, terminal = supportClosed(r.status), triage = r.status === 'triage', review = r.status === 'resolved';
        const canAct = !terminal && (triage ? hasPermission(a, 'support.manage') : review ? hasPermission(a, 'support.manage') && r.owner_id !== a.userId && !samePerson(a, r.resolved_by) : r.owner_id === a.userId && hasPermission(a, 'support.work') && Number(r.owner_eligible) > 0);
        return { id: String(r.id), type: 'support_ticket', title: String(r.title), detail: r.source_kind !== 'sales_order' ? 'Issue reported for Support triage' : 'Customer support case', href: `/support/${r.id}`, status: String(r.status), updatedAt: Number(r.updated_at), queue: terminal ? 'completed' : canAct ? 'todo' : 'waiting', canAct, responsibleQueue: triage ? 'Support triage' : review ? 'Independent Support manager' : 'Assigned support colleague', nextAction: triage ? 'Choose an owner and follow-up date' : review ? r.source_kind !== 'sales_order' ? 'Confirm outcome or return for further work' : 'Review customer confirmation or reject the proposed resolution' : r.status === 'waiting_customer' ? 'Follow up for information' : 'Accept and work the issue', dueAt: c?.state === 'running' && c.dueAt !== null ? c.dueAt : r.due_date ? Date.parse(String(r.due_date) + 'T00:00:00Z') : null, blocker: review && !Number(r.reviewer_count) ? 'No independent eligible Support manager; an administrator must assign authorized coverage' : !triage && !terminal && !Number(r.owner_eligible) ? 'Assigned support colleague is no longer eligible; manager reassignment is required' : r.blocked_reason ? String(r.blocked_reason) : null, relationships: [r.owner_id === a.userId ? 'assigned' : r.created_by === a.userId ? 'requested' : 'team'] };
    });
}
