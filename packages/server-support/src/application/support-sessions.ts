import type { WorkContext } from "@erpx/server-identity_access/public/authorization";
import { hasPermission } from "@erpx/feature-identity_access/lib/permissions";
import type { SupportCommandPorts, SupportRow } from "@erpx/server-support/application/command";
import { supportSessionProjection, supportSessionWorkItems, type SupportSession, type SupportSessionView, type SupportContextRow } from "@erpx/server-support/domain/support-sessions";
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
export function createSupportSessionContext(ports: Pick<SupportCommandPorts, "authorize" | "error" | "now"> & {
    context(a: WorkContext, id: string, now: () => number): Promise<SupportContextRow | null>;
    evidence(ticketId: string, sessionId: string): Promise<SupportRow[]>;
}) { return async (a: WorkContext, id: string) => { ports.authorize(a, 'support.work'); ports.authorize(a, 'support.read'); const base = await ports.context(a, id, ports.now); if (!base)
    throw ports.error(403, 'SUPPORT_CAPABILITY_REQUIRED', 'An active customer order context capability is required.'); return { ...base, evidence: await ports.evidence(base.ticket_id, id) }; }; }
export function createSupportSessionWork(ports: {
    workspace(a: WorkContext): Promise<SupportSessionWorkspace>;
}) { return async (a: WorkContext) => !hasPermission(a, 'support.read') ? [] : supportSessionWorkItems(a, (await ports.workspace(a)).rows); }
