import 'server-only';
import { randomUUID } from 'node:crypto';
import type {WorkContext} from '@erpx/identity-access-server/public/authorization';
import type {SqlDatabase} from '@erpx/platform-runtime-server/public/runtime/types';
import {brandAttachmentOrganizationSourceLocks} from '@erpx/organization-server/public/brand-attachment-read-sources';
import {nativeBrandAttachmentImage,nativeBrandOrganizationImage} from './native-brand-attachment-statement';

export function isBrandAttachmentReadTarget(target:string){return target==='holding_brand'||target==='branch_brand'||target==='company_storefront_brand';}
export function brandAttachmentReadSourceLocks(database:SqlDatabase){return [...brandAttachmentOrganizationSourceLocks(database),database.prepare('LOCK TABLE organization_brand_profiles,storefront_app_brands IN SHARE ROW EXCLUSIVE MODE')];}
export async function brandAttachmentReadSourceImage(database:SqlDatabase,actor:WorkContext,type:string,id:string){
 if(!isBrandAttachmentReadTarget(type))return null;
 const key=randomUUID(),now=Date.now(),target=type as 'holding_brand'|'branch_brand'|'company_storefront_brand';
 const brand=await nativeBrandAttachmentImage(database,actor,target,id,key,now);
 return JSON.stringify({brand,organization:await nativeBrandOrganizationImage(database,actor,target,id,key,now)});
}
