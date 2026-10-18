import 'server-only';
import type {PostgresDatabase} from '../../runtime/public/runtime/postgres';
import type {SqlDatabase} from '../../runtime/public/runtime/types';
import {packageSettlementSource,type PackageSettlementSourceLock} from '../../commercial/public/package-settlement-sources';
const sources=new WeakMap<SqlDatabase,string>();
export async function lockEcommerceFilePackageObligationSources(owner:PostgresDatabase,lock:PackageSettlementSourceLock){const {database,companyId}=packageSettlementSource(owner,lock);await database.prepare('LOCK TABLE attachment_sets,attachment_links,attachments,upload_intents,attachment_scan_attempts,attachment_publications,attachment_events IN SHARE MODE').run();sources.set(database,companyId);}
export async function ecommerceFilePackageObligationFacts(owner:PostgresDatabase,database:SqlDatabase,companyId:string){
 owner.assertActiveTransaction(database);if(sources.get(database)!==companyId)throw Error('ECOMMERCE_FILES_PACKAGE_OBLIGATION_SOURCE_REQUIRED');
 return (await database.prepare(`WITH scope AS(SELECT ?::text company_id),sets AS MATERIALIZED(SELECT s.* FROM attachment_sets s,scope WHERE s.company_id=scope.company_id AND s.target_type='ecommerce_listing')
 SELECT 'file-link' kind,l.id,s.target_id,to_jsonb(l)||jsonb_build_object('set',to_jsonb(s),'source',to_jsonb(a),'upload',to_jsonb(u)) fact,COALESCE(l.state='draft' OR l.state='bound' AND (s.state<>'bound' OR a.id IS NULL OR a.company_id<>s.company_id OR a.source_state='quarantined' OR EXISTS(SELECT 1 FROM attachment_scan_attempts scan WHERE scan.attachment_id=a.id AND scan.state IN ('pending','running','error'))),true) pending
 FROM sets s JOIN attachment_links l ON l.set_id=s.id LEFT JOIN attachments a ON a.id=l.attachment_id LEFT JOIN upload_intents u ON u.id=l.upload_intent_id
 UNION ALL SELECT 'publication',p.id,p.target_id,to_jsonb(p)||jsonb_build_object('set',to_jsonb(s),'link',to_jsonb(l)),p.state='approved' OR p.state='draft' AND COALESCE(l.state IN ('draft','bound'),false) FROM attachment_publications p CROSS JOIN scope LEFT JOIN attachment_links l ON l.id=p.source_link_id LEFT JOIN sets s ON s.id=l.set_id WHERE p.company_id=scope.company_id AND p.target_type='ecommerce_listing' ORDER BY kind,id`).bind(companyId).all<{kind:string;id:string;target_id:string;fact:unknown;pending:boolean}>()).results;
}
