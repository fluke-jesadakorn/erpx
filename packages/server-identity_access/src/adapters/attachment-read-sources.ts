import 'server-only';
import type {WorkContext} from '@erpx/server-identity_access/public/authorization';
import type {SqlDatabase} from '@erpx/server-runtime/public/runtime/types';

/** Native sanitized review/migration evidence, with the exact optional directory
 * connection. This private witness grants neither IAM nor directory authority. */
export function isIamAttachmentReadTarget(target:string){return target==='iam_review'||target==='iam_migration';}
export function iamAttachmentReadSourceLocks(database:SqlDatabase){return [database.prepare('LOCK TABLE iam_reviews,iam_migrations,iam_connections,iam_connection_previews,iam_mappings,iam_audit IN SHARE ROW EXCLUSIVE MODE')];}
export async function iamAttachmentReadSourceImage(database:SqlDatabase,actor:WorkContext,type:string,id:string){
 if(!isIamAttachmentReadTarget(type))return null;
 if(type==='iam_review')return database.prepare(`SELECT jsonb_build_object('review',to_jsonb(review),'audit',(SELECT jsonb_agg(to_jsonb(event) ORDER BY event.created_at,event.id) FROM iam_audit event WHERE event.company_id=review.company_id AND event.target_id=review.id))::text image FROM iam_reviews review WHERE review.id=? AND review.company_id=?`).bind(id,actor.companyId).first<string>('image');
 return database.prepare(`SELECT jsonb_build_object('migration',to_jsonb(migration),'connection',(SELECT to_jsonb(connection) FROM iam_connections connection WHERE connection.id=migration.connection_id AND connection.company_id=migration.company_id),
 'previews',(SELECT jsonb_agg(to_jsonb(preview) ORDER BY preview.version) FROM iam_connection_previews preview WHERE preview.connection_id=migration.connection_id),
 'mappings',(SELECT jsonb_agg(to_jsonb(mapping) ORDER BY mapping.id) FROM iam_mappings mapping WHERE mapping.connection_id=migration.connection_id AND mapping.company_id=migration.company_id),
 'audit',(SELECT jsonb_agg(to_jsonb(event) ORDER BY event.created_at,event.id) FROM iam_audit event WHERE event.company_id=migration.company_id AND event.target_id=migration.id))::text image FROM iam_migrations migration WHERE migration.id=? AND migration.company_id=?`).bind(id,actor.companyId).first<string>('image');
}
