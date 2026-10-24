import { platformOwnerProjectionSources as commercePlatformSources } from "@erpx/server-commerce/public/platform-owner-projection-sources";
import { platformOwnerProjectionSources as financePlatformSources } from "@erpx/server-finance/public/platform-owner-projection-sources";
import "server-only";
import type {WorkContext} from "@erpx/server-identity_access/public/authorization";
import type {SqlDatabase} from "@erpx/server-runtime/public/runtime/types";
import {hasPermission} from "@erpx/feature-identity_access/lib/permissions";
import {getEmployeeRequest} from '@erpx/server-records/public/employee-requests-read';
import {ApiError} from "@erpx/server-runtime/public/http";
import {BusinessRuleError} from "@erpx/server-shared/index";
import {createPurchaseProgressApplication} from "@erpx/server-procurement/application/request-progress";
import type {ProgressRow,PurchaseProgressRequest} from "@erpx/server-procurement/domain/request-progress";
export type {PurchaseProgressState,PurchaseProgressStep,PurchaseRequestProgress} from "@erpx/server-procurement/domain/request-progress";
async function facts(db:SqlDatabase,actor:WorkContext,request:PurchaseProgressRequest){
  const rfqs = (await db.prepare("SELECT id,status,order_id FROM procurement_rfqs WHERE request_id=? AND company_id=? AND status<>'cancelled' ORDER BY updated_at DESC,id DESC").bind(request.id,actor.companyId).all<ProgressRow>()).results;
  const orders=(await db.prepare(`SELECT DISTINCT o.id,o.number,o.state,o.created_at FROM (${commercePlatformSources.commerce_orders}) o WHERE o.company_id=? AND o.kind='purchase' AND (o.id=? OR EXISTS(SELECT 1 FROM procurement_request_orders l WHERE l.request_id=? AND l.order_id=o.id)) ORDER BY o.created_at DESC,o.id DESC`).bind(actor.companyId,request.purchase_order_id,request.id).all<ProgressRow>()).results;
  const quantities=await db.prepare(`SELECT COALESCE(SUM(l.quantity_milli),0) ordered_milli,COALESCE(SUM(l.fulfilled_milli),0) received_milli FROM (${commercePlatformSources.commerce_order_lines}) l JOIN (${commercePlatformSources.commerce_orders}) o ON o.id=l.order_id WHERE o.company_id=? AND (o.id=? OR EXISTS(SELECT 1 FROM procurement_request_orders link WHERE link.request_id=? AND link.order_id=o.id))`).bind(actor.companyId,request.purchase_order_id,request.id).first<ProgressRow>();
  const receipts=await db.prepare(`SELECT COUNT(*) receipt_count FROM (${commercePlatformSources.commerce_fulfillments}) f JOIN (${commercePlatformSources.commerce_orders}) o ON o.id=f.order_id WHERE o.company_id=? AND (o.id=? OR EXISTS(SELECT 1 FROM procurement_request_orders link WHERE link.request_id=? AND link.order_id=o.id))`).bind(actor.companyId,request.purchase_order_id,request.id).first<ProgressRow>();
  const bills=await db
        .prepare(
          `SELECT COUNT(*) bill_count,
             COUNT(*) FILTER (WHERE m.status='posted') posted_count,
             COUNT(m.invoice_id) invoice_count,
             COUNT(*) FILTER (WHERE i.status='paid' OR COALESCE(i.outstanding_minor,0)=0) paid_count,
             (SELECT COUNT(DISTINCT br.fulfillment_id)
                FROM procurement_bill_match_receipts br
                JOIN procurement_bill_matches covered ON covered.id=br.match_id
               JOIN (${commercePlatformSources.commerce_orders}) covered_order ON covered_order.id=covered.order_id
               WHERE covered.company_id=? AND (covered_order.id=? OR EXISTS(SELECT 1 FROM procurement_request_orders link WHERE link.request_id=? AND link.order_id=covered_order.id)) AND covered.status<>'returned' AND br.active=TRUE) matched_receipt_count
           FROM procurement_bill_matches m
           LEFT JOIN (${financePlatformSources.finance_invoices}) i ON i.id=m.invoice_id
           JOIN (${commercePlatformSources.commerce_orders}) matched_order ON matched_order.id=m.order_id
           WHERE m.company_id=? AND (matched_order.id=? OR EXISTS(SELECT 1 FROM procurement_request_orders link WHERE link.request_id=? AND link.order_id=matched_order.id)) AND m.status<>'returned'`,
        )
        .bind(actor.companyId,request.purchase_order_id,request.id,actor.companyId,request.purchase_order_id,request.id)
        .first<ProgressRow>();
  const payments = await db
        .prepare(
          `SELECT COUNT(*) instruction_count,
             COUNT(*) FILTER (WHERE s.status='confirmed') confirmed_count,
             MIN(s.batch_id) batch_id
           FROM (${financePlatformSources.finance_supplier_instructions}) s
           JOIN (${financePlatformSources.finance_invoices}) i ON i.id=s.invoice_id
           JOIN procurement_bill_matches m ON m.invoice_id=i.id
           JOIN (${commercePlatformSources.commerce_orders}) paid_order ON paid_order.id=m.order_id
           WHERE m.company_id=? AND (paid_order.id=? OR EXISTS(SELECT 1 FROM procurement_request_orders link WHERE link.request_id=? AND link.order_id=paid_order.id)) AND s.status<>'cancelled'`,
        )
        .bind(actor.companyId,request.purchase_order_id,request.id)
        .first<ProgressRow>();
  const allocations=(await db.prepare("SELECT l.request_item_ordinal ordinal,COALESCE(sum(l.request_quantity_milli),0) allocated_milli FROM procurement_rfq_lines l JOIN procurement_rfqs r ON r.id=l.rfq_id WHERE r.request_id=? AND r.status<>'cancelled' GROUP BY l.request_item_ordinal").bind(request.id).all<ProgressRow>()).results;
 return {rfqs,orders,quantities,receipts,bills,payments,allocations};
}
export async function purchaseRequestProgress(db:SqlDatabase,actor:WorkContext,requestId:string){
 const application=createPurchaseProgressApplication({canPurchase:hasPermission(actor,"commerce.read"),canFinance:hasPermission(actor,"finance.read"),request:(id)=>getEmployeeRequest(db,actor,id),facts:(request)=>facts(db,actor,request)});
 try{return await application(requestId);}catch(error){if(error instanceof BusinessRuleError)throw new ApiError(error.status,error.code,error.message);throw error;}
}
