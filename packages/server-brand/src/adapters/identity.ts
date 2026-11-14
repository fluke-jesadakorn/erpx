import { commercialOwnerProjectionSources as filesProjectionSources } from "@erpx/server-files/public/commercial-owner-projection-sources";
import { commercialOwnerProjectionSources as identity_accessProjectionSources } from "@erpx/server-identity_access/public/commercial-owner-projection-sources";
import { readBrandIdentityWorkspace, readPublishedBrandIdentity, type BrandIdentityEvent } from "@erpx/server-brand/application/identity-read";
import { type BrandDraft as Draft, type BrandPublished as Published, type BrandCategoryLock as Lock } from "@erpx/server-brand/domain/identity";
import "server-only";
import type {WorkContext} from "@erpx/server-identity_access/public/authorization";
import type {SqlDatabase} from "@erpx/server-runtime/public/runtime/types";
import {hasPermission} from "@erpx/feature-identity_access/lib/permissions";
import {hasModuleAccess} from "@erpx/catalogue/module-access";
import {holdingResources} from "../../../../packages/server-runtime/src/public/runtime";
import {requirePermission} from "@erpx/server-identity_access/public/iam-permissions";
import {readStorefrontAppBrand} from "@erpx/server-brand/public/branding";
import {ApiError} from "@erpx/server-runtime/public/http";

// Brand identity mutations are owned by the native Rust command/HTTP path.
// Only the published and workspace projections remain here for their live
// Work Projection, Records record-detail and branding read consumers.
export function publishedBrandIdentity(db:SqlDatabase,companyId:string){
 return readPublishedBrandIdentity({
  async available(){return !!await db.prepare("SELECT to_regclass('brand_identity_active') relation").first("relation");},
  active:()=>db.prepare("SELECT v.*,a.version active_version FROM brand_identity_active a JOIN brand_identity_versions v ON v.id=a.identity_version_id AND v.company_id=a.company_id WHERE a.company_id=?").bind(companyId).first<Published&{active_version:number}>(),
 });
}
async function authority(db:SqlDatabase,a:WorkContext){
 requirePermission(a,"records.read");if(!hasModuleAccess(a.modules,"brand"))throw new ApiError(403,"MODULE_FORBIDDEN","Brand identity is unavailable to the selected role.");
 const current=await db.prepare(`SELECT 1 FROM (${identity_accessProjectionSources.assignments}) g WHERE g.id=? AND g.user_id=? AND g.role=? AND g.company_id=? AND iam_assignment_active(g.id,?) AND iam_principal_active(g.user_id,g.company_id,?) AND iam_role_allows(g.role,g.company_id,?)`).bind(a.assignmentId,a.userId,a.role,a.companyId,Date.now(),Date.now(),"records.read").first();
 if(!current)throw new ApiError(403,"BRAND_AUTHORITY_CHANGED","Your current Brand authority is unavailable.");
}
async function lockState(db:SqlDatabase,a:WorkContext){return (await db.prepare("SELECT * FROM brand_category_locks WHERE company_id=? ORDER BY category").bind(a.companyId).all<Lock>()).results;}
export async function brandIdentityWorkspace(a:WorkContext){
 const {write}=await holdingResources(a.holdingId);
 return readBrandIdentityWorkspace({actorId:a.userId,canManage:hasPermission(a,"organization.manage")&&!a.branchId&&!a.departmentId,canLock:hasPermission(a,"company.owner")&&!a.branchId&&!a.departmentId,
  authorize:()=>authority(write,a),
  async drafts(canManage){return (await write.prepare("SELECT * FROM brand_identity_drafts WHERE company_id=? AND (?::boolean OR created_by=?) ORDER BY updated_at DESC").bind(a.companyId,canManage,a.userId).all<Draft>()).results;},
  async versions(){return (await write.prepare("SELECT * FROM brand_identity_versions WHERE company_id=? ORDER BY revision DESC").bind(a.companyId).all<Published>()).results;},locks:()=>lockState(write,a),
  async history(){return (await write.prepare("SELECT * FROM brand_identity_events WHERE company_id=? ORDER BY created_at DESC,id LIMIT 200").bind(a.companyId).all<BrandIdentityEvent>()).results;},
  active:()=>publishedBrandIdentity(write,a.companyId),legacy:()=>readStorefrontAppBrand(write,a),
  async media(){return (await write.prepare(`SELECT id,alt_text name FROM (${filesProjectionSources.attachment_publications}) attachment_publications WHERE company_id=? AND target_type='company_storefront_brand' AND target_id=? AND state='approved' ORDER BY approved_at DESC`).bind(a.companyId,a.companyId).all<{id:string;name:string}>()).results;},
 });
}
