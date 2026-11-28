import {createLegalRuntimeApplication,type LegalRuntimeTask} from "@erpx/legal-server/application/runtime-gate";
import 'server-only';
import type { SqlDatabase } from "@erpx/platform-runtime-server/public/runtime/types";
import { scheduledFeaturePredicate } from "@erpx/pms-server/public/feature-availability";
import type { WorkContext } from "@erpx/identity-access-server/public/authorization";
import { requirePermission } from "@erpx/identity-access-server/public/iam-permissions";
import contract from "../legal-runtime-gate-sources.json" with {type:"json"};

export type LegalCapability = 'operations' | 'documents' | 'ai' | 'sources' | 'invitations';

const fill = (template: string, values: Record<string, string>) => {
  let rendered = template;
  for (const [name, value] of Object.entries(values)) {
    const token = `{${name}}`;
    if (!rendered.includes(token)) throw new Error('LEGAL_RUNTIME_GATE_CONTRACT_INVALID');
    rendered = rendered.split(token).join(value);
  }
  return rendered;
};

/** Code-owned expressions only. Re-evaluate at the claim and side-effect boundary. */
export function legalRuntimePredicate(companyExpression: string, capability: LegalCapability = 'operations') {
  return fill(contract.settingsPredicate, {
    companyExpression,
    aiEnabled: capability === 'ai' ? ' AND runtime_legal_settings.ai_enabled' : '',
    scheduledFeaturePredicate: scheduledFeaturePredicate('runtime_legal_settings.company_id','legal'),
  });
}

export async function legalCompanyCapabilityReady(db: SqlDatabase, companyId: string, capability: LegalCapability = 'operations') {
  return !!await db.prepare(fill(contract.capabilityProbe,{predicate:legalRuntimePredicate('?',capability)})).bind(companyId).first();
}

export function legalRuntimeHealth(db:SqlDatabase,actor:WorkContext){return createLegalRuntimeApplication({require:()=>requirePermission(actor,'legal.manage'),tasks:async()=>(await db.prepare(contract.health).all<LegalRuntimeTask>()).results,ready:capability=>legalCompanyCapabilityReady(db,actor.companyId,capability),clock:{now:()=>Date.now()}}).legalRuntimeHealth();}
