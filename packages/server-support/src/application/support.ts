import type { WorkContext } from "@erpx/server-identity_access/public/authorization";
import type { SupportRow, SupportMutation, SupportCommand, SupportCommandPorts } from "@erpx/server-support/application/command";
import { hasPermission } from "@erpx/feature-identity_access/lib/permissions";
import { samePerson } from "@erpx/feature-identity_access/lib/iam";
import type { AuthorizedSourceRecord } from "@erpx/server-records/public/source-record-policy";
import { supportClosed, supportWorkItems } from "@erpx/server-support/domain/support";
import { type SupportClock } from "@erpx/server-support/domain/support-clock";
function supportRead(ports: Pick<SupportCommandPorts, "authorize">, a: WorkContext) { ports.authorize(a, hasPermission(a, "support.read") ? "support.read" : "records.read"); }
export interface SupportReadPorts {
    authorize: SupportCommandPorts["authorize"];
    error: SupportCommandPorts["error"];
    ticket(a: WorkContext, id: string): Promise<SupportRow | null>;
    sourceVisible(a: WorkContext, row: SupportRow): Promise<boolean>;
}
export function createGetSupportTicket(ports: SupportReadPorts) { return async (a: WorkContext, id: string) => { supportRead(ports, a); const r = await ports.ticket(a, id); if (!r)
    throw ports.error(hasPermission(a, 'support.read') ? 404 : 403, 'SUPPORT_TICKET_NOT_FOUND', 'Ticket not found in this scope.'); if (!await ports.sourceVisible(a, r))
    throw ports.error(404, 'SUPPORT_TICKET_NOT_FOUND', 'Ticket source is unavailable in this scope.'); return r; }; }
export function createSupportWorkspace(ports: Pick<SupportCommandPorts, "authorize"> & {
    tickets(a: WorkContext): Promise<SupportRow[]>;
    sourceVisible(a: WorkContext, r: SupportRow): Promise<boolean>;
    orderSources(a: WorkContext): Promise<SupportRow[]>;
}) { return async (a: WorkContext) => { supportRead(ports, a); const rows = await ports.tickets(a), items: SupportRow[] = []; for (const row of rows)
    if (await ports.sourceVisible(a, row))
        items.push(row); const sources = hasPermission(a, 'support.create') && hasPermission(a, 'commerce.read') ? await ports.orderSources(a) : []; return { items, sources, canCreate: hasPermission(a, 'records.write'), canCreateOrder: hasPermission(a, 'support.create') && hasPermission(a, 'commerce.read'), ownReportsOnly: !hasPermission(a, 'support.read') }; }; }
export function createSupportDetail(ports: {
    ticket(a: WorkContext, id: string): Promise<SupportRow>;
    now(): number;
    incidents(id: string): Promise<SupportRow[]>;
    recipients(a: WorkContext, id: string): Promise<{
        id: string;
        name: string;
    }[]>;
    history(id: string): Promise<SupportRow[]>;
    notes(a: WorkContext, orderId: string | number): Promise<SupportRow[]>;
    policies(companyId: string): Promise<{
        id: string;
        name: string;
        version: number;
    }[]>;
}) { return async (a: WorkContext, id: string) => { const row = await ports.ticket(a, id); return { incidents: hasPermission(a, 'support.read') ? await ports.incidents(id) : [], now: ports.now(), row, recipients: hasPermission(a, 'support.read') ? await ports.recipients(a, id) : [], history: await ports.history(id), notes: hasPermission(a, 'support.manage') && row.order_id ? await ports.notes(a, row.order_id) : [], canWork: hasPermission(a, 'support.work') && row.owner_id === a.userId && Number(row.owner_eligible) > 0, canManage: hasPermission(a, 'support.manage'), canReport: row.source_kind !== 'sales_order' && row.created_by === a.userId && hasPermission(a, 'records.write'), canConfirm: hasPermission(a, 'support.manage') && !samePerson(a, row.owner_id) && !samePerson(a, row.resolved_by), canReadOrder: !!row.order_id && hasPermission(a, 'commerce.read'), slaClock: row.sla_clock_json ? JSON.parse(String(row.sla_clock_json)) as SupportClock : null, slaPolicies: hasPermission(a, 'support.manage') && row.status === 'open' && !row.sla_clock_json ? await ports.policies(a.companyId) : [] }; }; }
export function createSupportWork(ports: {
    tickets(a: WorkContext): Promise<SupportRow[]>;
    sourceVisible(a: WorkContext, row: SupportRow): Promise<boolean>;
}) { return async (a: WorkContext) => { if (!hasPermission(a, 'records.read') && !hasPermission(a, 'support.read'))
    return []; const candidates = await ports.tickets(a), rows: SupportRow[] = []; for (const row of candidates)
    if (await ports.sourceVisible(a, row))
        rows.push(row); return supportWorkItems(a, rows); }; }
export function createSupportRecipients(ports: Pick<SupportCommandPorts, "authorize"> & {
    recipients(a: WorkContext, id: string): Promise<{
        id: string;
        name: string;
    }[]>;
}) { return async (a: WorkContext, id: string) => { ports.authorize(a, 'support.read'); return ports.recipients(a, id); }; }
export function createSupportTicketSourceGuards(ports: {
    ticket(a: WorkContext, id: string): Promise<SupportRow>;
    current(a: WorkContext, id: string, version: number, key: string, step: string): Promise<SupportMutation>;
    source(a: WorkContext, input: {
        kind: AuthorizedSourceRecord["kind"];
        id: string;
    }): Promise<AuthorizedSourceRecord>;
    origin(a: WorkContext, key: string, source: AuthorizedSourceRecord, step: string): Promise<SupportMutation>;
    combine(current: SupportMutation, origin: SupportMutation): SupportMutation;
}) { return async (a: WorkContext, key: string, id: string, version: number, step: string) => { const row = await ports.ticket(a, id), current = await ports.current(a, id, version, key, step); if (!row.source_record_json)
    return current; const original = JSON.parse(String(row.source_record_json)), source = await ports.source(a, { kind: original.kind, id: original.id }); return ports.combine(current, await ports.origin(a, key, source, `${step}-origin`)); }; }
