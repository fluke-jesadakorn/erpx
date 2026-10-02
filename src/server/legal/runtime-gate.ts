import 'server-only';
import type { SqlDatabase } from '../runtime/types';
import { scheduledFeaturePredicate } from '../hospitality-rollout';
import type { WorkContext } from '../authorization';
import { requirePermission } from '../iam-permissions';

export type LegalCapability = 'operations' | 'documents' | 'ai' | 'sources' | 'invitations';

/** Code-owned expressions only. Re-evaluate at the claim and side-effect boundary. */
export function legalRuntimePredicate(companyExpression: string, capability: LegalCapability = 'operations') {
  return `EXISTS(SELECT 1 FROM legal_settings runtime_legal_settings WHERE runtime_legal_settings.company_id=${companyExpression}
    AND runtime_legal_settings.enabled ${capability === 'ai' ? 'AND runtime_legal_settings.ai_enabled' : ''}
    AND ${scheduledFeaturePredicate('runtime_legal_settings.company_id','legal')})`;
}

export async function legalCompanyCapabilityReady(db: SqlDatabase, companyId: string, capability: LegalCapability = 'operations') {
  return !!await db.prepare(`SELECT 1 WHERE ${legalRuntimePredicate('?',capability)}`).bind(companyId).first();
}

export async function legalRuntimeHealth(db: SqlDatabase, actor: WorkContext) {
  requirePermission(actor,'legal.manage');
  const tasks = (await db.prepare('SELECT task,heartbeat_at,last_success_at,failure_count,last_error FROM legal_runtime_health ORDER BY task')
    .all<{task:string;heartbeat_at:number;last_success_at:number|null;failure_count:number;last_error:string|null}>()).results;
  return { companyReady:await legalCompanyCapabilityReady(db,actor.companyId), tasks:tasks.map(task => ({
    ...task, stale:Date.now()-Number(task.heartbeat_at)>5*60_000,
  })), notice:'Production operation requires current Company release acceptance. Electronic signing and automatic destruction are deferred.' };
}
