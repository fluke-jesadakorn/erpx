import 'server-only';
import type {WorkContext} from '@erpx/server-identity_access/public/authorization';
import type {SqlDatabase} from '@erpx/server-runtime/public/runtime/types';
import {foundationAttachmentOrganizationSourceLocks,foundationAttachmentOrganizationSourceImage} from '@erpx/server-organization/public/attachment-read-sources';

export function isCompanyReleaseAttachmentReadTarget(target:string){return target==='company_release_acceptance';}
export function companyReleaseAttachmentReadSourceLocks(database:SqlDatabase){return [...foundationAttachmentOrganizationSourceLocks(database),database.prepare('LOCK TABLE company_release_acceptances,company_release_gate_reviews,company_release_events,company_release_revisions IN SHARE ROW EXCLUSIVE MODE')];}
/** The release-child projection shared by the reviews, events and revision
 * branches of the attachment source image. Kept here once so this parameterized
 * family has one owner-local definition; each caller supplies its table, alias
 * and ordering. `aggregate=false` renders the scalar single-revision row instead
 * of a jsonb aggregate. Rendered SQL is byte-identical to the former inline copies. */
export function releaseChildAggSql(table:string,alias:string,order:string,aggregate=true){
 const projection=aggregate?`jsonb_agg(to_jsonb(${alias}) ORDER BY ${order})`:`to_jsonb(${alias})`;
 const predicate=aggregate?`${alias}.release_id=acceptance.id AND ${alias}.company_id=acceptance.company_id`:`${alias}.company_id=acceptance.company_id`;
 return `SELECT ${projection} FROM ${table} ${alias} WHERE ${predicate}`;
}
/** The Company's original release evidence is not Platform-provider authority. */
export async function companyReleaseAttachmentReadSourceImage(database:SqlDatabase,actor:WorkContext,type:string,id:string){
 if(!isCompanyReleaseAttachmentReadTarget(type))return null;
 const source=await database.prepare(`SELECT jsonb_build_object('acceptance',to_jsonb(acceptance),
 'reviews',(${releaseChildAggSql('company_release_gate_reviews','review','review.criterion_id')}),
 'events',(${releaseChildAggSql('company_release_events','event','event.version,event.id')}),
 'revision',(${releaseChildAggSql('company_release_revisions','revision','',false)}))::text image FROM company_release_acceptances acceptance WHERE acceptance.id=? AND acceptance.company_id=?`).bind(id,actor.companyId).first<string>('image');
 return source?JSON.stringify({source,company:await foundationAttachmentOrganizationSourceImage(database,actor.companyId)}):null;
}
