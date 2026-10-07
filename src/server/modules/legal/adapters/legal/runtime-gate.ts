import {createLegalRuntimeApplication,type LegalRuntimeTask} from "../../application/runtime-gate";
import 'server-only';
import type { SqlDatabase } from "@/server/modules/runtime/public/runtime/types";
import { scheduledFeaturePredicate } from "@/server/modules/pms/public/hospitality-rollout";
import type { WorkContext } from "@/server/modules/identity_access/public/authorization";
import { requirePermission } from "@/server/modules/identity_access/public/iam-permissions";

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

export function legalRuntimeHealth(db:SqlDatabase,actor:WorkContext){return createLegalRuntimeApplication({require:()=>requirePermission(actor,'legal.manage'),tasks:async()=>(await db.prepare('SELECT task,heartbeat_at,last_success_at,failure_count,last_error FROM legal_runtime_health ORDER BY task').all<LegalRuntimeTask>()).results,ready:capability=>legalCompanyCapabilityReady(db,actor.companyId,capability),clock:{now:()=>Date.now()}}).legalRuntimeHealth();}
