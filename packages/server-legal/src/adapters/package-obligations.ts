import 'server-only';
import {createHash} from 'node:crypto';
import type {PostgresDatabase} from '@erpx/server-runtime/public/runtime/postgres';
import type {SqlDatabase} from '@erpx/server-runtime/public/runtime/types';
import {packageSettlementSource,withPackageSettlementSources,type PackageSettlementSourceLock} from '@erpx/server-commercial/public/package-settlement-sources';
import {featureSpecs} from '@erpx/feature-legal/lib/legal/features';
import {nativeLegalPackageObligationPlan} from './native-legal-package-obligations';
type Input={companyId:string;packageId:string;packageVersion:number;moduleId:string;now:number};
const sources=new WeakMap<SqlDatabase,string>();
const tables=[...new Set(['legal_items','legal_members','legal_documents','legal_shares','legal_holds','legal_hold_targets','legal_hold_notices','legal_links','legal_executed_copies','legal_executed_reviews','legal_approvals','legal_approval_routes','legal_intake_forms','legal_authorities','legal_retention_policies','legal_retention_assignments','legal_destructions','legal_disposals','legal_file_preservations','legal_ai_proposals','legal_processing_jobs','legal_invitation_jobs',...Object.values(featureSpecs).map(spec=>spec.table)])];
export async function lockLegalPackageObligationSources(owner:PostgresDatabase,lock:PackageSettlementSourceLock){const {database,companyId}=packageSettlementSource(owner,lock);await database.prepare(`LOCK TABLE ${tables.join(',')} IN SHARE MODE`).run();sources.set(database,companyId);}
export function withLegalPackageObligations<Result>(owner:PostgresDatabase,companyId:string,run:(database:SqlDatabase)=>Promise<Result>){return withPackageSettlementSources(owner,companyId,[lock=>lockLegalPackageObligationSources(owner,lock)],run);}
/** Company-wide raw retained obligations; never derive a zero from a viewer's
 * purchase, membership, release or Work projection. Content stays owner-private.
 * The statement is planned by the native Legal owner and executed on the
 * caller's unit, so the read still observes uncommitted effects under the
 * source locks. */
export async function legalPackageObligations(owner:PostgresDatabase,database:SqlDatabase,input:Input){
 owner.assertActiveTransaction(database);if(sources.get(database)!==input.companyId)throw Error('LEGAL_PACKAGE_OBLIGATION_SOURCE_REQUIRED');if(input.moduleId!=='legal'||input.packageId!=='legal'||input.packageVersion!==1||!input.companyId||!Number.isSafeInteger(input.now)||input.now<0)throw Error('LEGAL_PACKAGE_OBLIGATION_SCOPE_REQUIRED');
 const plan=await nativeLegalPackageObligationPlan(input.companyId,input.now);
 const facts=(await database.prepare(plan.sql).bind(...plan.values).all()).results;
 return {companyId:input.companyId,packageId:input.packageId,moduleId:'legal',observedAt:input.now,revision:createHash('sha256').update(JSON.stringify({companyId:input.companyId,packageId:input.packageId,packageVersion:input.packageVersion,facts})).digest('hex'),outstanding:facts.length};
}
