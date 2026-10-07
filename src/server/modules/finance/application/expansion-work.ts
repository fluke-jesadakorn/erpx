import type { WorkItem } from '@/lib/work-item';
import { hasPermission } from '@/lib/permissions';
import { samePerson } from '@/lib/iam';
import type { WorkContext } from '@/server/modules/identity_access/public/authorization';
export type ExpansionWorkRow = {
    id: string;
    title: string;
    kind: string;
    chart_id: string | null;
    status: string;
    prepared_by: string;
    updated_at: number;
};
export type DueWorkRow = {
    id: string;
    kind: string;
    run_on: string;
    status: string;
    last_message: string | null;
    updated_at: number;
};
export type CloseWorkPeriod = {
    id: string;
    name: string;
    start_date: string;
    end_date: string;
};
export interface FinanceExpansionWorkPorts {
    permission(actor: WorkContext, permission: 'finance.read'): WorkContext;
    ready(actor: WorkContext): Promise<void>;
    today(): string;
    rows(actor: WorkContext): Promise<ExpansionWorkRow[]>;
    due(actor: WorkContext): Promise<DueWorkRow[]>;
    periods(actor: WorkContext, today: string): Promise<CloseWorkPeriod[]>;
    pending(companyId: string, start: string, end: string): Promise<{
        kind: string;
        id: string;
    }[]>;
}
export function createFinanceExpansionWork(ports: FinanceExpansionWorkPorts) {
    async function financeExpansionWork(actor: WorkContext): Promise<WorkItem[]> {
        actor = ports.permission(actor, "finance.read");
        await ports.ready(actor);
        const rows = await ports.rows(actor);
        const due = actor.scope.kind === "holding" ? (await ports.due(actor)).map((r): WorkItem => ({ id: r.id, type: "finance_review", title: `Scheduled ${r.kind.replaceAll('_', ' ')} draft`, detail: r.last_message ?? `Prepare on ${r.run_on} with retained source evidence`, href: "/finance/due-drafts", status: r.status, updatedAt: r.updated_at, queue: r.status === 'blocked' && hasPermission(actor, 'finance.write') ? 'todo' : 'waiting', canAct: r.status === 'blocked' && hasPermission(actor, 'finance.write'), actionKind: "work", nextAction: r.status === 'blocked' ? 'Resolve the source issue and retry' : 'Waiting for the scheduled date', responsibleQueue: "Company Finance", relationships: ["team"] })) : [];
        const today = ports.today(), periods = actor.scope.kind === 'holding' ? await ports.periods(actor, today) : [], omitted: WorkItem[] = [];
        for (const period of periods) {
            const missing = await ports.pending(actor.companyId, period.start_date, period.end_date);
            for (const r of missing.filter(r => r.kind.startsWith('due '))) {
                const href = r.kind.includes('asset') ? '/finance/asset-policies' : r.kind.includes('financing') ? '/finance/instruments' : '/finance/contracts', canAct = hasPermission(actor, 'finance.write');
                omitted.push({ id: `finance-due-source:${period.id}:${r.kind}:${r.id}`, type: 'finance_review', title: `Prepare ${r.kind.slice(4)}`, detail: `${period.name}: review the source evidence and prepare the missing draft before closing this period.`, href, status: 'source_missing', updatedAt: Date.parse(period.end_date), queue: canAct ? 'todo' : 'waiting', canAct, actionKind: 'work', nextAction: 'Prepare the source draft', responsibleQueue: 'Company Finance', relationships: ['team'] });
            }
        }
        return [...omitted, ...due, ...rows.filter((r) => actor.scope.kind === "holding" || !["group-edition", "ownership", "acquisition", "intercompany-pair"].includes(r.kind)).map((r) => {
                const self = samePerson(actor, r.prepared_by), delivery = r.kind === "collection" && r.status === "approved", canAct = [
                    "group-edition",
                    "ownership",
                    "acquisition",
                    "intercompany-pair",
                ].includes(r.kind) && actor.scope.kind !== "holding"
                    ? false
                    : delivery
                        ? hasPermission(actor, "finance.write")
                        : !self && hasPermission(actor, "finance.approve"), href = [
                    "group-edition",
                    "ownership",
                    "acquisition",
                    "intercompany-pair",
                ].includes(r.kind)
                    ? "/finance/group-editions"
                    : r.kind.startsWith("instrument")
                        ? "/finance/instruments"
                        : r.kind.startsWith("asset-policy")
                            ? "/finance/asset-policies"
                            : r.kind === "book-adjustment"
                                ? "/finance/book-adjustments"
                                : ["contract", "recognition", "zero-usage", "billing-correction"].includes(r.kind)
                                    ? "/finance/contracts"
                                    : r.kind === "book"
                                        ? "/finance/setup?step=books"
                                        : r.kind === "bank"
                                            ? "/finance/bank"
                                            : r.kind === "collection"
                                                ? "/finance/collections"
                                                : `/finance/reports?chart=${encodeURIComponent(r.chart_id ?? "")}`;
                return {
                    id: r.id,
                    type: "finance_review",
                    title: r.title,
                    detail: delivery
                        ? "Reviewed message ready for test delivery"
                        : `Review ${r.kind} source evidence before approval`,
                    href,
                    status: r.status,
                    updatedAt: r.updated_at,
                    queue: canAct ? "todo" : "waiting",
                    canAct,
                    actionKind: delivery ? "work" : "approval",
                    nextAction: delivery
                        ? "Record test delivery"
                        : self
                            ? "Waiting for an independent Finance approver"
                            : "Review and approve",
                    responsibleQueue: "Company Finance",
                    relationships: self
                        ? ["requested", "team"]
                        : canAct
                            ? ["approval", "team"]
                            : ["team"],
                } satisfies WorkItem;
            })];
    }
    return financeExpansionWork;
}
