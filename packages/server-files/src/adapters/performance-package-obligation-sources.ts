import 'server-only';
import type {PostgresDatabase} from '@erpx/server-runtime/public/runtime/postgres';
import type {SqlDatabase} from '@erpx/server-runtime/public/runtime/types';
import {packageSettlementSource,type PackageSettlementSourceLock} from '@erpx/server-commercial/public/package-settlement-sources';
const sources=new WeakMap<SqlDatabase,string>();
export async function lockPerformanceFilePackageObligationSources(owner:PostgresDatabase,lock:PackageSettlementSourceLock){
 const {database,companyId}=packageSettlementSource(owner,lock);await database.prepare('LOCK TABLE attachment_sets,attachment_links,attachments,upload_intents,attachment_scan_attempts,attachment_publications,attachment_events IN SHARE MODE').run();sources.set(database,companyId);
}
/** Private bound evidence and Legal-held bytes are retained history. Actual
 * unfinished uploads/scans and any existing public approval remain duties. */
export async function performanceFilePackageObligationFacts(owner:PostgresDatabase,database:SqlDatabase,companyId:string){
 owner.assertActiveTransaction(database);if(sources.get(database)!==companyId)throw Error('PERFORMANCE_FILES_PACKAGE_OBLIGATION_SOURCE_REQUIRED');
 return (await database.prepare(`WITH scope AS(SELECT ?::text company_id),sets AS MATERIALIZED(SELECT s.* FROM attachment_sets s,scope WHERE s.company_id=scope.company_id AND s.target_type='hr_evaluation_assessment')
 SELECT 'file-set' kind,s.id,s.target_id,to_jsonb(s) image,s.state='draft' pending FROM sets s
 UNION ALL SELECT 'file-link',l.id,s.target_id,to_jsonb(l)||jsonb_build_object('set',to_jsonb(s),'attachment',to_jsonb(a),'upload',to_jsonb(u),'scans',(SELECT jsonb_agg(to_jsonb(scan) ORDER BY scan.id) FROM attachment_scan_attempts scan WHERE scan.attachment_id=a.id)),
 COALESCE(l.state='draft' OR l.state='bound' AND (s.state<>'bound' OR a.id IS NULL OR a.company_id IS DISTINCT FROM s.company_id OR a.source_state='quarantined' OR EXISTS(SELECT 1 FROM attachment_scan_attempts scan WHERE scan.attachment_id=a.id AND scan.state IN ('pending','running','error'))),true)
 FROM sets s JOIN attachment_links l ON l.set_id=s.id LEFT JOIN attachments a ON a.id=l.attachment_id LEFT JOIN upload_intents u ON u.id=l.upload_intent_id
 UNION ALL SELECT 'publication',p.id,p.target_id,to_jsonb(p),p.state IN ('approved','draft') FROM attachment_publications p,scope WHERE p.company_id=scope.company_id AND p.target_type='hr_evaluation_assessment'
 ORDER BY kind,id`).bind(companyId).all<{kind:string;id:string;target_id:string;image:unknown;pending:boolean}>()).results;
}
