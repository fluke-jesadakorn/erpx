import type { WorkContext } from "@erpx/server-identity_access/public/authorization";
import { hasPermission } from "@erpx/feature-identity_access/lib/permissions";
import type { SupportCommandPorts, SupportCommand, SupportMutation, SupportRow } from "@erpx/server-support/application/command";
import { supportSessionRequestSchema, supportSessionActionSchema, supportSessionProjection, supportSessionWorkItems, type SupportSession, type SupportSessionView, type SupportContextRow, type SupportSessionRequest, type SupportSessionAction } from "@erpx/server-support/domain/support-sessions";
export type SupportSessionWorkspace = {
    actorId: string;
    rows: SupportSessionView[];
    tickets: SupportRow[];
};
export function createSupportSessionWorkspace(ports: Pick<SupportCommandPorts, "authorize" | "now"> & {
    sessions(a: WorkContext): Promise<SupportSession[]>;
    tickets(a: WorkContext): Promise<SupportRow[]>;
}) { return async (a: WorkContext): Promise<SupportSessionWorkspace> => { ports.authorize(a, 'support.read'); const rows = await ports.sessions(a); return { actorId: a.userId, rows: supportSessionProjection(a, rows, ports.now), tickets: await ports.tickets(a) }; }; }
export function createSupportSessionDetail(ports: Pick<SupportCommandPorts, "error"> & {
    workspace(a: WorkContext): Promise<SupportSessionWorkspace>;
    capabilities(id: string): Promise<SupportRow[]>;
    history(id: string): Promise<SupportRow[]>;
    capabilityHistory(id: string): Promise<SupportRow[]>;
    context(a: WorkContext, id: string): Promise<Pick<SupportContextRow, "ticket_id" | "order_number" | "order_kind" | "order_state" | "due_date" | "currency_code"> & {
        evidence: SupportRow[];
    }>;
}) { return async (a: WorkContext, id: string) => { const data = await ports.workspace(a), row = data.rows.find(r => r.id === id); if (!row)
    throw ports.error(404, 'SUPPORT_SESSION_NOT_FOUND', 'Support session not found in this scope.'); const capabilities = await ports.capabilities(id), hasContext = capabilities.some(c => c.code === 'customer_order_context' && c.status === 'active'); return { row, capabilities, history: await ports.history(id), capabilityHistory: await ports.capabilityHistory(id), context: row.requested_for === a.userId && row.effective_status === 'approved' && hasContext ? await ports.context(a, id) : null }; }; }
export function createRequestSupportSession(ports: SupportCommandPorts & {
    prepare(command: SupportCommand, input: SupportSessionRequest): Promise<SupportMutation>;
}) { return async (a: WorkContext, raw: unknown, key: string) => { ports.authorize(a, 'support.work'); ports.authorize(a, 'support.read'); const input = supportSessionRequestSchema.parse(raw), now = ports.now(); if (input.requestedFor !== a.userId)
    throw ports.error(403, 'SUPPORT_SESSION_SELF_REQUEST_REQUIRED', 'A support colleague must request their own temporary session.'); if (input.expiresAt <= now || input.expiresAt > now + 8 * 60 * 60 * 1000)
    throw ports.error(422, 'SUPPORT_SESSION_EXPIRY_INVALID', 'Session expiry must be within the next eight hours.'); const id = `support-session-${key}`, op = 'support.session.request', prior = await ports.replay(a, key, op, id, input); if (prior)
    return prior; const command: SupportCommand = { actor: a, key, operation: op, id, input, now, version: 1, result: { id, version: 1, status: 'pending' } }; return ports.commit(command, await ports.prepare(command, input)); }; }
export interface SupportSessionActionPorts extends SupportCommandPorts {
    session(a: WorkContext, id: string): Promise<(SupportSession & {
        ticket_status: string;
    }) | null>;
    review(command: SupportCommand, input: Extract<SupportSessionAction, {
        action: "approve" | "reject" | "revoke";
    }>, row: SupportSession, status: "approved" | "rejected"): Promise<SupportMutation>;
    revoke(command: SupportCommand, input: Extract<SupportSessionAction, {
        action: "approve" | "reject" | "revoke";
    }>, row: SupportSession): Promise<SupportMutation>;
    grant(command: SupportCommand, input: Extract<SupportSessionAction, {
        action: "issue-capability";
    }>, row: SupportSession): Promise<SupportMutation>;
    revokeGrant(command: SupportCommand, input: Extract<SupportSessionAction, {
        action: "revoke-capability";
    }>, row: SupportSession): Promise<SupportMutation>;
}
export function createChangeSupportSession(ports: SupportSessionActionPorts) { return async (a: WorkContext, id: string, raw: unknown, key: string) => { ports.authorize(a, 'support.read'); const input = supportSessionActionSchema.parse(raw), op = `support.session.${input.action}`, prior = await ports.replay(a, key, op, id, input); if (prior)
    return prior; const row = await ports.session(a, id); if (!row)
    throw ports.error(404, 'SUPPORT_SESSION_NOT_FOUND', 'Support session not found in this scope.'); if (!hasPermission(a, 'support.manage'))
    throw ports.error(403, 'PERMISSION_REQUIRED', 'Support management permission is required.'); const command: SupportCommand = { actor: a, key, operation: op, id, input, now: ports.now(), version: input.expectedVersion + 1, result: { id, version: input.expectedVersion + 1 } }; const mutation = input.action === 'approve' || input.action === 'reject' ? await ports.review(command, input, row, input.action === 'approve' ? 'approved' : 'rejected') : input.action === 'revoke' ? await ports.revoke(command, input, row) : input.action === 'issue-capability' ? await ports.grant(command, input, row) : await ports.revokeGrant(command, input as Extract<SupportSessionAction, {
    action: 'revoke-capability';
}>, row); return ports.commit(command, mutation); }; }
export function createSupportSessionContext(ports: Pick<SupportCommandPorts, "authorize" | "error" | "now"> & {
    context(a: WorkContext, id: string, now: () => number): Promise<SupportContextRow | null>;
    evidence(ticketId: string, sessionId: string): Promise<SupportRow[]>;
}) { return async (a: WorkContext, id: string) => { ports.authorize(a, 'support.work'); ports.authorize(a, 'support.read'); const base = await ports.context(a, id, ports.now); if (!base)
    throw ports.error(403, 'SUPPORT_CAPABILITY_REQUIRED', 'An active customer order context capability is required.'); return { ...base, evidence: await ports.evidence(base.ticket_id, id) }; }; }
export function createSupportSessionWork(ports: {
    workspace(a: WorkContext): Promise<SupportSessionWorkspace>;
}) { return async (a: WorkContext) => !hasPermission(a, 'support.read') ? [] : supportSessionWorkItems(a, (await ports.workspace(a)).rows); }
