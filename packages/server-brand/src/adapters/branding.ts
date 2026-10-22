import { commercialOwnerProjectionSources as commerceProjectionSources } from "@erpx/server-commerce/public/commercial-owner-projection-sources";
import { commercialOwnerProjectionSources as financeProjectionSources } from "@erpx/server-finance/public/commercial-owner-projection-sources";
import { commercialOwnerProjectionSources as identity_accessProjectionSources } from "@erpx/server-identity_access/public/commercial-owner-projection-sources";
import { commercialOwnerProjectionSources as inventoryProjectionSources } from "@erpx/server-inventory/public/commercial-owner-projection-sources";
import { commercialOwnerProjectionSources as organizationProjectionSources } from "@erpx/server-organization/public/commercial-owner-projection-sources";
import { commercialOwnerProjectionSources as people_payrollProjectionSources } from "@erpx/server-people_payroll/public/commercial-owner-projection-sources";
import "server-only";
import sharp from "sharp";
import { workspaceBase } from "@erpx/feature-work_projection/lib/workspace";
import { publicScope, type WorkContext } from "@erpx/server-identity_access/public/authorization";
import {assertion} from "@erpx/server-workflow/public/command-assertions";
import {executeCommand} from "@erpx/server-workflow/public/commands";
import { sha256 } from "@erpx/server-runtime/public/crypto";
import { ApiError } from "@erpx/server-runtime/public/http";
import { companyAuthority } from "@erpx/server-operations/public/staff-policy";
import type { FileStore, SqlDatabase } from "@erpx/server-runtime/public/runtime/types";
import { attachmentsV2Readable, attachmentsV2ReadOnly } from "@erpx/server-files/public/attachments/feature";
import { prepareBrandMediaUpload, prepareBrandMediaRemoval, brandMediaObjectKey } from "@erpx/server-files/public/brand-media";
import { publishedBrandIdentity } from "@erpx/server-brand/adapters/identity";
import { BusinessRuleError } from "@erpx/server-shared/index";
import { createBrandingApplication,normalizeBrandMedia,type BrandingPorts,type BrandMutation,type BrandOwnerType,type BrandRecordKind } from "@erpx/server-brand/application/branding";
import type { BrandProfileRow,BrandStorefrontRow,BrandVariant,BrandProfileInput,StorefrontBrandInput } from "@erpx/server-brand/domain/branding";
export { documentBrandModes } from "@erpx/server-brand/domain/branding";
export type { DocumentBrandMode,BrandVariant,WorkspaceBrand,StorefrontAppBrand,DocumentBrand } from "@erpx/server-brand/domain/branding";
const image={async metadata(buffer:ArrayBuffer){return sharp(Buffer.from(buffer),{failOn:"warning"}).metadata();},async encode(buffer:ArrayBuffer){return sharp(Buffer.from(buffer),{failOn:"warning"}).rotate().resize({width:4096,height:4096,fit:"inside",withoutEnlargement:true}).webp({quality:92,alphaQuality:100}).toBuffer();}};
function application(db:SqlDatabase,actor:WorkContext,files?:FileStore){
 const profile=(kind:"holding"|"branch",id:string)=>db.prepare("SELECT * FROM organization_brand_profiles WHERE scope_kind=? AND scope_id=?").bind(kind,id).first<BrandProfileRow>();
 const ports:BrandingPorts<Buffer>={actor,clock:{now:()=>Date.now()},legacyAttachmentsReadable:attachmentsV2Readable(),
 urls:{attachment:id=>id?`/api/v1${workspaceBase(publicScope(actor))}/attachments/${encodeURIComponent(id)}/content`:null,publication:id=>id?`/api/public/holdings/${encodeURIComponent(actor.holdingSlug)}/media-publications/${encodeURIComponent(id)}/content`:null},
 holdingAuthority:()=>companyAuthority(db,actor,["holding_owner"]),organizationAuthority:()=>companyAuthority(db,actor,"organization.manage"),evidenceAuthority:()=>companyAuthority(db,actor),
 assertMediaWritable(){if(attachmentsV2ReadOnly())throw new ApiError(503,"ATTACHMENTS_READ_ONLY","Brand media changes are disabled during attachment rollback. Existing approved media remains readable.");},
 company:()=>db.prepare(`SELECT display_name FROM (${organizationProjectionSources.organization_nodes}) organization_nodes WHERE id=? AND kind='company' AND status='active'`).bind(actor.companyId).first<{display_name:string}>(),
 branch:id=>db.prepare(`SELECT id,display_name FROM (${organizationProjectionSources.organization_nodes}) organization_nodes WHERE id=? AND company_id=? AND kind='branch' AND status='active'`).bind(id,actor.companyId).first<{id:string;display_name:string}>(),
 async companyExists(id){return !!await db.prepare(`SELECT 1 FROM (${organizationProjectionSources.organization_nodes}) organization_nodes WHERE id=? AND kind='company' AND status='active'`).bind(id).first();},profile,
 async branches(){return (await db.prepare(`SELECT id,display_name FROM (${organizationProjectionSources.organization_nodes}) organization_nodes WHERE company_id=? AND kind='branch' AND status='active' ORDER BY display_name,id`).bind(actor.companyId).all<{id:string;display_name:string}>()).results;},
 async profiles(){return (await db.prepare("SELECT * FROM organization_brand_profiles WHERE scope_kind='holding' OR company_id=?").bind(actor.companyId).all<BrandProfileRow>()).results;},
 storefront:()=>db.prepare(`SELECT c.slug,c.display_name,b.short_name,b.accent_color,b.logo_attachment_id,b.logo_publication_id FROM (${organizationProjectionSources.organization_nodes}) c LEFT JOIN storefront_app_brands b ON b.company_id=c.id WHERE c.id=? AND c.kind='company' AND c.status='active'`).bind(actor.companyId).first<BrandStorefrontRow>(),
 storefrontLogo:companyId=>db.prepare("SELECT logo_attachment_id FROM storefront_app_brands WHERE company_id=?").bind(companyId).first<{logo_attachment_id:string|null}>(),
 published:()=>publishedBrandIdentity(db,actor.companyId),recordBranch:(kind,id)=>recordBranchQuery(db,actor,kind,id),
 evidence:branchId=>db.prepare(`SELECT p.scope_id id,b.display_name branch_name,p.document_mode,p.updated_at,p.primary_attachment_id,p.inverse_attachment_id FROM organization_brand_profiles p JOIN (${organizationProjectionSources.organization_nodes}) b ON b.id=p.scope_id AND b.company_id=p.company_id AND b.kind='branch' AND b.status='active' WHERE p.scope_kind='branch' AND p.company_id=? AND p.scope_id=? LIMIT 1`).bind(actor.companyId,branchId).first(),image,
 checksum:bytes=>sha256(bytes.buffer.slice(bytes.byteOffset,bytes.byteOffset+bytes.byteLength) as ArrayBuffer),attachmentObjectKey:id=>brandMediaObjectKey(db,id),
 files:{async put(key,bytes,checksum){if(!files)throw new Error("Brand file port is unavailable");await files.put(key,bytes,{onlyIf:{etagDoesNotMatch:"*"},httpMetadata:{contentType:"image/webp"},customMetadata:{sha256:checksum}});},async delete(key){if(!files)throw new Error("Brand file port is unavailable");await files.delete(key);}},
 commit:(mutation,result)=>commit(db,actor,mutation,result),
 };return createBrandingApplication(ports);
}
async function recordBranchQuery(
  db: SqlDatabase,
  actor: WorkContext,
  kind: "order" | "invoice" | "payment" | "journal" | "expense",
  id: string,
): Promise<string | null> {
  if (kind === "order" || kind === "expense") {
    const table = kind === "order" ? "commerce_orders" : "finance_expenses";
    const found = await db.prepare(`SELECT branch_id FROM ${table} WHERE id=? AND company_id=?`).bind(id, actor.companyId).first<{ branch_id: string | null }>();
    return found?.branch_id ?? null;
  }
  if (kind === "invoice") {
    const found = await db.prepare(`SELECT CASE
      WHEN i.source_type='commerce_fulfillment' THEN (SELECT o.branch_id FROM (${commerceProjectionSources.commerce_fulfillments}) f JOIN (${commerceProjectionSources.commerce_orders}) o ON o.id=f.order_id WHERE f.id=i.source_id)
      WHEN i.source_type='expense_claim' THEN (SELECT e.branch_id FROM (${people_payrollProjectionSources.finance_expenses}) e WHERE e.id=i.source_id)
      ELSE NULL END branch_id FROM (${financeProjectionSources.finance_invoices}) i WHERE i.id=? AND i.company_id=?`).bind(id, actor.companyId).first<{ branch_id: string | null }>();
    return found?.branch_id ?? null;
  }
  if (kind === "payment") {
    const found = await db.prepare(`SELECT CASE WHEN COUNT(DISTINCT branch_id)=1 THEN MIN(branch_id) ELSE NULL END branch_id FROM (
      SELECT CASE WHEN i.source_type='commerce_fulfillment' THEN (SELECT o.branch_id FROM (${commerceProjectionSources.commerce_fulfillments}) f JOIN (${commerceProjectionSources.commerce_orders}) o ON o.id=f.order_id WHERE f.id=i.source_id)
        WHEN i.source_type='expense_claim' THEN (SELECT e.branch_id FROM (${people_payrollProjectionSources.finance_expenses}) e WHERE e.id=i.source_id) ELSE NULL END branch_id
      FROM (${financeProjectionSources.finance_allocations}) a JOIN (${financeProjectionSources.finance_invoices}) i ON i.id=a.invoice_id WHERE a.payment_id=? AND a.company_id=?) branded WHERE branch_id IS NOT NULL`).bind(id, actor.companyId).first<{ branch_id: string | null }>();
    return found?.branch_id ?? null;
  }
  const found = await db.prepare(`SELECT CASE
    WHEN j.source_type='expense_claim' THEN (SELECT e.branch_id FROM (${people_payrollProjectionSources.finance_expenses}) e WHERE e.id=j.source_id)
    WHEN j.source_type='stock_count' THEN (SELECT c.branch_id FROM (${inventoryProjectionSources.inventory_counts}) c WHERE c.id=j.source_id)
    ELSE NULL END branch_id FROM (${financeProjectionSources.finance_journals}) j WHERE j.id=? AND j.company_id=?`).bind(id, actor.companyId).first<{ branch_id: string | null }>();
  return found?.branch_id ?? null;
}

