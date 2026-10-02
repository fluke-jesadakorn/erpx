import 'server-only';
import { z } from 'zod';
import { moduleRegistry, moduleAvailability } from '@/data/module-registry';
import { hasModuleAccess } from '@/data/module-access';
import { isReleaseModule, resolveFeatureState, type RolloutState } from '@/lib/hospitality-rollout';
import type { WorkContext } from './authorization';
import { holdingResources } from './runtime';
import type { SqlDatabase } from './runtime/types';
import { ApiError } from './http';
import { requirePermission } from './iam-permissions';
import { assertion, executeCommand, replayCommand } from './commands';
import { acceptedReleasePredicate, companyReleaseStatus, releaseRevisionTable } from './release-acceptance';
import { productionScope, legalProductionScope } from '@/data/production-scope';
import artifact from '@/data/release-artifact.json';
import { authorityGuard } from './staff-policy';

const changeSchema = z.object({
  moduleId: z.string().min(1).max(80),
  state: z.enum(['off','pilot','available']),
  expectedVersion: z.number().int().min(0),
  reason: z.string().trim().min(3).max(1000),
});

type RolloutRow = { module_id: string; rollout_state: RolloutState; version: number; reason: string; changed_at: number };
type Readiness = Record<string, boolean>;

async function setupReadiness(db: SqlDatabase, companyId: string): Promise<Readiness> {
  const [pos, pms, finance, inventory, organization, commerce, legal] = await Promise.all([
    db.prepare(`SELECT 1 FROM pos_settings s WHERE s.company_id=? AND s.enabled=true
      AND EXISTS(SELECT 1 FROM pos_registers r WHERE r.company_id=s.company_id AND r.active=true)
      AND EXISTS(SELECT 1 FROM commerce_products p WHERE p.company_id=s.company_id AND p.status='active')`).bind(companyId).first(),
    db.prepare(`SELECT 1 FROM pms_properties p WHERE p.company_id=? AND p.state='open'
      AND p.config_json::jsonb->>'currency'='THB'
      AND COALESCE(p.config_json::jsonb->>'approvedBy','')<>''
      AND EXISTS(SELECT 1 FROM pms_records r WHERE r.property_id=p.id AND r.kind='room')
      AND EXISTS(SELECT 1 FROM pms_records r WHERE r.property_id=p.id AND r.kind='rate') LIMIT 1`).bind(companyId).first(),
    db.prepare(`SELECT 1 FROM finance_charts c WHERE c.company_id=? AND c.base_currency='THB'
      AND EXISTS(SELECT 1 FROM finance_periods p WHERE p.company_id=c.company_id AND p.state='open') LIMIT 1`).bind(companyId).first(),
    db.prepare("SELECT 1 FROM commerce_warehouses WHERE company_id=? AND status='active' LIMIT 1").bind(companyId).first(),
    db.prepare("SELECT 1 FROM organization_nodes WHERE company_id=? AND kind='branch' AND status='active' LIMIT 1").bind(companyId).first(),
    db.prepare('SELECT 1 FROM commerce_settings WHERE company_id=?').bind(companyId).first(),
    db.prepare('SELECT 1 FROM legal_settings WHERE company_id=? AND enabled').bind(companyId).first(),
  ]);
  return { pos: Boolean(pos), pms: Boolean(pms), finance: Boolean(finance), inventory: Boolean(inventory),
    organization:Boolean(organization), procurement:Boolean(commerce), sales:Boolean(commerce), legal:Boolean(legal) };
}

