import type { FlowKind } from "@erpx/feature-shared/lib/flow";
import type { WorkContext } from "@erpx/server-identity_access/public/authorization";
import type { companyAuthority } from "@erpx/server-operations/public/staff-policy";
import type { resolveFlow } from "@erpx/server-workflow/public/flow-routing";
import { routingQuerySchema } from '@erpx/server-workflow/domain/routing-attention';
import { BusinessRuleError, type Clock, type IdentifierSource } from '@erpx/server-shared/index';
import type { OwnerCommands } from '@erpx/server-workflow/domain/owner-command-ports';
type Tail<Value extends unknown[]> = Value extends [
    unknown,
    ...infer Rest
] ? Rest : never;
type PortValue<Value, Effect> = Value extends {
    bind(...args: never[]): unknown;
    run(): unknown;
} ? Effect : Value extends {
    guard: (...args: infer Args) => unknown;
} ? Omit<Value, 'guard'> & {
    guard(...args: Args): Effect;
} : Value extends readonly {
    bind(...args: never[]): unknown;
    run(): unknown;
}[] ? Effect[] : Value;
type PortResult<Value, Effect> = Value extends Promise<infer Result> ? Promise<PortValue<Result, Effect>> : PortValue<Value, Effect>;
export interface RoutingAttentionRepository {
    readRoutingAttentionRows(companyId: string, after: string, query: {
        kind: "expense" | "leave" | "staff_task" | "request" | "stock_count" | "transfer" | "order" | "invoice" | "journal" | "payroll" | "record";
        after: string;
        limit: number;
    }): Promise<{
        results: (Record<string, string | number | null>)[];
    }>;
}
export interface RoutingAttentionPorts<Effect> {
    repository: RoutingAttentionRepository;
    clock: Clock;
    identifiers: IdentifierSource;
    commands: OwnerCommands<Effect>;
    prepare(holdingId: string): Promise<void>;
    production: boolean;
    batch(effects: Effect[]): Promise<unknown>;
    policyRead<Result>(key: string, read: () => Promise<Result>): Promise<Result>;
    policyNow(): number;
    companyAuthority(...args: Tail<Parameters<typeof companyAuthority>>): PortResult<ReturnType<typeof companyAuthority>, Effect>;
    requireFlowAccess(actor: WorkContext, kind: FlowKind, id: string): Promise<void>;
    resolveFlow(...args: Tail<Parameters<typeof resolveFlow>>): PortResult<ReturnType<typeof resolveFlow>, Effect>;
}
export function createRoutingAttentionApplication<Effect>(ports: RoutingAttentionPorts<Effect>) {
    const r = ports.repository;
    async function requireRoutingManager(actor: WorkContext) {
        await ports.prepare(actor.holdingId);
        if (actor.branchId || actor.departmentId || !await ports.companyAuthority(actor))
            throw new BusinessRuleError(403, 'ROUTING_MANAGER_REQUIRED', 'Use an active Company organization-management assignment to inspect routing.');
    }
    async function routingAttention(actor: WorkContext, input: unknown) {
        const query = routingQuerySchema.parse(input);
        await requireRoutingManager(actor);
        const items: Array<{
            id: string;
            kind: FlowKind;
            title: string;
            state: string;
            blocker: string;
            action: string | null;
            team: string | null;
            recordVersion: number;
            routeVersion: number;
        }> = [];
        let after = query.after, exhausted = false;
        while (!exhausted && items.length <= query.limit) {
            const rows = (await r.readRoutingAttentionRows(actor.companyId, after, query)).results;
            if (!rows.length)
                break;
            for (const row of rows) {
                after = String(row.id);
                if (row.archived_at)
                    continue;
                try {
                    await ports.requireFlowAccess(actor, query.kind, after);
                }
                catch (error) {
                    if (error instanceof BusinessRuleError && [403, 404].includes(error.status))
                        continue;
                    throw error;
                }
                // Expense payables are routed by the expense owner, not the generic invoice workflow.
                if (query.kind === 'invoice' && row.source_type === 'expense_claim')
                    continue;
                const flow = await ports.resolveFlow(actor, query.kind, after);
                if (!flow.canManage || !flow.blocker)
                    continue;
                items.push({ id: after, kind: query.kind, title: String(row.title || row.number || row.invoice_number || row.journal_number || row.merchant || row.pay_date || after), state: flow.state, blocker: flow.blocker, action: flow.current?.action ?? null, team: flow.current?.teamName ?? null, recordVersion: flow.recordVersion, routeVersion: flow.version });
                if (items.length > query.limit)
                    break;
            }
            exhausted = rows.length < 100;
        }
        const more = items.length > query.limit, visible = items.slice(0, query.limit);
        return { kind: query.kind, items: visible, nextAfter: more ? visible.at(-1)!.id : null, checkedAt: ports.clock.now(), coverage: 'named_flow' as const };
    }
    return { requireRoutingManager, routingAttention };
}
