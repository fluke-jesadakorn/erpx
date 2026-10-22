import 'server-only';
import type {WorkContext} from '@erpx/server-identity_access/public/authorization';
import type {SqlDatabase} from '@erpx/server-runtime/public/runtime/types';
import {brandAttachmentOrganizationSourceImage,brandAttachmentOrganizationSourceLocks} from '@erpx/server-organization/public/brand-attachment-read-sources';

export function isBrandAttachmentReadTarget(target:string){return target==='holding_brand'||target==='branch_brand'||target==='company_storefront_brand';}
export function brandAttachmentReadSourceLocks(database:SqlDatabase){return [...brandAttachmentOrganizationSourceLocks(database),database.prepare('LOCK TABLE organization_brand_profiles,storefront_app_brands IN SHARE ROW EXCLUSIVE MODE')];}
export async function brandAttachmentReadSourceImage(database:SqlDatabase,actor:WorkContext,type:string,id:string){
 if(!isBrandAttachmentReadTarget(type))return null;
 const brand=await database.prepare(`SELECT jsonb_build_object('profile',(SELECT to_jsonb(profile) FROM organization_brand_profiles profile WHERE profile.scope_kind=? AND profile.scope_id=?),
 'storefront',(SELECT to_jsonb(storefront) FROM storefront_app_brands storefront WHERE storefront.company_id=? AND ?::text='company_storefront_brand'))::text image`).bind(type==='holding_brand'?'holding':'branch',type==='company_storefront_brand'?null:id,actor.companyId,type).first<string>('image');
 return JSON.stringify({brand,organization:await brandAttachmentOrganizationSourceImage(database,actor.companyId,type==='branch_brand'?id:null)});
}