export async function companyFeatureCatalogue(actor: WorkContext) {
  const { productionAccepted } = await import('./hospitality-evidence');
  const { write } = await holdingResources(actor.holdingId);
  const rows = (await write.prepare('SELECT module_id,rollout_state,version,reason,changed_at FROM company_feature_rollouts WHERE company_id=?')
    .bind(actor.companyId).all<RolloutRow>()).results;
  const rollout = new Map(rows.map((row) => [row.module_id,row]));
  const ready = await setupReadiness(write,actor.companyId);
  const release = await companyReleaseStatus(write,actor.companyId);
  const legalRelease = await companyReleaseStatus(write,actor.companyId,undefined,legalProductionScope.version);
  const production = process.env.NODE_ENV === 'production';
  return moduleRegistry.map((module) => {
    const row = rollout.get(module.id);
    const accepted = (module.id === 'legal' ? legalRelease : release).state === 'passed';
    const setupChecked = module.id in ready;
    const setupReady = ready[module.id] ?? false;
    const state = resolveFeatureState({moduleId:module.id,production,rollout:row?.rollout_state,setupReady,productionAccepted:accepted,businessAccepted:accepted});
    return {
      id: module.id, name: module.name, state,
      delivery: moduleAvailability(module),
      rollout: row?.rollout_state ?? 'off', version: row?.version ?? 0,
      setupReady, setupChecked, productionAccepted: accepted, businessAccepted: accepted,
      inventoryAccepted: productionAccepted(module.id),
      scopeVersion: module.id === 'legal' ? legalProductionScope.version : productionScope.version,
      environment: production ? 'production' : 'development',
      permitted: hasModuleAccess(actor.modules,module.id),
      ...(row ? { reason: row.reason, changedAt: row.changed_at } : {}),
    };
  });
}

export async function changeCompanyFeatureRollout(actor: WorkContext, raw: unknown, key: string) {
  requirePermission(actor,'organization.manage');
  if (actor.branchId || actor.departmentId) throw new ApiError(403,'FEATURE_COMPANY_REQUIRED','Manage rollout from the company workspace');
  const input = changeSchema.parse(raw);
  const { write } = await holdingResources(actor.holdingId);
  const commandId = `feature:${actor.companyId}:${input.moduleId}`;
  const prior = await replayCommand(write,actor,key,'feature.rollout.change',commandId,input); if(prior)return prior;
  if (!moduleRegistry.some((module) => module.id === input.moduleId)) throw new ApiError(404,'FEATURE_NOT_FOUND','Feature not found');
  if (input.state !== 'off' && !isReleaseModule(input.moduleId))
    throw new ApiError(409,'FEATURE_FUTURE_GATE','This feature needs a separate approved release cut');
  const scope = input.moduleId === 'legal' ? legalProductionScope.version : productionScope.version;
  const needsAcceptance = input.state === 'available' || (input.moduleId === 'legal' && input.state === 'pilot' && process.env.NODE_ENV === 'production');
  const release = needsAcceptance ? await companyReleaseStatus(write,actor.companyId,undefined,scope) : null;
  if (needsAcceptance && release?.state !== 'passed')
    throw new ApiError(409,'FEATURE_ACCEPTANCE_REQUIRED','All criteria for this Company, scope, build and configuration require independent release acceptance');
  const nextVersion = input.expectedVersion + 1;
  const now = Date.now();
  return executeCommand(write,actor,key,'feature.rollout.change',input,commandId,nextVersion,
    {id:input.moduleId,state:input.state,version:nextVersion},[
      authorityGuard(write,key,actor,'organization.manage'),
      ...(release?.release ? [
        write.prepare(`SELECT revision FROM ${releaseRevisionTable(scope)} WHERE company_id=? FOR SHARE`).bind(actor.companyId),
        write.prepare('SELECT id FROM company_release_acceptances WHERE id=? FOR SHARE').bind(release.release.id),
        assertion(write,key,'feature-release',`EXISTS(SELECT 1 FROM company_release_acceptances r JOIN ${releaseRevisionTable(scope)} c ON c.company_id=r.company_id
          WHERE r.id=? AND r.company_id=? AND r.version=? AND r.revoked_at IS NULL AND r.configuration_revision=c.revision
          AND r.scope_version=? AND r.software_version=?)`,
          [release.release.id,actor.companyId,release.release.version,scope,artifact.softwareVersion]),
        assertion(write,key,'feature-release-reviewers',acceptedReleasePredicate('?',scope),[actor.companyId]),
      ] : []),
      assertion(write,key,'feature-rollout-version',
        'COALESCE((SELECT version FROM company_feature_rollouts WHERE company_id=? AND module_id=?),0)=?',
        [actor.companyId,input.moduleId,input.expectedVersion]),
      write.prepare(`INSERT INTO company_feature_rollouts(company_id,module_id,rollout_state,version,reason,changed_by,changed_at)
        VALUES(?,?,?,?,?,?,?) ON CONFLICT(company_id,module_id) DO UPDATE SET
        rollout_state=excluded.rollout_state,version=excluded.version,reason=excluded.reason,
        changed_by=excluded.changed_by,changed_at=excluded.changed_at`).bind(
          actor.companyId,input.moduleId,input.state,nextVersion,input.reason,actor.userId,now),
    ]);
}

