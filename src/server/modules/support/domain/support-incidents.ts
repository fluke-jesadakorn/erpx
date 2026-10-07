import { z } from "zod";
import type { WorkContext } from "@/server/modules/identity_access/public/authorization";
import type { WorkItem } from "@/lib/work-item";
import { hasPermission } from "@/lib/permissions";
import { samePerson } from "@/lib/iam";
import type { AttachmentBindingContract, AttachmentManifestItem } from "@/server/modules/files/public/attachments/types";
import type { SupportRow as Row } from "./support-facts";
const id = z.string().min(1), version = z.number().int().positive(), reason = z.string().trim().min(3).max(4000), attachmentSetIds = z.array(id).max(1).default([]);
export const incidentCreateSchema = z.object({ ticketId: id, ticketVersion: version, title: z.string().trim().min(3).max(160), impact: reason, severity: z.enum(['critical', 'high', 'normal']), ownerId: id, dueDate: z.iso.date() });
export const incidentChangeSchema = z.discriminatedUnion('action', [
    z.object({ action: z.enum(['start', 'update', 'reject', 'close', 'reopen', 'cancel']), expectedVersion: version, reason, attachmentSetIds }),
    z.object({ action: z.literal('mitigate'), expectedVersion: version, reason, rootCause: reason, mitigation: reason, attachmentSetIds }),
    z.object({ action: z.literal('reassign'), expectedVersion: version, reason, ownerId: id, dueDate: z.iso.date(), attachmentSetIds }),
    z.object({ action: z.literal('unlink'), expectedVersion: version, reason, ticketId: id, attachmentSetIds }),
    z.object({ action: z.literal('link'), expectedVersion: version, reason, ticketId: id, ticketVersion: version, attachmentSetIds }),
]);
export type SupportIncidentCreate = z.infer<typeof incidentCreateSchema>;
export type SupportIncidentChange = z.infer<typeof incidentChangeSchema>;
export type SupportIncidentBinding = {
    manifest: AttachmentManifestItem[];
    readonly native: unknown;
};
export function supportIncidentBindingContract(status: unknown): AttachmentBindingContract { return { targetType: 'support_incident', bindingType: 'support_incident_action', contexts: [{ purposeKey: 'incident_evidence', componentKind: 'action', componentRef: `incident:${String(status)}` }] }; }
const terminal = (s: unknown) => ['closed', 'cancelled'].includes(String(s));
export function incidentWorkItems(a: WorkContext, rows: Row[]): WorkItem[] {
    return rows.map(r => { const done = terminal(r.status), review = r.status === 'mitigated', canAct = !done && (review ? hasPermission(a, 'support.manage') && Number(r.reviewer_eligible) > 0 : hasPermission(a, 'support.work') && samePerson(a, r.owner_id) && Number(r.owner_eligible) > 0); return { id: String(r.id), type: 'support_incident', title: `${r.severity} incident · ${r.title}`, detail: 'Shared cause; each linked customer case retains its own outcome.', href: `/support/incidents/${r.id}`, status: String(r.status), updatedAt: Number(r.updated_at), queue: done ? 'completed' : canAct ? 'todo' : 'waiting', canAct, dueAt: Date.parse(String(r.due_date) + 'T00:00:00Z'), responsibleQueue: review ? 'Independent Support manager' : 'Assigned incident owner', nextAction: review ? 'Review mitigation or request rework' : 'Investigate and record mitigation evidence', blocker: !done && (review ? !Number(r.reviewer_count) : !Number(r.owner_eligible)) ? 'No eligible recipient; Support manager or administrator coverage is required' : null, relationships: [samePerson(a, r.owner_id) ? 'assigned' : samePerson(a, r.created_by) ? 'requested' : canAct ? 'approval' : 'team'] }; });
}
