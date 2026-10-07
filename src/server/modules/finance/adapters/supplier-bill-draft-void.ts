import {financeOwnedSnapshotSources as commerceSnapshotSources} from "../../commerce/public/finance-owned-snapshot-sources";
import {financeOwnedSnapshotSources as procurementSnapshotSources} from "../../procurement/public/finance-owned-snapshot-sources";
import "server-only";
import type {SqlDatabase} from "../../runtime/public/runtime/types";
import {assertion} from "../../workflow/public/commands";
export function stageSupplierBillDraftVoid(db:SqlDatabase,companyId:string,input:{invoiceId:string;invoiceVersion:number;matchId:string},key:string,now:number){return [
 assertion(db,key,'unposted-ap',`EXISTS(SELECT 1 FROM finance_invoices i WHERE i.id=? AND i.company_id=? AND i.status='draft' AND i.version=? AND i.journal_id IS NULL AND i.source_type='commerce_fulfillment' AND NOT EXISTS(SELECT 1 FROM finance_allocations WHERE invoice_id=i.id) AND NOT EXISTS(SELECT 1 ${procurementSnapshotSources().supplier_bill_draft_void_procurement_bill_match_receipts_br_from} ${commerceSnapshotSources().supplier_bill_draft_void_commerce_fulfillments_f_join} ON f.id=br.fulfillment_id WHERE br.match_id=? AND f.invoice_id IS DISTINCT FROM i.id))`,[input.invoiceId,companyId,input.invoiceVersion,input.matchId]),
 db.prepare("UPDATE finance_invoices SET status='voided',outstanding_minor=0,version=version+1,updated_at=? WHERE id=?").bind(now,input.invoiceId),
 ];}
