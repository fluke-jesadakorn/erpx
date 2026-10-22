import 'server-only';
import { moduleRegistry } from '@erpx/catalogue/module-registry';
import { isReleaseModule } from "@erpx/feature-shared/lib/hospitality-rollout";
import { holdingResources } from "../../../../packages/server-runtime/src/public/runtime";
import type { SqlDatabase } from "@erpx/server-runtime/public/runtime/types";
import { ApiError } from "@erpx/server-runtime/public/http";
import { requirePermission } from "@erpx/server-identity_access/public/iam-permissions";
import {assertion} from "@erpx/server-workflow/public/command-assertions";
import {executeCommand,replayCommand} from "@erpx/server-workflow/public/commands";
import { acceptedReleasePredicate, companyReleaseStatus } from "@erpx/server-platform_administration/public/release-acceptance";
import { legalProductionScope } from '@erpx/catalogue/production-scope';
import { authorityGuard } from "@erpx/server-operations/public/staff-policy";
import { engineeringVerificationPredicate, isolatedEngineeringVerification } from "@erpx/server-platform_administration/public/engineering-verification";
import { createPmsRollout, type PmsRolloutRepository, type PmsRolloutRow } from "@erpx/server-pms/application/hospitality-rollout";
import { productionAccepted } from "@erpx/server-pms/public/hospitality-evidence";
type Readiness = Record<string, boolean>;
async function setupReadiness(db: SqlDatabase, companyId: string): Promise<Readiness> {
    const posFacts = pmsRolloutPosFacts(db), financeFacts = pmsRolloutFinanceFacts(db), commerceFacts = pmsRolloutCommerceFacts(db), organizationFacts = pmsRolloutOrganizationFacts(db), legalFacts = pmsRolloutLegalFacts(db);
    const [pos, pms, finance, inventory, organization, commerce, legal] = await Promise.all([
        posFacts.pos(companyId),
        db.prepare(`SELECT 1 FROM pms_properties p WHERE p.company_id=? AND p.state='open'
      AND p.config_json::jsonb->>'currency'='THB'
      AND COALESCE(p.config_json::jsonb->>'approvedBy','')<>''
      AND EXISTS(SELECT 1 FROM pms_records r WHERE r.property_id=p.id AND r.kind='room')
      AND EXISTS(SELECT 1 FROM pms_records r WHERE r.property_id=p.id AND r.kind='rate') LIMIT 1`).bind(companyId).first(),
        financeFacts.finance(companyId),
        commerceFacts.inventory(companyId),
        organizationFacts.organization(companyId),
        commerceFacts.commerce(companyId),
        legalFacts.legal(companyId),
    ]);
    return { pos: Boolean(pos), pms: Boolean(pms), finance: Boolean(finance), inventory: Boolean(inventory),
        organization: Boolean(organization), procurement: Boolean(commerce), sales: Boolean(commerce), legal: Boolean(legal) };
}
/** Keep a queued result from becoming visible if rollout changes during provider inference. */
export function productionFeatureGuard(db: SqlDatabase, key: string, companyId: string, moduleId: string) {
    if (process.env.NODE_ENV !== 'production')
        return [];
    const engineering = moduleRegistry.some(module => module.id === moduleId) ? engineeringVerificationPredicate('?') : 'FALSE';
    if (!isReleaseModule(moduleId)) {
        if (engineering === 'FALSE')
            throw new ApiError(403, 'FEATURE_COMING_SOON', 'This feature is outside the approved release scopes');
        return [assertion(db, key, 'isolated-engineering', engineering, [companyId])];
    }
    return [db.prepare('SELECT module_id FROM company_feature_rollouts WHERE company_id=? AND module_id=? FOR SHARE').bind(companyId, moduleId), assertion(db, key, 'feature-rollout', `(EXISTS(SELECT 1 FROM company_feature_rollouts f WHERE company_id=? AND module_id=? AND ${activeRolloutPredicate('f')}) OR ${engineering})`, [companyId, moduleId, ...(engineering === 'FALSE' ? [] : [companyId])])];
}
/** SQL predicate for company-scoped scheduled reads and writes. Expressions are code-owned, never user input. */
export function scheduledFeaturePredicate(companyExpression: string, moduleId: string) {
    if (process.env.NODE_ENV !== 'production')
        return 'TRUE';
    const engineering = moduleRegistry.some(module => module.id === moduleId) ? engineeringVerificationPredicate('engineering_subject.company_id') : 'FALSE';
    if (engineering === 'FALSE')
        return !isReleaseModule(moduleId) ? 'FALSE' : `EXISTS(SELECT 1 FROM company_feature_rollouts f WHERE f.company_id=${companyExpression} AND f.module_id='${moduleId}' AND ${activeRolloutPredicate('f')})`;
    const accepted = isReleaseModule(moduleId) ? `EXISTS(SELECT 1 FROM company_feature_rollouts f WHERE f.company_id=engineering_subject.company_id AND f.module_id='${moduleId}' AND ${activeRolloutPredicate('f')})` : 'FALSE';
    return `(SELECT (${accepted} OR ${engineering}) FROM (SELECT ${companyExpression}::text company_id) engineering_subject)`;
}
function activeRolloutPredicate(alias: string) {
    return `(CASE WHEN ${alias}.module_id='legal' THEN ${alias}.rollout_state IN ('pilot','available') AND ${acceptedReleasePredicate(`${alias}.company_id`, legalProductionScope.version)} ELSE (${alias}.rollout_state='pilot' OR (${alias}.rollout_state='available' AND ${acceptedReleasePredicate(`${alias}.company_id`)})) END)`;
}
function repository(write: SqlDatabase): PmsRolloutRepository {
    return {
        rollouts: companyId => write.prepare('SELECT module_id,rollout_state,version,reason,changed_at FROM company_feature_rollouts WHERE company_id=?').bind(companyId).all<PmsRolloutRow>().then(rows => rows.results),
        readiness: companyId => setupReadiness(write, companyId), release: (companyId, id, scope) => companyReleaseStatus(write, companyId, id, scope), engineering: companyId => isolatedEngineeringVerification(write, companyId),
        replay: (actor, key, commandId, input) => replayCommand(write, actor, key, 'feature.rollout.change', commandId, input),
        commit: (actor, key, input, commandId, nextVersion, now, release, scope) => {
            return executeCommand(write, actor, key, 'feature.rollout.change', input, commandId, nextVersion, { id: input.moduleId, state: input.state, version: nextVersion }, [
                authorityGuard(write, key, actor, 'organization.manage'),
                ...pmsRolloutReleaseGuards(write, key, actor, release, scope),
                assertion(write, key, 'feature-rollout-version', 'COALESCE((SELECT version FROM company_feature_rollouts WHERE company_id=? AND module_id=?),0)=?', [actor.companyId, input.moduleId, input.expectedVersion]),
                write.prepare(`INSERT INTO company_feature_rollouts(company_id,module_id,rollout_state,version,reason,changed_by,changed_at)
        VALUES(?,?,?,?,?,?,?) ON CONFLICT(company_id,module_id) DO UPDATE SET
        rollout_state=excluded.rollout_state,version=excluded.version,reason=excluded.reason,
        changed_by=excluded.changed_by,changed_at=excluded.changed_at`).bind(actor.companyId, input.moduleId, input.state, nextVersion, input.reason, actor.userId, now),
            ]);
        },
        enabled: (companyId, modules) => write.prepare(`SELECT module_id FROM company_feature_rollouts f WHERE company_id=? ${modules ? `AND module_id IN (${modules.map(() => '?').join(',')})` : ''} AND ${activeRolloutPredicate('f')}`).bind(companyId, ...(modules ?? [])).all<{
            module_id: string;
        }>().then(rows => rows.results),
        active: (companyId, moduleId) => write.prepare(`SELECT 1 FROM company_feature_rollouts f WHERE company_id=? AND module_id=? AND ${activeRolloutPredicate('f')}`).bind(companyId, moduleId).first()
    };
}
const application = createPmsRollout({ repository: async (actor) => repository((await holdingResources(actor.holdingId)).write), clock: { now: () => Date.now() }, require: requirePermission, error: (status, code, message) => new ApiError(status, code, message), environment: () => process.env.NODE_ENV, deployment: () => process.env.ERPX_ENVIRONMENT, inventoryAccepted: productionAccepted });
export const companyFeatureCatalogue = application.companyFeatureCatalogue;
export const changeCompanyFeatureRollout = application.changeCompanyFeatureRollout;
export function assertProductionFeatureRoute(db: SqlDatabase, companyId: string, destination: string, method: string) { return application.assertProductionFeatureRoute(repository(db), companyId, destination, method); }
export function assertProductionModuleEnabled(db: SqlDatabase, companyId: string, moduleId: string) { return application.assertProductionModuleEnabled(repository(db), companyId, moduleId); }
export function enabledProductionModules(db: SqlDatabase, companyId: string) { return application.enabledProductionModules(repository(db), companyId); }
import { pmsRolloutPosFacts } from "@erpx/server-pos/public/pms-rollout-facts";
import { pmsRolloutFinanceFacts } from "@erpx/server-finance/public/pms-rollout-facts";
import { pmsRolloutCommerceFacts } from "@erpx/server-commerce/public/pms-rollout-facts";
import { pmsRolloutOrganizationFacts } from "@erpx/server-organization/public/pms-rollout-facts";
import { pmsRolloutLegalFacts } from "@erpx/server-legal/public/pms-rollout-facts";
import { pmsRolloutReleaseGuards } from "@erpx/server-platform_administration/public/pms-rollout-guards";
