import 'server-only';
import {createHash} from 'node:crypto';
import type {PostgresDatabase} from '@erpx/server-runtime/public/runtime/postgres';
import type {SqlDatabase} from '@erpx/server-runtime/public/runtime/types';
import {packageSettlementSource,withPackageSettlementSources,type PackageSettlementSourceLock} from '@erpx/server-commercial/public/package-settlement-sources';
import {featureSpecs} from '@erpx/feature-legal/lib/legal/features';
type Input={companyId:string;packageId:string;packageVersion:number;moduleId:string;now:number};
const sources=new WeakMap<SqlDatabase,string>();
const tables=[...new Set(['legal_items','legal_members','legal_documents','legal_shares','legal_holds','legal_hold_targets','legal_hold_notices','legal_links','legal_executed_copies','legal_executed_reviews','legal_approvals','legal_approval_routes','legal_intake_forms','legal_authorities','legal_retention_policies','legal_retention_assignments','legal_destructions','legal_disposals','legal_file_preservations','legal_ai_proposals','legal_processing_jobs','legal_invitation_jobs',...Object.values(featureSpecs).map(spec=>spec.table)])];
export async function lockLegalPackageObligationSources(owner:PostgresDatabase,lock:PackageSettlementSourceLock){const {database,companyId}=packageSettlementSource(owner,lock);await database.prepare(`LOCK TABLE ${tables.join(',')} IN SHARE MODE`).run();sources.set(database,companyId);}
export function withLegalPackageObligations<Result>(owner:PostgresDatabase,companyId:string,run:(database:SqlDatabase)=>Promise<Result>){return withPackageSettlementSources(owner,companyId,[lock=>lockLegalPackageObligationSources(owner,lock)],run);}
/** Company-wide raw retained obligations; never derive a zero from a viewer's
 * purchase, membership, release or Work projection. Content stays owner-private. */
export async function legalPackageObligations(owner:PostgresDatabase,database:SqlDatabase,input:Input){
 owner.assertActiveTransaction(database);if(sources.get(database)!==input.companyId)throw Error('LEGAL_PACKAGE_OBLIGATION_SOURCE_REQUIRED');if(input.moduleId!=='legal'||input.packageId!=='legal'||input.packageVersion!==1||!input.companyId||!Number.isSafeInteger(input.now)||input.now<0)throw Error('LEGAL_PACKAGE_OBLIGATION_SCOPE_REQUIRED');
 const features=Object.values(featureSpecs).map(spec=>`SELECT '${spec.table}' kind,f.id,to_jsonb(f)::text fact FROM ${spec.table} f LEFT JOIN legal_items i ON i.id=f.item_id,scope WHERE (i.company_id=scope.company_id OR i.id IS NULL) AND (i.id IS NULL OR f.state NOT IN ('completed','closed','cancelled','acknowledged','waived','accepted','billed') AND NOT(f.category='custody' AND f.state='received'))`).join(' UNION ALL ');
 const facts=(await database.prepare(`WITH scope AS(SELECT ?::text company_id,?::bigint now_ms)
 SELECT 'item' kind,i.id,to_jsonb(i)::text fact FROM legal_items i,scope WHERE i.company_id=scope.company_id AND i.state NOT IN ('archived','closed','completed','expired','terminated','cancelled','rejected')
 UNION ALL ${features}
 UNION ALL SELECT 'hold',h.id,to_jsonb(h)::text FROM legal_holds h LEFT JOIN legal_items i ON i.id=h.item_id,scope WHERE (i.company_id=scope.company_id OR i.id IS NULL) AND h.state<>'released'
 UNION ALL SELECT 'share',s.id,to_jsonb(s)::text FROM legal_shares s LEFT JOIN legal_items i ON i.id=s.item_id LEFT JOIN legal_documents d ON d.id=s.document_id AND d.item_id=s.item_id,scope WHERE (i.company_id=scope.company_id OR i.id IS NULL) AND (i.id IS NULL OR d.id IS NULL OR s.revoked_at IS NULL AND s.expires_at>scope.now_ms)
 UNION ALL SELECT 'executed-review',e.id,to_jsonb(e)::text FROM legal_executed_copies e LEFT JOIN legal_items i ON i.id=e.item_id LEFT JOIN legal_documents d ON d.id=e.document_id AND d.item_id=e.item_id,scope WHERE (i.company_id=scope.company_id OR i.id IS NULL) AND (i.id IS NULL OR d.id IS NULL OR e.state<>'confirmed')
 UNION ALL SELECT 'approval',r.item_id,to_jsonb(r)::text FROM legal_approval_routes r LEFT JOIN legal_items i ON i.id=r.item_id,scope WHERE (i.company_id=scope.company_id OR i.id IS NULL) AND (i.id IS NULL OR i.state IN ('review','approval','signing'))
 UNION ALL SELECT 'ai-job',j.id,to_jsonb(j)::text FROM legal_ai_proposals j LEFT JOIN legal_items i ON i.id=j.item_id,scope WHERE (i.company_id=scope.company_id OR i.id IS NULL) AND (i.id IS NULL OR j.state IN ('queued','running','proposed'))
 UNION ALL SELECT 'processing-job',j.id,to_jsonb(j)::text FROM legal_processing_jobs j LEFT JOIN legal_items i ON i.id=j.item_id,scope WHERE (j.company_id=scope.company_id OR i.company_id=scope.company_id) AND (i.id IS NULL OR i.company_id<>j.company_id OR j.state IN ('queued','running'))
 UNION ALL SELECT 'invitation-job',j.id,to_jsonb(j)::text FROM legal_invitation_jobs j LEFT JOIN legal_shares s ON s.id=j.share_id LEFT JOIN legal_items i ON i.id=s.item_id,scope WHERE (i.company_id=scope.company_id OR i.id IS NULL) AND (i.id IS NULL OR j.state IN ('queued','running'))
 UNION ALL SELECT 'destruction',d.id,to_jsonb(d)::text FROM legal_destructions d LEFT JOIN legal_items i ON i.id=d.item_id,scope WHERE (i.company_id=scope.company_id OR i.id IS NULL) AND (i.id IS NULL OR d.state NOT IN ('completed','cancelled','rejected'))
 UNION ALL SELECT 'intake-form',f.id,to_jsonb(f)::text FROM legal_intake_forms f,scope WHERE f.company_id=scope.company_id AND f.state='draft'
 UNION ALL SELECT 'retention-policy',p.id,to_jsonb(p)::text FROM legal_retention_policies p,scope WHERE p.company_id=scope.company_id AND p.state='draft'
 ORDER BY kind,id,fact`).bind(input.companyId,input.now).all()).results;
 return {companyId:input.companyId,packageId:input.packageId,moduleId:'legal',observedAt:input.now,revision:createHash('sha256').update(JSON.stringify({companyId:input.companyId,packageId:input.packageId,packageVersion:input.packageVersion,facts})).digest('hex'),outstanding:facts.length};
}
