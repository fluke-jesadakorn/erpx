import 'server-only';
import type {PostgresDatabase} from '@erpx/server-runtime/public/runtime/postgres';
import type {SqlDatabase} from '@erpx/server-runtime/public/runtime/types';
import {packageSettlementSource,type PackageSettlementSourceLock} from '@erpx/server-commercial/public/package-settlement-sources';
import {scanPendingSql} from './scan-pending-dedupe-sql';
import {PACKAGE_OBLIGATION_SCOPE_SQL} from './read-tail-dedupe-sql';
import {packageFileLinkJoinSql} from './read-tail-r29-source-sql';
import {PACKAGE_FILE_LINK_TARGET_HEAD_SQL} from './read-tail-r48-source-sql';
const sources=new WeakMap<SqlDatabase,string>();
export async function lockMarketingFilePackageObligationSources(owner:PostgresDatabase,lock:PackageSettlementSourceLock){const {database,companyId}=packageSettlementSource(owner,lock);await database.prepare('LOCK TABLE attachment_sets,attachment_links,attachments,upload_intents,attachment_scan_attempts,attachment_publications,attachment_events IN SHARE MODE').run();sources.set(database,companyId);}
/** Public publication and outstanding material are clearing obligations. Bound
 * private evidence, withdrawn drafts and Legal-held bytes remain retained. */
export async function marketingFilePackageObligationFacts(owner:PostgresDatabase,database:SqlDatabase,companyId:string){
 owner.assertActiveTransaction(database);if(sources.get(database)!==companyId)throw Error('MARKETING_FILES_PACKAGE_OBLIGATION_SOURCE_REQUIRED');
 return (await database.prepare(`${PACKAGE_OBLIGATION_SCOPE_SQL}s.* FROM attachment_sets s,scope WHERE s.company_id=scope.company_id AND s.target_type IN ('marketing_asset','marketing_campaign'))
 ${PACKAGE_FILE_LINK_TARGET_HEAD_SQL}type,s.target_id,to_jsonb(l)||jsonb_build_object('set',to_jsonb(s),'source',to_jsonb(a),'upload',to_jsonb(u)) fact,
 COALESCE(l.state='draft' OR l.state='bound' AND (s.state<>'bound' OR a.id IS NULL OR a.company_id<>s.company_id OR a.source_state='quarantined' OR ${scanPendingSql('a')}),true) pending${packageFileLinkJoinSql('public-creative')}p.id,p.target_type,p.target_id,to_jsonb(p)||jsonb_build_object('set',to_jsonb(s),'link',to_jsonb(l)),p.state='approved' OR p.state='draft' AND COALESCE(l.state IN ('draft','bound'),false)
 FROM attachment_publications p CROSS JOIN scope LEFT JOIN attachment_links l ON l.id=p.source_link_id LEFT JOIN sets s ON s.id=l.set_id
 WHERE p.company_id=scope.company_id AND p.target_type IN ('marketing_asset','marketing_campaign') ORDER BY kind,id`).bind(companyId).all<{kind:string;id:string;target_type:string;target_id:string;fact:unknown;pending:boolean}>()).results;
}
