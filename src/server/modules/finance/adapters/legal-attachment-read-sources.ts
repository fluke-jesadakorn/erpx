import 'server-only';
import type {SqlDatabase} from '../../runtime/public/runtime/types';
import {financeSourceDocumentReadLeases} from './document-purchases';
/** Retain the actual payable and its original source joins under Legal's
 * complete parent. No generic Finance or included-source mutation is granted. */
export function legalInvoiceAttachmentSourceLocks(database:SqlDatabase,companyId:string){return [...financeSourceDocumentReadLeases(database,companyId),database.prepare('LOCK TABLE finance_invoices,finance_invoice_line_versions,finance_journals,finance_journal_lines,finance_allocations,finance_payments IN SHARE ROW EXCLUSIVE MODE')];}
export function legalInvoiceAttachmentSourceImage(database:SqlDatabase,companyId:string,id:string){return database.prepare(`SELECT jsonb_build_object('invoice',to_jsonb(i),'journal',to_jsonb(j),'lines',(SELECT jsonb_agg(to_jsonb(l) ORDER BY to_jsonb(l)::text) FROM finance_invoice_line_versions l WHERE l.invoice_id=i.id),'allocations',(SELECT jsonb_agg(to_jsonb(a) ORDER BY to_jsonb(a)::text) FROM finance_allocations a WHERE a.invoice_id=i.id AND a.company_id=i.company_id))::text image FROM finance_invoices i LEFT JOIN finance_journals j ON j.id=i.journal_id AND j.company_id=i.company_id WHERE i.id=? AND i.company_id=? AND i.kind='payable'`).bind(id,companyId).first<string>('image');}