async function commit<Result>(db:SqlDatabase,actor:WorkContext,m:BrandMutation,result:Result):Promise<Result>{
 const now=m.now;
 switch(m.action){
 case "upload-profile":{const {intent,attachmentId,previousId,checksum}=m,commandId=`upload-${intent.id}`,kind=intent.owner_type==="holding_brand"?"holding":"branch",column=intent.purpose==="inverse"?"inverse_attachment_id":"primary_attachment_id";
await executeCommand(db, actor, commandId, "organization.brand.replace", { ownerType: intent.owner_type, ownerId: intent.owner_id, variant: intent.purpose, checksum }, intent.owner_id, 1, result, [
      assertion(db, commandId, "brand-owner", kind === "holding"
        ? "?=?"
        : `EXISTS(SELECT 1 FROM (${organizationProjectionSources.organization_nodes}) organization_nodes WHERE id=? AND company_id=? AND kind='branch' AND status='active')`,
      kind === "holding" ? [intent.owner_id, actor.holdingId] : [intent.owner_id, actor.companyId]),
      ...prepareBrandMediaUpload(db, actor, m),
      db.prepare(`INSERT INTO organization_brand_profiles(scope_kind,scope_id,company_id,${column},document_mode,updated_by,updated_at) VALUES(?,?,?,?,?,?,?) ON CONFLICT(scope_kind,scope_id) DO UPDATE SET ${column}=excluded.${column},updated_by=excluded.updated_by,updated_at=excluded.updated_at`).bind(kind, intent.owner_id, kind === "branch" ? actor.companyId : null, attachmentId, kind === "holding" ? "holding" : "branch", actor.userId, now),
      ...prepareBrandMediaRemoval(db, actor, {attachmentId:previousId,commandId,now,scope:"workspace"}),
    ], { companyId: actor.companyId, branchId: kind === "branch" ? intent.owner_id : null, departmentId: null });
 return result;}
 case "upload-storefront":{const {intent,attachmentId,previousId,checksum}=m,commandId=`upload-${intent.id}`;
await executeCommand(db, actor, commandId, "organization.storefront-brand.logo", { companyId: intent.owner_id, checksum }, intent.owner_id, 1, result, [
      assertion(db, commandId, "storefront-brand-owner", `EXISTS(SELECT 1 FROM (${organizationProjectionSources.organization_nodes}) organization_nodes WHERE id=? AND kind='company' AND status='active')`, [intent.owner_id]),
      assertion(db, commandId, "storefront-brand-admin", `EXISTS(SELECT 1 FROM (${identity_accessProjectionSources.assignments}) assignments WHERE id=? AND company_id=? AND scope_kind='holding' AND revoked_at IS NULL AND valid_from<=? AND valid_until>? AND iam_assignment_active(id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) AND iam_principal_active(user_id,company_id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) AND iam_role_allows(role,company_id,'organization.manage'))`, [actor.assignmentId, actor.companyId, now, now]),
      ...prepareBrandMediaUpload(db, actor, m),
      db.prepare(`INSERT INTO storefront_app_brands(company_id,short_name,accent_color,logo_attachment_id,updated_by,updated_at)
        SELECT id,left(display_name,40),'#3659c9',?,?,? FROM (${organizationProjectionSources.organization_nodes}) organization_nodes WHERE id=? AND kind='company' AND status='active'
        ON CONFLICT(company_id) DO UPDATE SET logo_attachment_id=excluded.logo_attachment_id,updated_by=excluded.updated_by,updated_at=excluded.updated_at`).bind(attachmentId, actor.userId, now, intent.owner_id),
      ...prepareBrandMediaRemoval(db, actor, {attachmentId:previousId,commandId,now,scope:"storefront"}),
    ], { companyId: actor.companyId, branchId: null, departmentId: null });
 return result;}
 case "storefront":{const {input,current,shortName,previousId,previousPublicationId}=m;
await executeCommand(db, actor, input.commandId, "organization.storefront-brand.update", input, actor.companyId, 1, result, [
    assertion(db, input.commandId, "storefront-brand-owner", `EXISTS(SELECT 1 FROM (${organizationProjectionSources.organization_nodes}) organization_nodes WHERE id=? AND kind='company' AND status='active')`, [actor.companyId]),
    assertion(db, input.commandId, "storefront-brand-admin", `EXISTS(SELECT 1 FROM (${identity_accessProjectionSources.assignments}) assignments WHERE id=? AND company_id=? AND scope_kind='holding' AND revoked_at IS NULL AND valid_from<=? AND valid_until>? AND iam_assignment_active(id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) AND iam_principal_active(user_id,company_id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) AND iam_role_allows(role,company_id,'organization.manage'))`, [actor.assignmentId, actor.companyId, now, now]),
    db.prepare(`INSERT INTO storefront_app_brands(company_id,short_name,accent_color,logo_attachment_id,logo_publication_id,updated_by,updated_at)
      VALUES(?,?,?,?,?,?,?)
      ON CONFLICT(company_id) DO UPDATE SET short_name=COALESCE(?,storefront_app_brands.short_name),accent_color=COALESCE(?,storefront_app_brands.accent_color),logo_attachment_id=${input.removeLogo ? "NULL" : "COALESCE(excluded.logo_attachment_id,storefront_app_brands.logo_attachment_id)"},logo_publication_id=${input.removeLogo?"NULL":"COALESCE(excluded.logo_publication_id,storefront_app_brands.logo_publication_id)"},updated_by=excluded.updated_by,updated_at=excluded.updated_at`).bind(
        actor.companyId,
        shortName ?? current.short_name ?? Array.from(current.display_name.trim()).slice(0, 40).join(""),
        input.accentColor ?? current.accent_color ?? "#3659c9",
        input.removeLogo ? null : current.logo_attachment_id,
        input.removeLogo ? null : current.logo_publication_id,
        actor.userId,
        now,
        shortName ?? null,
        input.accentColor ?? null,
      ),
    ...prepareBrandMediaRemoval(db, actor, {attachmentId:previousId,publicationId:previousPublicationId,commandId:input.commandId,now,scope:"storefront"}),
  ], { companyId: actor.companyId, branchId: null, departmentId: null });
 return result;}
 case "profile":{const {input,previousId,previousPublicationId}=m,removeColumn=input.remove==="primary"?"primary_attachment_id":input.remove==="inverse"?"inverse_attachment_id":null,removePublicationColumn=input.remove==="primary"?"primary_publication_id":input.remove==="inverse"?"inverse_publication_id":null;
await executeCommand(db, actor, input.commandId, "organization.brand.update", input, input.scopeId, 1, result, [
    db.prepare(`INSERT INTO organization_brand_profiles(scope_kind,scope_id,company_id,document_mode,updated_by,updated_at) VALUES(?,?,?,?,?,?) ON CONFLICT(scope_kind,scope_id) DO UPDATE SET document_mode=excluded.document_mode,${removeColumn ? `${removeColumn}=NULL,` : ""}${removePublicationColumn?`${removePublicationColumn}=NULL,`:""}updated_by=excluded.updated_by,updated_at=excluded.updated_at`).bind(input.scopeKind, input.scopeId, input.scopeKind === "branch" ? actor.companyId : null, m.result.documentMode, actor.userId, now),
    ...prepareBrandMediaRemoval(db, actor, {attachmentId:previousId,publicationId:previousPublicationId,commandId:input.commandId,now,scope:"workspace"}),
  ], { companyId: actor.companyId, branchId: input.scopeKind === "branch" ? input.scopeId : null, departmentId: null });
 return result;}
 }
}
async function translated<Result>(action:()=>Promise<Result>):Promise<Result>{try{return await action();}catch(error){if(error instanceof BusinessRuleError)throw new ApiError(error.status,error.code,error.message);throw error;}}
export function resolveWorkspaceBrand(db:SqlDatabase,a:WorkContext){return translated(()=>application(db,a).workspace());}
export function resolveDocumentBrand(db:SqlDatabase,a:WorkContext,scope:{branchId:string|null;requestedMode?:string|null}){return translated(()=>application(db,a).document(scope));}
export function recordBranchId(db:SqlDatabase,a:WorkContext,kind:BrandRecordKind,id:string){return translated(()=>application(db,a).recordBranch(kind,id));}
export function listBrandProfiles(db:SqlDatabase,a:WorkContext){return translated(()=>application(db,a).list());}
export function readStorefrontAppBrand(db:SqlDatabase,a:WorkContext){return translated(()=>application(db,a).storefront());}
export function requireBrandManagement(db:SqlDatabase,a:WorkContext,type:BrandOwnerType,id:string){return translated(()=>application(db,a).manage(type,id));}
export function normalizeBrandImage(buffer:ArrayBuffer){return translated(()=>normalizeBrandMedia(image,buffer));}
export function completeBrandUpload(args:{db:SqlDatabase;files:FileStore;actor:WorkContext;intent:{id:string;owner_type:"holding_brand"|"branch_brand";owner_id:string;purpose:BrandVariant;revision:number;original_name:string};buffer:ArrayBuffer}){return translated(()=>application(args.db,args.actor,args.files).completeUpload(args.intent,args.buffer));}
export function completeStorefrontBrandUpload(args:{db:SqlDatabase;files:FileStore;actor:WorkContext;intent:{id:string;owner_id:string;revision:number;original_name:string};buffer:ArrayBuffer}){return translated(()=>application(args.db,args.actor,args.files).completeUpload({...args.intent,owner_type:"company_storefront_brand",purpose:"logo"},args.buffer));}
export function updateStorefrontAppBrand(db:SqlDatabase,a:WorkContext,input:StorefrontBrandInput){return translated(()=>application(db,a).updateStorefront(input));}
export function updateBrandProfile(db:SqlDatabase,a:WorkContext,input:BrandProfileInput){return translated(()=>application(db,a).updateProfile(input));}
export function brandProfileEvidenceRow(db:SqlDatabase,a:WorkContext,branchId:string){return translated(()=>application(db,a).evidence(branchId));}
