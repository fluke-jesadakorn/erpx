import {moduleRegistry} from "@erpx/catalogue/module-registry";
import {isReleaseModule} from "@erpx/feature-shared/lib/hospitality-rollout";
import type {PmsRolloutRepository} from "@erpx/server-pms/application/hospitality-rollout";
export type FeatureAvailabilityRepository=Pick<PmsRolloutRepository,"engineering"|"enabled"|"active">;
/** Availability decisions require read ports; they cannot execute rollout commands. */
export function createFeatureAvailability(ports:{environment():string|undefined;deployment():string|undefined;error(status:number,code:string,message:string):Error}){
async function assertProductionFeatureRoute(db: FeatureAvailabilityRepository, companyId: string, destination: string, method: string) {
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
async function assertProductionModuleEnabled(db: FeatureAvailabilityRepository, companyId: string, moduleId: string) {
        if (ports.environment() !== 'production')
            return;
        const registeredEngineeringFeature = moduleRegistry.some(module =>
            module.sections.some(section => section.id === moduleId && section.capability));
        if ((moduleRegistry.some(module => module.id === moduleId) || registeredEngineeringFeature) && await db.engineering(companyId))
            return;
        if (!isReleaseModule(moduleId))
            throw ports.error(403, 'FEATURE_COMING_SOON', 'This feature is outside the approved release scopes');
        const row = await db.active(companyId, moduleId);
        if (!row)
            throw ports.error(403, 'FEATURE_COMING_SOON', 'This company has not activated this workflow for the pilot');
    }
async function enabledProductionModules(db: FeatureAvailabilityRepository, companyId: string) { if (await db.engineering(companyId))
        return new Set(moduleRegistry.map(module => module.id)); const rows = await db.enabled(companyId); return new Set(rows.map(row => row.module_id).filter(isReleaseModule)); }
return {assertProductionFeatureRoute,assertProductionModuleEnabled,enabledProductionModules};
}
