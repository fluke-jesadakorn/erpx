import { z } from 'zod';
import type { WorkContext } from '@erpx/server-identity_access/public/authorization';
import type { FinanceError } from '@erpx/server-finance/domain/invoice-line-revision';
import { financeStarterSchema, starterDates, starterConfiguration, type StarterInput, type StarterPlan, type StarterNode, type StarterCandidateFacts } from '@erpx/server-finance/domain/starter';
export interface FinanceStarterPorts {
    error: FinanceError;
    permission(actor: WorkContext, permission: 'finance.write'): WorkContext;
    ready(actor: WorkContext): Promise<void>;
    transaction<Result>(work: () => Promise<Result>): Promise<Result>;
    now(): number;
    uniqueConflict(error: unknown): boolean;
    replay<Result=Record<string,unknown>>(actor: WorkContext, key: string, op: string, id: string, input: unknown): Promise<Result|null>;
    requireMoneyScale(actor: WorkContext): Promise<void>;
    company(actor: WorkContext): Promise<unknown | null>;
    existingChart(actor: WorkContext): Promise<boolean>;
    overlappingPeriod(actor: WorkContext, start: string, end: string): Promise<boolean>;
    organizationNodes(actor: WorkContext): Promise<StarterNode[]>;
    candidates(companyId: string, now: number): Promise<StarterCandidateFacts>;
    independent(companyId: string, preparers: string[], approvers: string[]): Promise<boolean>;
    commit(facts: {
        actor: WorkContext;
        input: StarterInput;
        key: string;
        id: string;
        op: string;
        plan: StarterPlan;
        now: number;
    }): Promise<Record<string,unknown>>;
}
export function createFinanceStarterService(ports: FinanceStarterPorts) {
    async function financeSetupAuthority(companyId: string) { const { rows, permissions } = await ports.candidates(companyId, ports.now()), roles = new Map(permissions.map(r => [r.role, r.permissions])), preparers = [...new Set(rows.filter(r => roles.get(r.role)?.includes('finance.write')).map(r => r.user_id))], approvers = [...new Set(rows.filter(r => roles.get(r.role)?.includes('finance.approve')).map(r => r.user_id))]; return { preparerCount: preparers.length, approverCount: approvers.length, independent: await ports.independent(companyId, preparers, approvers) }; }
    async function createFinanceStarter(actor: WorkContext, raw: z.input<typeof financeStarterSchema>, key: string) { actor = ports.permission(actor, 'finance.write'); if (actor.scope.kind !== 'holding')
        throw ports.error(403, 'COMPANY_FINANCE_REQUIRED', 'Use a Company Finance assignment for initial ledger configuration.'); const input = financeStarterSchema.parse(raw), id = `starter-chart-${actor.companyId}`, op = 'finance.starter.create'; await ports.ready(actor); try {
        return await ports.transaction(async () => { const replay = await ports.replay<Record<string,unknown>>(actor, key, op, id, input); if (replay)
            return replay; await ports.requireMoneyScale(actor); if (!await ports.company(actor))
            throw ports.error(404, 'COMPANY_NOT_FOUND', 'Choose an active Company.'); if (await ports.existingChart(actor))
            throw ports.error(409, 'STARTER_ALREADY_CONFIGURED', 'This Company already has a chart. Review its existing configuration or use the account import workflow.'); const dates = starterDates(input.firstMonth); if (await ports.overlappingPeriod(actor, dates.start, dates.end))
            throw ports.error(409, 'STARTER_PERIOD_OVERLAP', 'Review the existing accounting period before creating the starter configuration.'); const nodes = input.organizationDimensions ? await ports.organizationNodes(actor) : [], plan = starterConfiguration(input, actor.companyId, id, nodes); return ports.commit({ actor, input, key, id, op, plan, now: ports.now() }); });
    }
    catch (error) {
        if (ports.uniqueConflict(error)) {
            const replay = await ports.replay(actor, key, op, id, input);
            if (replay)
                return replay;
            throw ports.error(409, 'STARTER_CONFIGURATION_CONFLICT', 'Another configuration already uses these settings. Refresh the Company setup; keep existing dimensions or turn off their starter import.');
        }
        throw error;
    } }
    return { financeSetupAuthority, createFinanceStarter };
}

