import 'server-only';
import type {WorkContext} from '@erpx/server-identity_access/public/authorization';
import type {SqlDatabase} from '@erpx/server-runtime/public/runtime/types';
import {foundationAttachmentOrganizationSourceLocks,foundationAttachmentOrganizationSourceImage} from '@erpx/server-organization/public/attachment-read-sources';
import {readNativeReleaseChildImage} from './native-lane-33/client';

export function isCompanyReleaseAttachmentReadTarget(target:string){return target==='company_release_acceptance';}
export function companyReleaseAttachmentReadSourceLocks(database:SqlDatabase){return [...foundationAttachmentOrganizationSourceLocks(database),database.prepare('LOCK TABLE company_release_acceptances,company_release_gate_reviews,company_release_events,company_release_revisions IN SHARE ROW EXCLUSIVE MODE')];}
/** The Company's original release evidence is not Platform-provider authority.
 * The release-owned acceptance/child image is read by the native owner; only
 * the cross-owner organization company projection remains a TypeScript read. */
export async function companyReleaseAttachmentReadSourceImage(database:SqlDatabase,actor:WorkContext,type:string,id:string){
  if(!isCompanyReleaseAttachmentReadTarget(type))return null;
  const source=await readNativeReleaseChildImage(actor,id);
  return source?JSON.stringify({source,company:await foundationAttachmentOrganizationSourceImage(database,actor.companyId)}):null;
}
