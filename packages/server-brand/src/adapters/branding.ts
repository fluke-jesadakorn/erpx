import { commercialOwnerProjectionSources as organizationProjectionSources } from "@erpx/server-organization/public/commercial-owner-projection-sources";
import "server-only";
import sharp from "sharp";
import type { WorkContext } from "@erpx/server-identity_access/public/authorization";
import { companyAuthority } from "@erpx/server-operations/public/staff-policy";
import type { SqlDatabase } from "@erpx/server-runtime/public/runtime/types";
import { BusinessRuleError } from "@erpx/server-shared/index";
import { normalizeBrandMedia } from "@erpx/server-brand/application/branding";
import type { BrandOwnerType } from "@erpx/server-brand/domain/branding";
import {
  nativeBrandProfileEvidence,
  nativeResolveDocumentBrand,
  nativeResolveWorkspaceBrand,
} from "./native-brand-presentation";
import { brandManagementGuardHead } from "./read-tail-r1";
export { documentBrandModes } from "@erpx/server-brand/domain/branding";
export type { DocumentBrandMode,BrandVariant,WorkspaceBrand,StorefrontAppBrand,DocumentBrand } from "@erpx/server-brand/domain/branding";
const image={async metadata(buffer:ArrayBuffer){return sharp(Buffer.from(buffer),{failOn:"warning"}).metadata();},async encode(buffer:ArrayBuffer){return sharp(Buffer.from(buffer),{failOn:"warning"}).rotate().resize({width:4096,height:4096,fit:"inside",withoutEnlargement:true}).webp({quality:92,alphaQuality:100}).toBuffer();}};
const branch=(db:SqlDatabase,actor:WorkContext,id:string)=>db.prepare(brandManagementGuardHead("id,display_name",organizationProjectionSources.organization_nodes,"company_id=? AND kind='branch' AND status='active'")).bind(id,actor.companyId).first<{id:string;display_name:string}>();
/** Native workspace brand for the selected scope. The Brand owner resolves the
 * Holding label, media URL precedence and document mode on the current Holding
 * transaction; no TypeScript Brand SQL read remains on this path. */
export async function resolveWorkspaceBrand(a:WorkContext){
 return nativeResolveWorkspaceBrand(a);
}
/** Native document brand choice. Holding mode is the no-Branch default and a
 * requested Branch mode is ignored for a document without a Branch. */
export async function resolveDocumentBrand(a:WorkContext,scope:{branchId:string|null;requestedMode?:string|null}){
 return nativeResolveDocumentBrand(a,scope);
}
async function companyExists(db:SqlDatabase,actor:WorkContext,id:string){return !!await db.prepare(brandManagementGuardHead("1",organizationProjectionSources.organization_nodes,"kind='company' AND status='active'")).bind(id).first();}
export async function requireBrandManagement(db:SqlDatabase,a:WorkContext,type:BrandOwnerType,id:string){
 const allowed=type==="holding_brand"?id===a.holdingId&&await companyAuthority(db,a,["holding_owner"])
  :type==="branch_brand"?await companyAuthority(db,a,"organization.manage")&&!!await branch(db,a,id)
  :id===a.companyId&&await companyAuthority(db,a,"organization.manage")&&await companyExists(db,a,id);
 if(!allowed)throw new BusinessRuleError(403,"BRAND_MANAGEMENT_FORBIDDEN","A Holding owner or authorized organization administrator is required.");
}
export function normalizeBrandImage(buffer:ArrayBuffer){return normalizeBrandMedia(image,buffer);}
/** Native executive Brand profile evidence row for one active Branch. */
export async function readBrandProfileEvidence(a:WorkContext,branchId:string){
 return nativeBrandProfileEvidence(a,branchId);
}
