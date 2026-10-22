import 'server-only';
import type {WorkContext} from '@erpx/server-identity_access/public/authorization';
import type {SqlDatabase} from '@erpx/server-runtime/public/runtime/types';
import {foundationAttachmentOrganizationSourceLocks,foundationAttachmentOrganizationSourceImage} from '@erpx/server-organization/public/attachment-read-sources';

export function isCompanyProvisioningAttachmentReadTarget(target:string){return target==='company_provisioning';}
export function companyProvisioningAttachmentReadSourceLocks(database:SqlDatabase){return [...foundationAttachmentOrganizationSourceLocks(database),database.prepare('LOCK TABLE company_provisioning,company_provisioning_events IN SHARE ROW EXCLUSIVE MODE')];}
/** targetId is the actual Company primary key, never a guessed operation ID. */
export async function companyProvisioningAttachmentReadSourceImage(database:SqlDatabase,actor:WorkContext,type:string,id:string){
 if(!isCompanyProvisioningAttachmentReadTarget(type))return null;
 const source=await database.prepare(`SELECT jsonb_build_object('provisioning',to_jsonb(provisioning),'events',(SELECT jsonb_agg(to_jsonb(event) ORDER BY event.version,event.id) FROM company_provisioning_events event WHERE event.company_id=provisioning.company_id AND event.holding_id=provisioning.holding_id))::text image FROM company_provisioning provisioning WHERE provisioning.company_id=? AND provisioning.company_id=?`).bind(id,actor.companyId).first<string>('image');
 return source?JSON.stringify({source,company:await foundationAttachmentOrganizationSourceImage(database,actor.companyId)}):null;
}
