import 'server-only';
import type {WorkContext} from '@erpx/server-identity_access/public/authorization';
import type {SqlDatabase} from '@erpx/server-runtime/public/runtime/types';
import {foundationAttachmentOrganizationSourceLocks,foundationAttachmentOrganizationSourceImage} from '@erpx/server-organization/public/attachment-read-sources';
import {nativeCompanyProvisioningAttachmentImage} from './native-owner-read';

export function isCompanyProvisioningAttachmentReadTarget(target:string){return target==='company_provisioning';}
export function companyProvisioningAttachmentReadSourceLocks(database:SqlDatabase){return [...foundationAttachmentOrganizationSourceLocks(database),database.prepare('LOCK TABLE company_provisioning,company_provisioning_events IN SHARE ROW EXCLUSIVE MODE')];}
/** targetId is the actual Company primary key, never a guessed operation ID. */
export async function companyProvisioningAttachmentReadSourceImage(database:SqlDatabase,actor:WorkContext,type:string,id:string){
 if(!isCompanyProvisioningAttachmentReadTarget(type))return null;
 const source=await nativeCompanyProvisioningAttachmentImage(database,id,actor.companyId);
 return source?JSON.stringify({source,company:await foundationAttachmentOrganizationSourceImage(database,actor.companyId)}):null;
}