/** Request boundary for owner APIs; reads remain available for authorized history. */
export async function assertProductionFeatureRoute(db: SqlDatabase, companyId: string, destination: string, method: string) {
  if (process.env.NODE_ENV !== 'production' || ['GET','HEAD','OPTIONS'].includes(method)) return;
  const [root,second] = destination.split('/');
  // The isolated staging standalone runs with NODE_ENV=production. Payroll
  // acceptance exercises remain sandbox-only there; the private production
  // service keeps its separate ERPX_ENVIRONMENT=production prohibition.
  if (process.env.ERPX_ENVIRONMENT === 'staging' && root === 'people' &&
      ['payroll','pay-groups','country-packs','payments'].includes(second)) return;
  if (root === 'feature-catalogue') return;
  // Legal shares one command endpoint for configuration and operations. Its
  // domain guards gate operational commands while managers prepare acceptance.
  if (root === 'legal') return;
  if (destination === 'pos/sale/reverse_room_charge') return;
  if (root === 'finance' && ['engines','group'].includes(second))
    throw new ApiError(403,'FEATURE_COMING_SOON','Advanced Finance is not in this hotel pilot');
  if (root === 'people' && ['payroll','pay-groups','country-packs','payments'].includes(second))
    throw new ApiError(403,'FEATURE_COMING_SOON','Statutory payroll and payment batches need separate acceptance');
  const routes: Record<string,string[]> = {
    pos:['pos'], pms:['pms'], finance:['finance'], 'hospitality-tax-documents':['pos','pms'], commerce:['procurement','inventory'],
    'stock-counts':['inventory'], 'stock-transfers':['inventory'], 'stock-transit':['inventory'],
    'procurement-rfqs':['procurement'], 'supplier-bills':['procurement'], 'supplier-promises':['procurement'],
    staff:['people_payroll'], 'self-service':['people_payroll'],
    leave:['people_payroll'], 'leave-requests':['people_payroll'], 'overtime-requests':['people_payroll'],
    iam:['identity_access'], assignments:['identity_access'], technology:['identity_access'],
    approvals:['operations'], work:['operations'], requests:['operations'], 'staff-tasks':['operations'],
    'routine-templates':['operations'], 'routine-schedules':['operations'], 'routing-attention':['operations'],
    calendar:['operations'], flows:['operations'],
    collaboration:['collaboration'], messaging:['collaboration'], attachments:['collaboration'], uploads:['collaboration'],
    crm:['crm'], customers:['customer_360'], 'customer-followups':['crm'],
    'customer-access':['customer_authority'], 'customer-duplicates':['customer_authority'],
    'customer-identity':['customer_authority'], 'customer-notes':['customer_360'],
    support:['support'], marketing:['marketing'], projects:['projects'],
    'field-service':['field_service'], assets:['assets'], maintenance:['maintenance'],
    quality:['quality'], planning:['planning'], manufacturing:['manufacturing'],
    talent:['talent'], learning:['learning'], benefits:['benefits'],
    ecommerce:['ecommerce'], legal:['legal'], agents:['agent'],
    executive:['executive'], branding:['brand'], feed:['feed'], posts:['feed'], stories:['feed'],
    billing:['commercial'], expenses:['people_payroll'], production:['manufacturing'],
    'landed-costs':['inventory'], 'service-teams':['field_service'], records:['records'], beta:['beta'],
  };
  const peopleOwner = root === 'people'
    ? ['organization','templates'].includes(second) ? 'organization'
      : ['directory','directory-operations','access'].includes(second) ? 'identity_access'
        : ['lifecycle','people','engagements','compensation','time-entries','roster'].includes(second) ? 'people_payroll'
          : null
    : null;
  const modules=(root === 'people' ? (peopleOwner ? [peopleOwner] : []) : routes[root])?.filter(isReleaseModule);
  if (!modules?.length) {
    if (['ai-providers','assistant','business-readiness','companies','navigation','page-layouts'].includes(root)) return;
    throw new ApiError(403,'FEATURE_COMING_SOON','This command is outside the approved hotel pilot');
  }
  const enabled=(await db.prepare(`SELECT module_id FROM company_feature_rollouts f
    WHERE company_id=? AND module_id IN (${modules.map(()=>'?').join(',')}) AND ${activeRolloutPredicate('f')}`)
    .bind(companyId,...modules).all<{module_id:string}>()).results;
  if (!enabled.length) throw new ApiError(403,'FEATURE_COMING_SOON','This company has not activated this workflow for the pilot');
}

