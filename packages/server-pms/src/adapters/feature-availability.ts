import 'server-only';
import {moduleRegistry} from '@erpx/catalogue/module-registry';
import {isReleaseModule} from '@erpx/feature-shared/lib/hospitality-rollout';
import {legalProductionScope} from '@erpx/catalogue/production-scope';
import type {SqlDatabase} from '@erpx/server-runtime/public/runtime/types';
import {ApiError} from '@erpx/server-runtime/public/http';
import {assertion} from '@erpx/server-workflow/public/command-assertions';
import {acceptedReleasePredicate} from '@erpx/server-platform_administration/public/release-predicate';
import {engineeringVerificationPredicate,isolatedEngineeringVerification} from '@erpx/server-platform_administration/public/engineering-verification';
import {createFeatureAvailability,type FeatureAvailabilityRepository} from '@erpx/server-pms/application/feature-availability';
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
export function scheduledFeaturePredicate(companyExpression: string, moduleId: string) {
    if (process.env.NODE_ENV !== 'production')
        return 'TRUE';
    const engineering = moduleRegistry.some(module => module.id === moduleId) ? engineeringVerificationPredicate('engineering_subject.company_id') : 'FALSE';
    if (engineering === 'FALSE')
        return !isReleaseModule(moduleId) ? 'FALSE' : `EXISTS(SELECT 1 FROM company_feature_rollouts f WHERE f.company_id=${companyExpression} AND f.module_id='${moduleId}' AND ${activeRolloutPredicate('f')})`;
    const accepted = isReleaseModule(moduleId) ? `EXISTS(SELECT 1 FROM company_feature_rollouts f WHERE f.company_id=engineering_subject.company_id AND f.module_id='${moduleId}' AND ${activeRolloutPredicate('f')})` : 'FALSE';
    return `(SELECT (${accepted} OR ${engineering}) FROM (SELECT ${companyExpression}::text company_id) engineering_subject)`;
}
export function activeRolloutPredicate(alias: string) {
    return `(CASE WHEN ${alias}.module_id='legal' THEN ${alias}.rollout_state IN ('pilot','available') AND ${acceptedReleasePredicate(`${alias}.company_id`, legalProductionScope.version)} ELSE (${alias}.rollout_state='pilot' OR (${alias}.rollout_state='available' AND ${acceptedReleasePredicate(`${alias}.company_id`)})) END)`;
}
function repository(write:SqlDatabase):FeatureAvailabilityRepository{return {engineering:companyId=>isolatedEngineeringVerification(write,companyId),enabled: (companyId, modules) => write.prepare(`SELECT module_id FROM company_feature_rollouts f WHERE company_id=? ${modules ? `AND module_id IN (${modules.map(() => '?').join(',')})` : ''} AND ${activeRolloutPredicate('f')}`).bind(companyId, ...(modules ?? [])).all<{
            module_id: string;
        }>().then(rows => rows.results),active: (companyId, moduleId) => write.prepare(`SELECT 1 FROM company_feature_rollouts f WHERE company_id=? AND module_id=? AND ${activeRolloutPredicate('f')}`).bind(companyId, moduleId).first()};}
const availability=createFeatureAvailability({environment:()=>process.env.NODE_ENV,deployment:()=>process.env.ERPX_ENVIRONMENT,error:(status,code,message)=>new ApiError(status,code,message)});
export function assertProductionFeatureRoute(db: SqlDatabase, companyId: string, destination: string, method: string) { return availability.assertProductionFeatureRoute(repository(db), companyId, destination, method); }
export function assertProductionModuleEnabled(db: SqlDatabase, companyId: string, moduleId: string) { return availability.assertProductionModuleEnabled(repository(db), companyId, moduleId); }
export function enabledProductionModules(db: SqlDatabase, companyId: string) { return availability.enabledProductionModules(repository(db), companyId); }
