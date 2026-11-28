import 'server-only';
import type {WorkContext} from '@erpx/identity-access-server/public/authorization';
import type {SqlDatabase} from '@erpx/platform-runtime-server/public/runtime/types';
import {auditAggSql} from './read-tail-family-r38';

/** Native sanitized review/migration evidence, with the exact optional directory
 * connection. This private witness grants neither IAM nor directory authority. */
export function isIamAttachmentReadTarget(target:string){return target==='iam_review'||target==='iam_migration';}
export function iamAttachmentReadSourceLocks(database:SqlDatabase){return [database.prepare('LOCK TABLE iam_reviews,iam_migrations,iam_connections,iam_connection_previews,iam_mappings,iam_audit IN SHARE ROW EXCLUSIVE MODE')];}
export async function iamAttachmentReadSourceImage(database:SqlDatabase,actor:WorkContext,type:string,id:string){
 if(!isIamAttachmentReadTarget(type))return null;
 if(type==='iam_review')return database.prepare(`SELECT jsonb_build_object('review',to_jsonb(review),'audit',${auditAggSql('review')})::text image FROM iam_reviews review WHERE review.id=? AND review.company_id=?`).bind(id,actor.companyId).first<string>('image');
 return database.prepare(`SELECT jsonb_build_object('migration',to_jsonb(migration),'connection',(SELECT to_jsonb(connection) FROM iam_connections connection WHERE connection.id=migration.connection_id AND connection.company_id=migration.company_id),
 'previews',(SELECT jsonb_agg(to_jsonb(preview) ORDER BY preview.version) FROM iam_connection_previews preview WHERE preview.connection_id=migration.connection_id),
 'mappings',(SELECT jsonb_agg(to_jsonb(mapping) ORDER BY mapping.id) FROM iam_mappings mapping WHERE mapping.connection_id=migration.connection_id AND mapping.company_id=migration.company_id),
 'audit',${auditAggSql('migration')})::text image FROM iam_migrations migration WHERE migration.id=? AND migration.company_id=?`).bind(id,actor.companyId).first<string>('image');
}
