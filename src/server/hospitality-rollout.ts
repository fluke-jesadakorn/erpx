import 'server-only';
import { z } from 'zod';
import { moduleRegistry, moduleAvailability } from '@/data/module-registry';
import { hasModuleAccess } from '@/data/module-access';
import { isPilotModule, resolveFeatureState, type RolloutState } from '@/lib/hospitality-rollout';
import type { WorkContext } from './authorization';
import { holdingResources } from './runtime';
import type { SqlDatabase } from './runtime/types';
import { ApiError } from './http';
import { requirePermission } from './iam-permissions';
import { assertion, executeCommand } from './commands';

const changeSchema = z.object({
  moduleId: z.string().min(1).max(80),
  state: z.enum(['off','pilot','available']),
  expectedVersion: z.number().int().min(0),
  reason: z.string().trim().min(3).max(1000),
});

type RolloutRow = { module_id: string; rollout_state: RolloutState; version: number; reason: string; changed_at: number };
type Readiness = Record<string, boolean>;

async function setupReadiness(db: SqlDatabase, companyId: string): Promise<Readiness> {
  const [pos, pms, finance, inventory, organization, commerce] = await Promise.all([
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
  ]);
  return { pos: Boolean(pos), pms: Boolean(pms), finance: Boolean(finance), inventory: Boolean(inventory),
    organization:Boolean(organization), procurement:Boolean(commerce), sales:Boolean(commerce) };
}

export async function companyFeatureCatalogue(actor: WorkContext) {
  const { productionAccepted } = await import('./hospitality-evidence');
  const { write } = await holdingResources(actor.holdingId);
  const rows = (await write.prepare('SELECT module_id,rollout_state,version,reason,changed_at FROM company_feature_rollouts WHERE company_id=?')
    .bind(actor.companyId).all<RolloutRow>()).results;
  const rollout = new Map(rows.map((row) => [row.module_id,row]));
  const ready = await setupReadiness(write,actor.companyId);
  const production = process.env.NODE_ENV === 'production';
  return moduleRegistry.map((module) => {
    const row = rollout.get(module.id);
    const accepted = productionAccepted(module.id);
    const setupChecked = module.id in ready;
    const setupReady = ready[module.id] ?? false;
    const state = resolveFeatureState({moduleId:module.id,production,rollout:row?.rollout_state,setupReady,productionAccepted:accepted,businessAccepted:false});
    return {
      id: module.id, name: module.name, state,
      delivery: moduleAvailability(module),
      rollout: row?.rollout_state ?? 'off', version: row?.version ?? 0,
      setupReady, setupChecked, productionAccepted: accepted, businessAccepted: false,
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
  if (!moduleRegistry.some((module) => module.id === input.moduleId)) throw new ApiError(404,'FEATURE_NOT_FOUND','Feature not found');
  if (input.state === 'pilot' && !isPilotModule(input.moduleId))
    throw new ApiError(409,'FEATURE_FUTURE_GATE','This feature needs a separate approved release cut');
  if (input.state === 'available' && !(await import('./hospitality-evidence')).productionAccepted(input.moduleId))
    throw new ApiError(409,'FEATURE_ACCEPTANCE_REQUIRED','Automated, browser, and production evidence must be accepted before general availability');
  if (input.state === 'available')
    throw new ApiError(409,'FEATURE_BUSINESS_ACCEPTANCE_REQUIRED','Independent hotel operations and Finance acceptance is required before general availability');
  const { write } = await holdingResources(actor.holdingId);
  const nextVersion = input.expectedVersion + 1;
  const now = Date.now();
  const commandId = `feature:${actor.companyId}:${input.moduleId}`;
  return executeCommand(write,actor,key,'feature.rollout.change',input,commandId,nextVersion,
    {id:input.moduleId,state:input.state,version:nextVersion},[
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
  if (root === 'feature-catalogue') return;
  if (destination === 'pos/sale/reverse_room_charge') return;
  if (root === 'finance' && ['engines','group'].includes(second))
    throw new ApiError(403,'FEATURE_COMING_SOON','Advanced Finance is not in this hotel pilot');
  const routes: Record<string,string[]> = {
    pos:['pos'], pms:['pms'], finance:['finance'], 'hospitality-tax-documents':['pos','pms'], commerce:['procurement','inventory'],
    'stock-counts':['inventory'], 'stock-transfers':['inventory'], 'stock-transit':['inventory'],
    'procurement-rfqs':['procurement'], 'supplier-bills':['procurement'], 'supplier-promises':['procurement'],
    people:['people_payroll','organization'], staff:['people_payroll'], 'self-service':['people_payroll'],
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
  const modules=routes[root]?.filter(isPilotModule);
  if (!modules?.length) {
    if (['ai-providers','assistant','business-readiness','companies','navigation','page-layouts'].includes(root)) return;
    throw new ApiError(403,'FEATURE_COMING_SOON','This command is outside the approved hotel pilot');
  }
  const enabled=(await db.prepare(`SELECT module_id FROM company_feature_rollouts
    WHERE company_id=? AND module_id IN (${modules.map(()=>'?').join(',')}) AND rollout_state IN ('pilot','available')`)
    .bind(companyId,...modules).all<{module_id:string}>()).results;
  if (!enabled.length) throw new ApiError(403,'FEATURE_COMING_SOON','This company has not activated this workflow for the pilot');
}

export async function assertProductionModuleEnabled(db: SqlDatabase, companyId: string, moduleId: string) {
  if (process.env.NODE_ENV !== 'production') return;
  if (!isPilotModule(moduleId)) throw new ApiError(403,'FEATURE_COMING_SOON','This feature is outside the approved hotel pilot');
  const row=await db.prepare("SELECT 1 FROM company_feature_rollouts WHERE company_id=? AND module_id=? AND rollout_state IN ('pilot','available')")
    .bind(companyId,moduleId).first();
  if (!row) throw new ApiError(403,'FEATURE_COMING_SOON','This company has not activated this workflow for the pilot');
}

/** Keep a queued result from becoming visible if rollout changes during provider inference. */
export function productionFeatureGuard(db: SqlDatabase, key: string, companyId: string, moduleId: string) {
  if (process.env.NODE_ENV !== 'production') return [];
  if (!isPilotModule(moduleId)) throw new ApiError(403,'FEATURE_COMING_SOON','This feature is outside the approved hotel pilot');
  return [db.prepare('SELECT module_id FROM company_feature_rollouts WHERE company_id=? AND module_id=? FOR SHARE').bind(companyId,moduleId),assertion(db,key,'feature-rollout',
    "EXISTS(SELECT 1 FROM company_feature_rollouts WHERE company_id=? AND module_id=? AND rollout_state IN ('pilot','available'))",
    [companyId,moduleId])];
}

/** SQL predicate for company-scoped scheduled reads and writes. Expressions are code-owned, never user input. */
export function scheduledFeaturePredicate(companyExpression: string, moduleId: string) {
  if (process.env.NODE_ENV !== 'production') return 'TRUE';
  if (!isPilotModule(moduleId)) return 'FALSE';
  return `EXISTS(SELECT 1 FROM company_feature_rollouts f WHERE f.company_id=${companyExpression} AND f.module_id='${moduleId}' AND f.rollout_state IN ('pilot','available'))`;
}
