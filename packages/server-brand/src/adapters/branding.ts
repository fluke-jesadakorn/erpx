import { commercialOwnerProjectionSources as commerceProjectionSources } from "@erpx/server-commerce/public/commercial-owner-projection-sources";
import { commercialOwnerProjectionSources as financeProjectionSources } from "@erpx/server-finance/public/commercial-owner-projection-sources";
import { commercialOwnerProjectionSources as inventoryProjectionSources } from "@erpx/server-inventory/public/commercial-owner-projection-sources";
import { commercialOwnerProjectionSources as organizationProjectionSources } from "@erpx/server-organization/public/commercial-owner-projection-sources";
import { commercialOwnerProjectionSources as people_payrollProjectionSources } from "@erpx/server-people_payroll/public/commercial-owner-projection-sources";
import "server-only";
import sharp from "sharp";
import type { WorkContext } from "@erpx/server-identity_access/public/authorization";
import { companyAuthority } from "@erpx/server-operations/public/staff-policy";
import type { SqlDatabase } from "@erpx/server-runtime/public/runtime/types";
import { BusinessRuleError } from "@erpx/server-shared/index";
import { normalizeBrandMedia } from "@erpx/server-brand/application/branding";
import type { BrandOwnerType, BrandRecordKind } from "@erpx/server-brand/domain/branding";
import {
  nativeBrandProfileEvidence,
  nativeResolveDocumentBrand,
  nativeResolveWorkspaceBrand,
} from "./native-brand-presentation";
export { documentBrandModes } from "@erpx/server-brand/domain/branding";
export type { DocumentBrandMode,BrandVariant,WorkspaceBrand,StorefrontAppBrand,DocumentBrand } from "@erpx/server-brand/domain/branding";
const image={async metadata(buffer:ArrayBuffer){return sharp(Buffer.from(buffer),{failOn:"warning"}).metadata();},async encode(buffer:ArrayBuffer){return sharp(Buffer.from(buffer),{failOn:"warning"}).rotate().resize({width:4096,height:4096,fit:"inside",withoutEnlargement:true}).webp({quality:92,alphaQuality:100}).toBuffer();}};
const branch=(db:SqlDatabase,actor:WorkContext,id:string)=>db.prepare(`SELECT id,display_name FROM (${organizationProjectionSources.organization_nodes}) organization_nodes WHERE id=? AND company_id=? AND kind='branch' AND status='active'`).bind(id,actor.companyId).first<{id:string;display_name:string}>();
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
async function companyExists(db:SqlDatabase,actor:WorkContext,id:string){return !!await db.prepare(`SELECT 1 FROM (${organizationProjectionSources.organization_nodes}) organization_nodes WHERE id=? AND kind='company' AND status='active'`).bind(id).first();}
export async function requireBrandManagement(db:SqlDatabase,a:WorkContext,type:BrandOwnerType,id:string){
 const allowed=type==="holding_brand"?id===a.holdingId&&await companyAuthority(db,a,["holding_owner"])
  :type==="branch_brand"?await companyAuthority(db,a,"organization.manage")&&!!await branch(db,a,id)
  :id===a.companyId&&await companyAuthority(db,a,"organization.manage")&&await companyExists(db,a,id);
 if(!allowed)throw new BusinessRuleError(403,"BRAND_MANAGEMENT_FORBIDDEN","A Holding owner or authorized organization administrator is required.");
}
export async function recordBranchId(db:SqlDatabase,a:WorkContext,kind:BrandRecordKind,id:string){
 if(kind==="order"||kind==="expense"){const table=kind==="order"?"commerce_orders":"finance_expenses";const found=await db.prepare(`SELECT branch_id FROM ${table} WHERE id=? AND company_id=?`).bind(id,a.companyId).first<{branch_id:string|null}>();return found?.branch_id??null;}
 if(kind==="invoice"){const found=await db.prepare(`SELECT CASE
  WHEN i.source_type='commerce_fulfillment' THEN (SELECT o.branch_id FROM (${commerceProjectionSources.commerce_fulfillments}) f JOIN (${commerceProjectionSources.commerce_orders}) o ON o.id=f.order_id WHERE f.id=i.source_id)
  WHEN i.source_type='expense_claim' THEN (SELECT e.branch_id FROM (${people_payrollProjectionSources.finance_expenses}) e WHERE e.id=i.source_id)
  ELSE NULL END branch_id FROM (${financeProjectionSources.finance_invoices}) i WHERE i.id=? AND i.company_id=?`).bind(id,a.companyId).first<{branch_id:string|null}>();return found?.branch_id??null;}
 if(kind==="payment"){const found=await db.prepare(`SELECT CASE WHEN COUNT(DISTINCT branch_id)=1 THEN MIN(branch_id) ELSE NULL END branch_id FROM (
  SELECT CASE WHEN i.source_type='commerce_fulfillment' THEN (SELECT o.branch_id FROM (${commerceProjectionSources.commerce_fulfillments}) f JOIN (${commerceProjectionSources.commerce_orders}) o ON o.id=f.order_id WHERE f.id=i.source_id)
  WHEN i.source_type='expense_claim' THEN (SELECT e.branch_id FROM (${people_payrollProjectionSources.finance_expenses}) e WHERE e.id=i.source_id) ELSE NULL END branch_id
  FROM (${financeProjectionSources.finance_allocations}) a JOIN (${financeProjectionSources.finance_invoices}) i ON i.id=a.invoice_id WHERE a.payment_id=? AND a.company_id=?) branded WHERE branch_id IS NOT NULL`).bind(id,a.companyId).first<{branch_id:string|null}>();return found?.branch_id??null;}
 const found=await db.prepare(`SELECT CASE
  WHEN j.source_type='expense_claim' THEN (SELECT e.branch_id FROM (${people_payrollProjectionSources.finance_expenses}) e WHERE e.id=j.source_id)
  WHEN j.source_type='stock_count' THEN (SELECT c.branch_id FROM (${inventoryProjectionSources.inventory_counts}) c WHERE c.id=j.source_id)
  ELSE NULL END branch_id FROM (${financeProjectionSources.finance_journals}) j WHERE j.id=? AND j.company_id=?`).bind(id,a.companyId).first<{branch_id:string|null}>();return found?.branch_id??null;
}
export function normalizeBrandImage(buffer:ArrayBuffer){return normalizeBrandMedia(image,buffer);}
/** Native executive Brand profile evidence row for one active Branch. */
export async function readBrandProfileEvidence(a:WorkContext,branchId:string){
 return nativeBrandProfileEvidence(a,branchId);
}
