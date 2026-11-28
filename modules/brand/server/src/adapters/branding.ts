import "server-only";
import sharp from "sharp";
import type { WorkContext } from "@erpx/identity-access-server/public/authorization";
import { companyAuthority } from "@erpx/operations-server/public/staff-policy";
import type { SqlDatabase } from "@erpx/platform-runtime-server/public/runtime/types";
import { BusinessRuleError } from "@erpx/platform-shared-server/index";
import { normalizeBrandMedia } from "@erpx/brand-server/application/branding";
import type { BrandOwnerType } from "@erpx/brand-server/domain/branding";
import {
  nativeBrandProfileEvidence,
  nativeResolveDocumentBrand,
  nativeResolveWorkspaceBrand,
} from "./native-brand-presentation";
import { nativeBrandManagementBranch, nativeBrandManagementCompanyExists } from "./native-brand-management";
export { documentBrandModes } from "@erpx/brand-server/domain/branding";
export type { DocumentBrandMode,BrandVariant,WorkspaceBrand,StorefrontAppBrand,DocumentBrand } from "@erpx/brand-server/domain/branding";
const image={async metadata(buffer:ArrayBuffer){return sharp(Buffer.from(buffer),{failOn:"warning"}).metadata();},async encode(buffer:ArrayBuffer){return sharp(Buffer.from(buffer),{failOn:"warning"}).rotate().resize({width:4096,height:4096,fit:"inside",withoutEnlargement:true}).webp({quality:92,alphaQuality:100}).toBuffer();}};
/** Native active-Branch guard. No TypeScript statement remains on this path. */
const branch=(db:SqlDatabase,actor:WorkContext,id:string)=>nativeBrandManagementBranch(db,actor,id);
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
/** Native active-Company guard. No TypeScript statement remains on this path. */
async function companyExists(db:SqlDatabase,actor:WorkContext,id:string){return nativeBrandManagementCompanyExists(db,actor,id);}
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
