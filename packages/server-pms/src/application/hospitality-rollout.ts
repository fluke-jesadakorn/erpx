import {createFeatureAvailability} from "@erpx/server-pms/application/feature-availability";
import { moduleRegistry, moduleAvailability } from "@erpx/catalogue/module-registry";
import { hasModuleAccess } from "@erpx/catalogue/module-access";
import { isReleaseModule, resolveFeatureState, type RolloutState } from "@erpx/feature-shared/lib/hospitality-rollout";
import { productionScope, legalProductionScope } from "@erpx/catalogue/production-scope";
import { changeSchema } from "@erpx/server-pms/domain/hospitality-rollout";
import type { WorkContext } from "@erpx/server-identity_access/public/authorization";
import type { Clock } from "@erpx/server-shared/application/ports";
import type { z } from "zod";
export type PmsRolloutRow = {
    module_id: string;
    rollout_state: RolloutState;
    version: number;
    reason: string;
    changed_at: number;
};
export type PmsRolloutInput = z.infer<typeof changeSchema>;
export type PmsRolloutRelease = Awaited<ReturnType<typeof import("@erpx/server-platform_administration/public/release-acceptance").companyReleaseStatus>>;
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
    return { companyFeatureCatalogue, changeCompanyFeatureRollout, ...createFeatureAvailability(ports) };
}
