import 'server-only';
import type {WorkContext} from '@erpx/server-identity_access/public/authorization';
import type {SqlDatabase} from '@erpx/server-runtime/public/runtime/types';
import {nativeOrgOne} from './native-organization-owner-write';

/** Company foundation facts only; this point witness never grants Organization
 * management or provider access. All native rows remain inside the read unit. */
export function isOrganizationAttachmentReadTarget(target:string){return target==='organization_node'||target==='organization_change';}
export function organizationAttachmentReadSourceLocks(database:SqlDatabase){return [database.prepare('LOCK TABLE organization_nodes,organization_node_changes,organization_node_change_events IN SHARE ROW EXCLUSIVE MODE')];}
export function foundationAttachmentOrganizationSourceLocks(database:SqlDatabase){return [database.prepare('LOCK TABLE organization_nodes IN SHARE ROW EXCLUSIVE MODE')];}
export function foundationAttachmentOrganizationSourceImage(database:SqlDatabase,companyId:string){return nativeOrgOne<{image:string}>(database,companyId,'attachment_organization_source_image',[companyId]).then(row=>row?.image??null);}
export async function organizationAttachmentReadSourceImage(database:SqlDatabase,actor:WorkContext,type:string,id:string){
 if(!isOrganizationAttachmentReadTarget(type))return null;
 const kind=type==='organization_node'?'attachment_source_node_image':'attachment_source_change_image';
 return nativeOrgOne<{image:string}>(database,actor.companyId,kind,[id,actor.companyId]).then(row=>row?.image??null);
}
