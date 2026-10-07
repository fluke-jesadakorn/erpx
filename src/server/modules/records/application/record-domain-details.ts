import type { WorkContext } from "@/server/modules/identity_access/public/authorization";
import type { brandIdentityWorkspace } from "@/server/modules/brand/public/brand-identity";
import type { agentWorkspace } from "@/server/modules/agent/public/agent-governance";
import type { productionWorkspace } from "@/server/modules/manufacturing/public/production";
import type { ecommerceWorkspace } from "@/server/modules/ecommerce/public/ecommerce";
import type { learningWorkspace } from "@/server/modules/learning/public/learning";
import type { benefitsWorkspace } from "@/server/modules/benefits/public/benefits";
import type { talentWorkspace } from "@/server/modules/talent/public/talent";
import { type DomainRecordKind, isDomainRecordKind } from '../domain/record-domain-details';
import { BusinessRuleError, type Clock, type IdentifierSource } from '../../shared';
import type { RecordsCommands } from '../domain/commands';
type PortResult<Value, Effect> = Value extends Promise<infer Result> ? Promise<PortResult<Result, Effect>> : Value extends {
    bind(...args: never[]): unknown;
    run(): unknown;
} ? Effect : Value extends (...args: infer Args) => infer Result ? (...args: Args) => PortResult<Result, Effect> : Value extends readonly unknown[] ? {
    [Index in keyof Value]: PortResult<Value[Index], Effect>;
} : Value extends object ? {
    [Key in keyof Value]: PortResult<Value[Key], Effect>;
} : Value;
export type RecordDomainDetailsRepository<Effect> = Record<never, Effect>;
export interface RecordDomainDetailsPorts<Effect> {
    repository: RecordDomainDetailsRepository<Effect>;
    clock: Clock;
    identifiers: IdentifierSource;
    commands: RecordsCommands<Effect>;
    prepare(holdingId: string): Promise<void>;
    production: boolean;
    batch(effects: Effect[]): Promise<unknown>;
    policyRead<Result>(key: string, read: () => Promise<Result>): Promise<Result>;
    policyNow(): number;
    brandIdentityWorkspace(...args: Parameters<typeof brandIdentityWorkspace>): PortResult<ReturnType<typeof brandIdentityWorkspace>, Effect>;
    agentWorkspace(...args: Parameters<typeof agentWorkspace>): PortResult<ReturnType<typeof agentWorkspace>, Effect>;
    productionWorkspace(...args: Parameters<typeof productionWorkspace>): PortResult<ReturnType<typeof productionWorkspace>, Effect>;
    ecommerceWorkspace(...args: Parameters<typeof ecommerceWorkspace>): PortResult<ReturnType<typeof ecommerceWorkspace>, Effect>;
    learningWorkspace(...args: Parameters<typeof learningWorkspace>): PortResult<ReturnType<typeof learningWorkspace>, Effect>;
    benefitsWorkspace(...args: Parameters<typeof benefitsWorkspace>): PortResult<ReturnType<typeof benefitsWorkspace>, Effect>;
    talentWorkspace(...args: Parameters<typeof talentWorkspace>): PortResult<ReturnType<typeof talentWorkspace>, Effect>;
}
export function createRecordDomainDetailsApplication<Effect>(ports: RecordDomainDetailsPorts<Effect>) {
    function found<T>(row: T | undefined): T {
        if (!row)
            throw new BusinessRuleError(404, 'RECORD_NOT_FOUND', 'Record not found in this work scope.');
        return row;
    }
    async function recordDomainData(actor: WorkContext, kind: DomainRecordKind, id: string) {
        if (kind === 'brand_draft' || kind === 'brand_version') {
            const data = await ports.brandIdentityWorkspace(actor), row = found(kind === 'brand_draft' ? data.drafts.find(r => r.id === id) : data.versions.find(r => r.id === id));
            return { domain: 'brand' as const, kind, row, data: { ...data, drafts: data.drafts.filter(r => r.id === id), versions: data.versions.filter(r => r.id === id) } };
        }
        if (kind === 'agent_pool') {
            const data = await ports.agentWorkspace(actor), row = found(data.pools.find(pool => pool.id === id));
            return { domain: 'agents' as const, kind, row, data: { ...data, agents: [], versions: [], pools: [row], assignments: [], invocations: [], sessions: [], members: data.members.filter(member => member.pool_id === id), poolHistory: data.poolHistory.filter(revision => revision.pool_id === id) } };
        }
        if (kind === 'agent_version' || kind === 'agent_assignment') {
            const data = await ports.agentWorkspace(actor), row = found((kind === 'agent_version' ? data.versions : data.assignments).find(r => r.id === id));
            return { domain: 'agents' as const, kind, row, data: { ...data, agents: data.agents.filter(agent => agent.id === row.agent_id || data.versions.some(version => version.id === row.agent_version_id && version.agent_id === agent.id)), versions: data.versions.filter(r => r.id === id || r.id === row.agent_version_id), assignments: data.assignments.filter(r => r.id === id), pools: [], invocations: [], sessions: [] } };
        }
        if (kind === 'manufacturing_spec' || kind === 'planning_scenario') {
            const productionModule: 'planning' | 'manufacturing' = kind === 'planning_scenario' ? 'planning' : 'manufacturing', data = await ports.productionWorkspace(actor, productionModule), row = found((kind === 'planning_scenario' ? data.plans : data.specifications).find(r => r.id === id));
            return { domain: 'production' as const, kind, module: productionModule, row, data: { ...data, specifications: data.specifications.filter(r => r.id === id || r.id === row.specification_id), plans: data.plans.filter(r => r.id === id), recommendations: data.recommendations.filter(r => r.scenario_id === id), orders: [] } };
        }
        if (kind === 'ecommerce_listing' || kind === 'ecommerce_cart' || kind === 'ecommerce_channel') {
            const data = await ports.ecommerceWorkspace(actor), row = found((kind === 'ecommerce_listing' ? data.listings : kind === 'ecommerce_cart' ? data.carts : data.channels).find(r => r.id === id));
            return { domain: 'ecommerce' as const, kind, row, data: { ...data, listings: data.listings.filter(r => r.id === id), carts: data.carts.filter(r => r.id === id), channels: data.channels.filter(r => r.id === id || r.id === row.channel_id), checkouts: [] } };
        }
        if (kind === 'learning_course') {
            const data = await ports.learningWorkspace(actor), row = found(data.courses.find(r => r.id === id));
            return { domain: 'learning' as const, kind, row, data: { ...data, courses: [row], assignments: [] } };
        }
        if (kind === 'benefit_plan') {
            const data = await ports.benefitsWorkspace(actor), row = found(data.plans.find(r => r.id === id));
            return { domain: 'benefits' as const, kind, row, data: { ...data, plans: [row], elections: [] } };
        }
        const data = await ports.talentWorkspace(actor), row = found((kind === 'talent_requisition' ? data.requisitions : data.candidates).find(r => r.id === id));
        return { domain: 'talent' as const, kind, row, data: { ...data, requisitions: data.requisitions.filter(r => r.id === id), candidates: data.candidates.filter(r => r.id === id), applications: [] } };
    }
    return { isDomainRecordKind, recordDomainData };
}
