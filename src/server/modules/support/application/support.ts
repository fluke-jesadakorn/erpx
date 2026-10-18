import type { WorkContext } from "@/server/modules/identity_access/public/authorization";
import type { SupportRow, SupportMutation, SupportCommand, SupportCommandPorts } from "./command";
import { hasPermission } from "@/lib/identity_access/permissions";
import { samePerson } from "@/lib/identity_access/iam";
import type { AuthorizedSourceRecord } from "@/server/modules/records/public/source-record-policy";
import { supportCreateSchema, supportChangeSchema, supportClosed, supportWorkItems, type SupportCreate, type SupportChange } from "../domain/support";
import { advanceSupportClock, type SupportClock } from "../domain/support-clock";
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
export function createSupportTicket(ports: SupportCommandPorts & {
    ticket(a: WorkContext, id: string): Promise<SupportRow>;
    source(a: WorkContext, input: Extract<SupportCreate, {
        kind: "source";
    }>["source"]): Promise<AuthorizedSourceRecord>;
    report(command: SupportCommand, input: Extract<SupportCreate, {
        kind: string;
    }>, source: AuthorizedSourceRecord | null): Promise<SupportMutation>;
    order(command: SupportCommand, input: Extract<SupportCreate, {
        orderId: string;
    }>): Promise<SupportMutation>;
}) { return async (a: WorkContext, raw: unknown, key: string) => { const input = supportCreateSchema.parse(raw); if ('kind' in input)
    ports.authorize(a, 'records.write');
else {
    ports.authorize(a, 'support.create');
    ports.authorize(a, 'support.read');
    ports.authorize(a, 'commerce.read');
} const id = `ticket-${key}`, op = 'support.create', prior = await ports.replay(a, key, op, id, input); if (prior) {
    await ports.ticket(a, id);
    return prior;
} const now = ports.now(), source = 'kind' in input && input.kind === 'source' ? await ports.source(a, input.source) : null; const command: SupportCommand = { actor: a, key, operation: op, id, input, version: 1, result: 'kind' in input ? { id, version: 1, status: 'triage' } : { id, version: 1 }, now, ...('kind' in input ? { scope: { companyId: a.companyId, branchId: a.branchId, departmentId: a.departmentId } } : {}) }; return ports.commit(command, 'kind' in input ? await ports.report(command, input, source) : await ports.order(command, input)); }; }
export function createChangeSupportTicket(ports: SupportCommandPorts & {
    ticket(a: WorkContext, id: string): Promise<SupportRow>;
    report(command: SupportCommand, input: SupportChange, row: SupportRow): Promise<SupportMutation>;
    assigned(command: SupportCommand, input: SupportChange, row: SupportRow, manage: boolean, status: string, nextClock: SupportClock | null): Promise<SupportMutation>;
}) {
    return async (a: WorkContext, id: string, raw: unknown, key: string) => {
        const input = supportChangeSchema.parse(raw), reporting = ['edit-report', 'withdraw-report', 'add-information'].includes(input.action);
        supportRead(ports, a);
        const manage = ['triage', 'reassign', 'confirm', 'confirm-general', 'reject-resolution', 'reopen', 'cancel'].includes(input.action);
        ports.authorize(a, reporting ? 'records.write' : manage ? 'support.manage' : 'support.work');
        const op = `support.${input.action}`, prior = await ports.replay(a, key, op, id, input);
        if (prior) {
            await ports.ticket(a, id);
            return prior;
        }
        const r = await ports.ticket(a, id), now = ports.now();
        if (r.source_kind === 'field_service')
            throw ports.error(409, 'SOURCE_WORKSPACE_REQUIRED', 'Continue this case in Field Service.');
        if (reporting) {
            if (r.source_kind === 'sales_order' || r.created_by !== a.userId)
                throw ports.error(403, 'SUPPORT_REPORTER_REQUIRED', 'Only the original reporter can change their report.');
            if (supportClosed(r.status) || input.action === 'edit-report' && r.status !== 'triage')
                throw ports.error(409, 'SUPPORT_REPORT_CHANGED', 'This report is no longer editable. Add current information or ask Support to reopen it.');
            const command: SupportCommand = { actor: a, key, operation: op, id, input, now, version: input.expectedVersion + 1, result: { id, status: input.action === 'withdraw-report' ? 'cancelled' : r.status, version: input.expectedVersion + 1 } };
            return ports.commit(command, await ports.report(command, input, r));
        }
        const allowed: Record<string, string[]> = { triage: ['triage'], start: ['open'], 'wait-customer': ['in_progress'], resume: ['waiting_customer'], resolve: ['in_progress'], 'reject-resolution': ['resolved'], confirm: ['resolved'], 'confirm-general': ['resolved'], reopen: ['closed'], cancel: ['triage', 'open', 'in_progress', 'waiting_customer', 'resolved'], reassign: ['open', 'in_progress', 'waiting_customer', 'resolved'] };
        if (!allowed[input.action].includes(String(r.status)))
            throw ports.error(409, 'SUPPORT_STATE_CHANGED', 'Review the current ticket state.');
        if (input.action === 'confirm' && r.source_kind !== 'sales_order' || input.action === 'confirm-general' && r.source_kind === 'sales_order')
            throw ports.error(409, 'SUPPORT_CONFIRMATION_MISMATCH', 'Choose the confirmation method for this issue source.');
        if (!manage && r.owner_id !== a.userId)
            throw ports.error(403, 'SUPPORT_OWNER_REQUIRED', 'The assigned support colleague must perform this action.');
        if (['confirm', 'confirm-general'].includes(input.action) && (samePerson(a, r.owner_id) || samePerson(a, r.resolved_by)))
            throw ports.error(403, 'INDEPENDENT_CONFIRMATION_REQUIRED', 'Another support manager must review this resolution.');
        const status = ({ triage: 'open', start: 'in_progress', 'wait-customer': 'waiting_customer', resume: 'in_progress', resolve: 'resolved', 'reject-resolution': 'in_progress', confirm: 'closed', 'confirm-general': 'closed', reopen: 'in_progress', cancel: 'cancelled' } as Record<string, string>)[input.action] ?? String(r.status);
        const nextClock = r.sla_clock_json ? advanceSupportClock(JSON.parse(String(r.sla_clock_json)) as SupportClock, input.action, now) : null;
        const command: SupportCommand = { actor: a, key, operation: op, id, input, now, version: input.expectedVersion + 1, result: { id, status, version: input.expectedVersion + 1 }, scope: { companyId: a.companyId, branchId: r.branch_id ? String(r.branch_id) : null, departmentId: r.department_id ? String(r.department_id) : null } };
        return ports.commit(command, await ports.assigned(command, input, r, manage, status, nextClock));
    };
}
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
