import "server-only";
import type { WorkContext } from "../authorization";
import { holdingResources } from "../runtime";
import { requirePermission } from "../iam-permissions";
import { ApiError } from "../http";
import { pmsHotelOverview } from "./home";

/** Posted GL, subledger and original evidence remain separate, auditable figures. */
export async function pmsFinanceOverview(actor:WorkContext,propertyId?:string) {
  requirePermission(actor,"finance.read");requirePermission(actor,"pms.reports");
  if(actor.departmentId||actor.beta)throw new ApiError(403,"PMS_BRANCH_REQUIRED","Choose a company or branch.");
  const {write}=await holdingResources(actor.holdingId);
  const hotels=await pmsHotelOverview(write,actor,propertyId);
  if(propertyId&&!hotels.length)throw new ApiError(404,"PMS_NOT_FOUND","Hotel not found.");
  const gl=(await write.prepare(`SELECT p.id,
      COALESCE(sum(l.debit_minor-l.credit_minor) FILTER (WHERE l.account_id=p.config_json::jsonb->'accounts'->>'receivable'),0) receivable,
      COALESCE(sum(l.credit_minor-l.debit_minor) FILTER (WHERE l.account_id=p.config_json::jsonb->'accounts'->>'advance'),0) advance,
      COALESCE(sum(l.credit_minor-l.debit_minor) FILTER (WHERE l.account_id=p.config_json::jsonb->'accounts'->>'deposit'),0) deposit,
      COALESCE(sum(l.debit_minor-l.credit_minor) FILTER (WHERE l.account_id IN (p.config_json::jsonb->'accounts'->>'bank',p.config_json::jsonb->'accounts'->>'cash')),0) received,
      COALESCE(sum(l.debit_minor),0) debit,COALESCE(sum(l.credit_minor),0) credit
    FROM pms_properties p LEFT JOIN (SELECT property_id,journal_id FROM pms_postings
      UNION SELECT property_id,opening_journal_id FROM pms_import_batches WHERE state='applied' AND opening_journal_id IS NOT NULL) pp ON pp.property_id=p.id
    LEFT JOIN finance_journals j ON j.id=pp.journal_id AND j.company_id=p.company_id AND j.status='posted'
    LEFT JOIN finance_journal_lines l ON l.journal_id=j.id
    WHERE p.company_id=? AND (?::text IS NULL OR p.branch_id=?) AND (?::text IS NULL OR p.id=?) GROUP BY p.id`)
    .bind(actor.companyId,actor.branchId,actor.branchId,propertyId??null,propertyId??null)
    .all<{id:string;receivable:number;advance:number;deposit:number;received:number;debit:number;credit:number}>()).results;
  const reviews=(await write.prepare(`SELECT t.id,t.property_id,t.folio_id,t.state,t.amount_minor,t.currency,t.version,t.reference,t.recorded_by,
      t.bank_transaction_id,t.entry_id,t.finance_payment_id,t.updated_at
    FROM pms_transfer_requests t JOIN pms_properties p ON p.id=t.property_id AND p.company_id=t.company_id
    WHERE t.company_id=? AND (?::text IS NULL OR p.branch_id=?) AND (?::text IS NULL OR p.id=?)
    ORDER BY CASE WHEN t.state='pending' THEN 0 ELSE 1 END,t.updated_at DESC LIMIT 100`)
    .bind(actor.companyId,actor.branchId,actor.branchId,propertyId??null,propertyId??null).all<{
      id:string;property_id:string;folio_id:string;state:string;amount_minor:number;currency:string;version:number;reference:string;recorded_by:string;
      bank_transaction_id:string|null;entry_id:string|null;finance_payment_id:string|null;updated_at:number}>()).results;
  const exceptions=(await write.prepare(`SELECT r.id,r.property_id,r.data_json::jsonb->>'folioId' folio_id,
      r.data_json::jsonb->>'description' description,r.data_json::jsonb->>'kind' kind,
      (r.data_json::jsonb->>'total')::bigint total
    FROM pms_records r JOIN pms_properties p ON p.id=r.property_id WHERE p.company_id=?
    AND (?::text IS NULL OR p.branch_id=?) AND (?::text IS NULL OR p.id=?) AND r.kind='entry'
    AND (r.data_json::jsonb->>'total')::bigint<>0
    AND NOT EXISTS(SELECT 1 FROM pms_import_balances ib JOIN pms_import_batches b ON b.id=ib.batch_id
      JOIN finance_journals j ON j.id=b.opening_journal_id AND j.status='posted'
      WHERE ib.entry_id=r.id AND ib.property_id=r.property_id AND b.state='applied')
    AND NOT EXISTS(SELECT 1 FROM pms_postings pp JOIN finance_journals j ON j.id=pp.journal_id AND j.status='posted'
      WHERE pp.property_id=r.property_id AND pp.record_id=r.id)
    ORDER BY r.updated_at DESC,r.id LIMIT 100`).bind(actor.companyId,actor.branchId,actor.branchId,propertyId??null,propertyId??null)
    .all<{id:string;property_id:string;folio_id:string;description:string;kind:string;total:number}>()).results;
  const documents=(await write.prepare(`SELECT d.id,d.property_id,d.folio_id,d.kind,d.finance_id,d.created_at,
      COALESCE(i.journal_id,pay.journal_id,d.snapshot_json::jsonb->>'openingJournalId') journal_id,
      CASE WHEN pay.id IS NOT NULL THEN 'payments' ELSE 'invoices' END finance_kind
    FROM pms_documents d JOIN pms_properties p ON p.id=d.property_id
    LEFT JOIN finance_invoices i ON i.id=d.finance_id AND i.company_id=p.company_id
    LEFT JOIN finance_payments pay ON pay.id=d.finance_id AND pay.company_id=p.company_id
    WHERE p.company_id=? AND (?::text IS NULL OR p.branch_id=?) AND (?::text IS NULL OR p.id=?)
    ORDER BY d.created_at DESC,d.id LIMIT 100`).bind(actor.companyId,actor.branchId,actor.branchId,propertyId??null,propertyId??null)
    .all<{id:string;property_id:string;folio_id:string;kind:string;finance_id:string|null;journal_id:string|null;finance_kind:string;created_at:number}>()).results;
  return {hotels:hotels.map(hotel=>{
    const line=gl.find(row=>row.id===hotel.id);
    const ledger={receivable:Number(line?.receivable??0),advance:Number(line?.advance??0),deposit:Number(line?.deposit??0),received:Number(line?.received??0)};
    return {...hotel,ledger,differences:{receivable:(hotel.receivable??0)-ledger.receivable,advance:(hotel.advance??0)-ledger.advance,deposit:(hotel.deposit??0)-ledger.deposit},
      unbalanced:Number(line?.debit??0)-Number(line?.credit??0),pendingTransfers:reviews.filter(row=>row.property_id===hotel.id&&row.state==="pending").length};
  }),reviews,exceptions,documents,updatedAt:Date.now()};
}
