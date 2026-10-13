import type { WorkContext } from "@/server/modules/identity_access/public/authorization";
import { hasPermission } from "@/lib/identity_access/permissions";
import { samePerson } from "@/lib/identity_access/iam";
import type { AttachmentBindingContract } from "@/server/modules/files/public/attachments/types";
import type { SupportCommandPorts, SupportCommand, SupportMutation, SupportRow } from "./command";
import { incidentCreateSchema, incidentChangeSchema, incidentWorkItems, supportIncidentBindingContract, type SupportIncidentCreate, type SupportIncidentChange, type SupportIncidentBinding } from "../domain/support-incidents";
export function createGetSupportIncident(ports: Pick<SupportCommandPorts, "authorize" | "error"> & {
    incident(a: WorkContext, id: string): Promise<SupportRow | null>;
}) { return async (a: WorkContext, id: string) => { ports.authorize(a, 'support.read'); const r = await ports.incident(a, id); if (!r)
    throw ports.error(404, 'SUPPORT_INCIDENT_NOT_FOUND', 'Incident is not available in this scope.'); return r; }; }
export function createSupportIncident(ports: SupportCommandPorts & {
    incident(a: WorkContext, id: string): Promise<SupportRow>;
    ticket(a: WorkContext, id: string): Promise<SupportRow>;
    prepare(command: SupportCommand, input: SupportIncidentCreate, source: SupportRow): Promise<SupportMutation>;
}) { return async (a: WorkContext, raw: unknown, key: string) => { ports.authorize(a, 'support.manage'); const input = incidentCreateSchema.parse(raw), id = `incident-${key}`, op = 'support.incident.create', prior = await ports.replay(a, key, op, id, input); if (prior) {
    await ports.incident(a, id);
    return prior;
} const source = await ports.ticket(a, input.ticketId), now = ports.now(); if (source.source_kind !== 'sales_order')
    throw ports.error(422, 'SUPPORT_INCIDENT_SOURCE_REQUIRED', 'Shared customer incidents require a linked customer order.'); const command: SupportCommand = { actor: a, key, operation: op, id, input, version: 1, result: { id, version: 1 }, now, scope: { companyId: a.companyId, branchId: String(source.branch_id), departmentId: source.department_id ? String(source.department_id) : null } }; return ports.commit(command, await ports.prepare(command, input, source)); }; }
export function createChangeSupportIncident(ports: SupportCommandPorts & {
    incident(a: WorkContext, id: string): Promise<SupportRow>;
    ticket(a: WorkContext, id: string): Promise<SupportRow>;
    binding(a: WorkContext, id: string, input: SupportIncidentChange, key: string, contract: AttachmentBindingContract): Promise<SupportIncidentBinding>;
    prepare(command: SupportCommand, input: SupportIncidentChange, row: SupportRow, manage: boolean, status: string | number | null, binding: SupportIncidentBinding | null): Promise<SupportMutation>;
}) {
    return async (a: WorkContext, id: string, raw: unknown, key: string) => {
        const input = incidentChangeSchema.parse(raw), manage = ['close', 'reject', 'reopen', 'cancel', 'reassign', 'link', 'unlink'].includes(input.action);
        ports.authorize(a, manage ? 'support.manage' : 'support.work');
        const op = `support.incident.${input.action}`, prior = await ports.replay(a, key, op, id, input);
        if (prior) {
            await ports.incident(a, id);
            return prior;
        }
        const r = await ports.incident(a, id), now = ports.now(), allowed: Record<string, string[]> = { start: ['open'], update: ['open', 'investigating'], mitigate: ['investigating'], reject: ['mitigated'], close: ['mitigated'], reopen: ['closed'], cancel: ['open', 'investigating', 'mitigated'], reassign: ['open', 'investigating', 'mitigated'], link: ['open', 'investigating'], unlink: ['open', 'investigating'] };
        if (!allowed[input.action].includes(String(r.status)))
            throw ports.error(409, 'SUPPORT_INCIDENT_STATE', 'Review the current incident state.');
        if (!manage && !samePerson(a, r.owner_id))
            throw ports.error(403, 'SUPPORT_INCIDENT_OWNER_REQUIRED', 'The assigned incident owner must perform this action.');
        if (input.action === 'close' && (samePerson(a, r.owner_id) || samePerson(a, r.mitigated_by)))
            throw ports.error(403, 'INDEPENDENT_APPROVER_REQUIRED', 'Another manager must review the incident mitigation.');
        const attachmentSetIds = input.attachmentSetIds ?? [];
        if (attachmentSetIds.length && !['start', 'update', 'mitigate'].includes(input.action))
            throw ports.error(422, 'INCIDENT_ATTACHMENT_ACTION', 'Bind incident evidence only while starting, updating, or proposing mitigation.');
        const prepared = attachmentSetIds.length ? await ports.binding(a, id, input, key, supportIncidentBindingContract(r.status)) : null;
        const status = ({ start: 'investigating', mitigate: 'mitigated', reject: 'investigating', close: 'closed', reopen: 'investigating', cancel: 'cancelled' } as Record<string, string>)[input.action] ?? r.status;
        if (input.action === 'link')
            await ports.ticket(a, input.ticketId);
        const command: SupportCommand = { actor: a, key, operation: op, id, input, now, version: input.expectedVersion + 1, result: { id, version: input.expectedVersion + 1, status, ...(prepared ? { manifest: prepared.manifest } : {}) }, scope: { companyId: a.companyId, branchId: String(r.branch_id), departmentId: r.department_id ? String(r.department_id) : null } };
        return ports.commit(command, await ports.prepare(command, input, r, manage, status, prepared));
    };
}
export function createIncidentWorkspace(ports: Pick<SupportCommandPorts, "authorize"> & {
    incidents(a: WorkContext): Promise<SupportRow[]>;
    tickets(a: WorkContext): Promise<SupportRow[]>;
}) { return async (a: WorkContext) => { ports.authorize(a, 'support.read'); const items = await ports.incidents(a), tickets = await ports.tickets(a); return { items, tickets, canManage: hasPermission(a, 'support.manage') }; }; }
export function createIncidentDetail(ports: {
    incident(a: WorkContext, id: string): Promise<SupportRow>;
    ticket(a: WorkContext, id: string): Promise<SupportRow>;
    links(id: string): Promise<SupportRow[]>;
    recipients(a: WorkContext, orderId: string): Promise<{
        id: string;
        name: string;
    }[]>;
    candidates(a: WorkContext, row: SupportRow, id: string): Promise<SupportRow[]>;
    history(id: string): Promise<SupportRow[]>;
}) { return async (a: WorkContext, id: string) => { const row = await ports.incident(a, id), links = await ports.links(id), first = await ports.ticket(a, String(links[0].id)); return { row, links, recipients: await ports.recipients(a, String(first.order_id)), candidates: await ports.candidates(a, row, id), history: await ports.history(id), canManage: hasPermission(a, 'support.manage'), canWork: hasPermission(a, 'support.work') && samePerson(a, row.owner_id) && Number(row.owner_eligible) > 0, canReview: hasPermission(a, 'support.manage') && Number(row.reviewer_eligible) > 0 }; }; }
export function createIncidentWork(ports: {
    incidents(a: WorkContext): Promise<SupportRow[]>;
}) { return async (a: WorkContext) => !hasPermission(a, 'support.read') ? [] : incidentWorkItems(a, await ports.incidents(a)); }
