import { moduleRegistry, moduleAvailability } from "@/data/module-registry";
import { hasModuleAccess } from "@/data/module-access";
import { isReleaseModule, resolveFeatureState, type RolloutState } from "@/lib/shared/hospitality-rollout";
import { productionScope, legalProductionScope } from "@/data/production-scope";
import { changeSchema } from "../domain/hospitality-rollout";
import type { WorkContext } from "../../identity_access/public/authorization";
import type { Clock } from "../../shared/application/ports";
import type { z } from "zod";
export type PmsRolloutRow = {
    module_id: string;
    rollout_state: RolloutState;
    version: number;
    reason: string;
    changed_at: number;
};
export type PmsRolloutInput = z.infer<typeof changeSchema>;
export type PmsRolloutRelease = Awaited<ReturnType<typeof import("../../platform_administration/public/release-acceptance").companyReleaseStatus>>;
export interface PmsRolloutRepository {
    rollouts(companyId: string): Promise<PmsRolloutRow[]>;
    readiness(companyId: string): Promise<Record<string, boolean>>;
    release(companyId: string, id?: string, scope?: typeof productionScope.version | typeof legalProductionScope.version): Promise<PmsRolloutRelease>;
    engineering(companyId: string): Promise<boolean>;
    replay(actor: WorkContext, key: string, commandId: string, input: PmsRolloutInput): Promise<Record<string, unknown> | null>;
    commit(actor: WorkContext, key: string, input: PmsRolloutInput, commandId: string, version: number, now: number, release: PmsRolloutRelease | null, scope: typeof productionScope.version | typeof legalProductionScope.version): Promise<{
        id: string;
        state: PmsRolloutInput["state"];
        version: number;
    }>;
    enabled(companyId: string, modules?: string[]): Promise<{
        module_id: string;
    }[]>;
    active(companyId: string, moduleId: string): Promise<unknown>;
}
export function createPmsRollout(ports: {
    repository(actor: WorkContext): Promise<PmsRolloutRepository>;
    clock: Clock;
    require(actor: WorkContext, permission: string): void;
    error(status: number, code: string, message: string): Error;
    environment(): string | undefined;
    deployment(): string | undefined;
    inventoryAccepted(id: string): boolean;
}) {
    async function companyFeatureCatalogue(actor: WorkContext) {
        const write = await ports.repository(actor);
        const rows = await write.rollouts(actor.companyId);
        const rollout = new Map(rows.map((row) => [row.module_id, row]));
        const ready = await write.readiness(actor.companyId);
        const release = await write.release(actor.companyId);
        const legalRelease = await write.release(actor.companyId, undefined, legalProductionScope.version);
        const production = ports.environment() === 'production';
        const engineering = await write.engineering(actor.companyId);
        return moduleRegistry.map((module) => {
            const row = rollout.get(module.id);
            const accepted = (module.id === 'legal' ? legalRelease : release).state === 'passed';
            const setupChecked = module.id in ready;
            const setupReady = ready[module.id] ?? false;
            const state = engineering ? setupChecked && !setupReady ? 'Setup required' as const : 'Pilot' as const : resolveFeatureState({ moduleId: module.id, production, rollout: row?.rollout_state, setupReady, productionAccepted: accepted, businessAccepted: accepted });
            return {
                id: module.id, name: module.name, state,
                delivery: moduleAvailability(module),
                rollout: row?.rollout_state ?? 'off', version: row?.version ?? 0,
                setupReady, setupChecked, productionAccepted: accepted, businessAccepted: accepted,
                inventoryAccepted: ports.inventoryAccepted(module.id),
                scopeVersion: module.id === 'legal' ? legalProductionScope.version : productionScope.version,
                environment: engineering ? 'isolated-engineering' : production ? 'production' : 'development',
                permitted: hasModuleAccess(actor.modules, module.id),
                ...(row ? { reason: row.reason, changedAt: row.changed_at } : {}),
            };
        });
    }
    async function changeCompanyFeatureRollout(actor: WorkContext, raw: unknown, key: string) {
        ports.require(actor, 'organization.manage');
        if (actor.branchId || actor.departmentId)
            throw ports.error(403, 'FEATURE_COMPANY_REQUIRED', 'Manage rollout from the company workspace');
        const input = changeSchema.parse(raw);
        const write = await ports.repository(actor);
        const commandId = `feature:${actor.companyId}:${input.moduleId}`;
        const prior = await write.replay(actor, key, commandId, input);
        if (prior)
            return prior;
        if (!moduleRegistry.some((module) => module.id === input.moduleId))
            throw ports.error(404, 'FEATURE_NOT_FOUND', 'Feature not found');
        if (input.state !== 'off' && !isReleaseModule(input.moduleId))
            throw ports.error(409, 'FEATURE_FUTURE_GATE', 'This feature needs a separate approved release cut');
        const scope = input.moduleId === 'legal' ? legalProductionScope.version : productionScope.version;
        const needsAcceptance = input.state === 'available' || (input.moduleId === 'legal' && input.state === 'pilot' && ports.environment() === 'production');
        const release = needsAcceptance ? await write.release(actor.companyId, undefined, scope) : null;
        if (needsAcceptance && release?.state !== 'passed')
            throw ports.error(409, 'FEATURE_ACCEPTANCE_REQUIRED', 'All criteria for this Company, scope, build and configuration require independent release acceptance');
        const nextVersion = input.expectedVersion + 1;
        const now = ports.clock.now();
        return write.commit(actor, key, input, commandId, nextVersion, now, release, scope);
    }
    /** Request boundary for owner APIs; reads remain available for authorized history. */
    async function assertProductionFeatureRoute(db: PmsRolloutRepository, companyId: string, destination: string, method: string) {
        if (ports.environment() !== 'production' || ['GET', 'HEAD', 'OPTIONS'].includes(method))
            return;
        if (await db.engineering(companyId))
            return;
        const [root, second] = destination.split('/');
        // The isolated staging standalone runs with NODE_ENV=production. Payroll
        // acceptance exercises remain sandbox-only there; the private production
        // service keeps its separate ERPX_ENVIRONMENT=production prohibition.
        if (ports.deployment() === 'staging' && root === 'people' &&
            ['payroll', 'pay-groups', 'country-packs', 'payments'].includes(second))
            return;
        if (root === 'feature-catalogue')
            return;
        // Legal shares one command endpoint for configuration and operations. Its
        // domain guards gate operational commands while managers prepare acceptance.
        if (root === 'legal')
            return;
        if (destination === 'pos/sale/reverse_room_charge')
            return;
        if (root === 'finance' && ['engines', 'group'].includes(second))
            throw ports.error(403, 'FEATURE_COMING_SOON', 'Advanced Finance is not in this hotel pilot');
        if (root === 'people' && ['payroll', 'pay-groups', 'country-packs', 'payments'].includes(second))
            throw ports.error(403, 'FEATURE_COMING_SOON', 'Statutory payroll and payment batches need separate acceptance');
        const routes: Record<string, string[]> = {
            pos: ['pos'], pms: ['pms'], finance: ['finance'], 'hospitality-tax-documents': ['pos', 'pms'], commerce: ['procurement', 'inventory'],
            'stock-counts': ['inventory'], 'stock-transfers': ['inventory'], 'stock-transit': ['inventory'],
            'procurement-rfqs': ['procurement'], 'supplier-bills': ['procurement'], 'supplier-promises': ['procurement'],
            staff: ['people_payroll'], 'self-service': ['people_payroll'],
            leave: ['people_payroll'], 'leave-requests': ['people_payroll'], 'overtime-requests': ['people_payroll'],
            iam: ['identity_access'], assignments: ['identity_access'], technology: ['identity_access'],
            approvals: ['operations'], work: ['operations'], requests: ['operations'], 'staff-tasks': ['operations'],
            'routine-templates': ['operations'], 'routine-schedules': ['operations'], 'routing-attention': ['operations'],
            calendar: ['operations'], flows: ['operations'],
            collaboration: ['collaboration'], messaging: ['collaboration'], attachments: ['collaboration'], uploads: ['collaboration'],
            crm: ['crm'], customers: ['customer_360'], 'customer-followups': ['crm'],
            'customer-access': ['customer_authority'], 'customer-duplicates': ['customer_authority'],
            'customer-identity': ['customer_authority'], 'customer-notes': ['customer_360'],
            support: ['support'], marketing: ['marketing'], projects: ['projects'],
            'field-service': ['field_service'], assets: ['assets'], maintenance: ['maintenance'],
            quality: ['quality'], planning: ['planning'], manufacturing: ['manufacturing'],
            talent: ['talent'], learning: ['learning'], benefits: ['benefits'],
            ecommerce: ['ecommerce'], legal: ['legal'], agents: ['agent'],
            executive: ['executive'], branding: ['brand'], feed: ['feed'], posts: ['feed'], stories: ['feed'],
            billing: ['commercial'], expenses: ['people_payroll'], production: ['manufacturing'],
            'landed-costs': ['inventory'], 'service-teams': ['field_service'], records: ['records'], beta: ['beta'],
        };
        const peopleOwner = root === 'people'
            ? ['organization', 'templates'].includes(second) ? 'organization'
                : ['directory', 'directory-operations', 'access'].includes(second) ? 'identity_access'
                    : ['lifecycle', 'people', 'engagements', 'compensation', 'time-entries', 'roster'].includes(second) ? 'people_payroll'
                        : null
            : null;
        const modules = (root === 'people' ? (peopleOwner ? [peopleOwner] : []) : routes[root])?.filter(isReleaseModule);
        if (!modules?.length) {
            if (['ai-providers', 'assistant', 'business-readiness', 'companies', 'navigation', 'page-layouts'].includes(root))
                return;
            throw ports.error(403, 'FEATURE_COMING_SOON', 'This command is outside the approved hotel pilot');
        }
        const enabled = await db.enabled(companyId, modules);
        if (!enabled.length)
            throw ports.error(403, 'FEATURE_COMING_SOON', 'This company has not activated this workflow for the pilot');
    }
    async function assertProductionModuleEnabled(db: PmsRolloutRepository, companyId: string, moduleId: string) {
        if (ports.environment() !== 'production')
            return;
        if (moduleRegistry.some(module => module.id === moduleId) && await db.engineering(companyId))
            return;
        if (!isReleaseModule(moduleId))
            throw ports.error(403, 'FEATURE_COMING_SOON', 'This feature is outside the approved release scopes');
        const row = await db.active(companyId, moduleId);
        if (!row)
            throw ports.error(403, 'FEATURE_COMING_SOON', 'This company has not activated this workflow for the pilot');
    }
    async function enabledProductionModules(db: PmsRolloutRepository, companyId: string) { if (await db.engineering(companyId))
        return new Set(moduleRegistry.map(module => module.id)); const rows = await db.enabled(companyId); return new Set(rows.map(row => row.module_id).filter(isReleaseModule)); }
    return { companyFeatureCatalogue, changeCompanyFeatureRollout, assertProductionFeatureRoute, assertProductionModuleEnabled, enabledProductionModules };
}