export async function assertProductionModuleEnabled(db: SqlDatabase, companyId: string, moduleId: string) {
  if (process.env.NODE_ENV !== 'production') return;
  if (!isReleaseModule(moduleId)) throw new ApiError(403,'FEATURE_COMING_SOON','This feature is outside the approved release scopes');
  const row=await db.prepare(`SELECT 1 FROM company_feature_rollouts f WHERE company_id=? AND module_id=? AND ${activeRolloutPredicate('f')}`)
    .bind(companyId,moduleId).first();
  if (!row) throw new ApiError(403,'FEATURE_COMING_SOON','This company has not activated this workflow for the pilot');
}

/** Keep a queued result from becoming visible if rollout changes during provider inference. */
export function productionFeatureGuard(db: SqlDatabase, key: string, companyId: string, moduleId: string) {
  if (process.env.NODE_ENV !== 'production') return [];
  if (!isReleaseModule(moduleId)) throw new ApiError(403,'FEATURE_COMING_SOON','This feature is outside the approved release scopes');
  return [db.prepare('SELECT module_id FROM company_feature_rollouts WHERE company_id=? AND module_id=? FOR SHARE').bind(companyId,moduleId),assertion(db,key,'feature-rollout',
    `EXISTS(SELECT 1 FROM company_feature_rollouts f WHERE company_id=? AND module_id=? AND ${activeRolloutPredicate('f')})`,
    [companyId,moduleId])];
}

/** SQL predicate for company-scoped scheduled reads and writes. Expressions are code-owned, never user input. */
export function scheduledFeaturePredicate(companyExpression: string, moduleId: string) {
  if (process.env.NODE_ENV !== 'production') return 'TRUE';
  if (!isReleaseModule(moduleId)) return 'FALSE';
  return `EXISTS(SELECT 1 FROM company_feature_rollouts f WHERE f.company_id=${companyExpression} AND f.module_id='${moduleId}' AND ${activeRolloutPredicate('f')})`;
}

function activeRolloutPredicate(alias: string) {
  return `(CASE WHEN ${alias}.module_id='legal' THEN ${alias}.rollout_state IN ('pilot','available') AND ${acceptedReleasePredicate(`${alias}.company_id`,legalProductionScope.version)} ELSE (${alias}.rollout_state='pilot' OR (${alias}.rollout_state='available' AND ${acceptedReleasePredicate(`${alias}.company_id`)})) END)`;
}

export async function enabledProductionModules(db: SqlDatabase, companyId: string) {
  const rows = (await db.prepare(`SELECT module_id FROM company_feature_rollouts f WHERE f.company_id=? AND ${activeRolloutPredicate('f')}`)
    .bind(companyId).all<{ module_id: string }>()).results;
  return new Set(rows.map(row=>row.module_id).filter(isReleaseModule));
}
