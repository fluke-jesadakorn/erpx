import type { WorkContext } from "@erpx/server-identity_access/public/authorization";
import { hasPermission } from "@erpx/feature-identity_access/lib/permissions";
import { samePerson } from "@erpx/feature-identity_access/lib/iam";
import type { SupportCommandPorts, SupportRow } from "@erpx/server-support/application/command";
import { incidentWorkItems } from "@erpx/server-support/domain/support-incidents";
export function createGetSupportIncident(ports: Pick<SupportCommandPorts, "authorize" | "error"> & {
    incident(a: WorkContext, id: string): Promise<SupportRow | null>;
}) { return async (a: WorkContext, id: string) => { ports.authorize(a, 'support.read'); const r = await ports.incident(a, id); if (!r)
    throw ports.error(404, 'SUPPORT_INCIDENT_NOT_FOUND', 'Incident is not available in this scope.'); return r; }; }
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
