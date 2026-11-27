import { moduleRegistry } from '@erpx/catalogue/module-registry';
import { hasPermission, permissionCatalog } from '@erpx/feature-identity_access/lib/permissions';
import type { Filters, MetricInfo, MetricData, MetricId, Breakdown } from '@erpx/server-runtime/public/rules/canvas-domain';
import type { WorkContext } from '@erpx/server-identity_access/public/authorization';
import type { Clock } from '@erpx/server-shared/index';

export interface ExecutiveContextPorts {
    clock: Clock;
    currentAuthority(actor: WorkContext): Promise<void>;
    catalog(actor: WorkContext): Promise<MetricInfo[]>;
    metric(actor: WorkContext, id: MetricId, filters: Filters, breakdown: Breakdown): Promise<MetricData>;
    sourceUnavailable?(error: unknown): boolean;
    nodes(actor: WorkContext): Promise<Record<string, unknown>[]>;
    positions(actor: WorkContext): Promise<Record<string, unknown>[]>;
}

export function createExecutiveContextApplication(ports: ExecutiveContextPorts) {
    return {
        async executiveAiContext(actor: WorkContext, filters: Filters, selection?: { question: string; limit: number }) {
            await ports.currentAuthority(actor);
            const available = (await ports.catalog(actor)).filter(metric => metric.available);
            const tokens = new Set(selection?.question.toLowerCase().match(/[a-z0-9]{3,}/g) ?? []);
            const relevance = (metric: MetricInfo) => [...tokens].filter(token =>
                `${metric.id} ${metric.label.toLowerCase()}`.includes(token)).length;
            const allowed = selection
                ? available.sort((left, right) => relevance(right) - relevance(left)).slice(0, selection.limit)
                : available;
            const allowedModules = new Set(permissionCatalog
                .filter(permission => hasPermission(actor, permission.id) && (/\.(read|reports)$/.test(permission.id) || permission.id.startsWith('self.')))
                .map(permission => permission.module));
            const modules = moduleRegistry.filter(module => allowedModules.has(module.id))
                .map(module => ({ id: module.id, name: module.name, description: module.description }));
            const catalog: MetricInfo[] = [];
            const unavailableMetrics: MetricId[] = [];
            const snapshots = [];
            for (const metric of allowed) {
                const readings = [];
                const breakdowns: Breakdown[] = [];
                const supportedBreakdowns = metric.breakdowns ?? ['default'];
                const selectedBreakdowns = selection && supportedBreakdowns.includes('default')
                    ? ['default'] as const : supportedBreakdowns;
                for (const breakdown of selectedBreakdowns) {
                    let data: MetricData;
                    try {
                        data = await ports.metric(actor, metric.id, filters, breakdown);
                    } catch (error) {
                        if (!ports.sourceUnavailable?.(error)) throw error;
                        continue;
                    }
                    if (data.status === 'unavailable' || data.status === 'failed') continue;
                    breakdowns.push(breakdown);
                    readings.push({
                        metric: metric.id, breakdown, total: data.total, unit: data.unit,
                        currency: data.currency, definition: data.definition, scope: data.scope,
                        rows: data.rows.slice(0, 20), rowsTruncated: data.rows.length > 20, revision: data.revision,
                    });
                }
                if (!readings.length) {
                    unavailableMetrics.push(metric.id);
                    continue;
                }
                catalog.push({ ...metric, breakdowns });
                snapshots.push(...readings);
            }
            let organization: unknown = { available: false, reason: 'The selected role cannot read organization and people data.' };
            if (!selection && hasPermission(actor, 'people.read')) {
                try {
                    const nodes = await ports.nodes(actor), positions = await ports.positions(actor);
                    const ids = new Set(positions.slice(0, 500).map(position => position.id));
                    organization = {
                        available: true,
                        nodes: nodes.slice(0, 500),
                        positions: positions.slice(0, 500).map(position => ({
                            ...position,
                            reports_to_position_id: ids.has(position.reports_to_position_id) ? position.reports_to_position_id : null,
                        })),
                        truncated: nodes.length > 500 || positions.length > 500,
                    };
                } catch (error) {
                    if (!ports.sourceUnavailable?.(error)) throw error;
                    organization = { available: false, reason: 'The organization source could not be read.' };
                }
            } else if (selection) organization = { available: false, reason: 'This proposal uses a focused set of metric sources.' };
            await ports.currentAuthority(actor);
            return {
                scope: {
                    companyId: actor.companyId, branchId: actor.branchId, departmentId: actor.departmentId,
                    role: actor.role, roleRevision: actor.iam?.roleRevision ?? null,
                },
                filters, catalog, modules, organization, snapshots, unavailableMetrics, capturedAt: ports.clock.now(),
                coverage: "Authorized metric adapters available in the selected company and work scope. Module descriptions are structural context. Never infer missing payroll, profit, occupancy, forecasts or another company's data.",
            };
        },
    };
}
