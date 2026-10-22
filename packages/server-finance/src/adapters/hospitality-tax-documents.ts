import {financeOwnedSnapshotSources as pmsSnapshotSources} from "@erpx/server-pms/public/finance-owned-snapshot-sources";
import {financeOwnedSnapshotSources as posSnapshotSources} from "@erpx/server-pos/public/finance-owned-snapshot-sources";
import 'server-only';
import { randomUUID } from 'node:crypto';
import { hasPermission } from "@erpx/feature-identity_access/lib/permissions";
import type { WorkContext } from "@erpx/server-identity_access/public/authorization";
import {assertion} from "@erpx/server-workflow/public/command-assertions";
import {executeCommand,replayCommand} from "@erpx/server-workflow/public/commands";
import { assertProductionModuleEnabled } from "@erpx/server-pms/public/hospitality-rollout";
import { ApiError } from "@erpx/server-runtime/public/http";
import { holdingResources } from "../../../../packages/server-runtime/src/public/runtime";
import type { SqlDatabase } from "@erpx/server-runtime/public/runtime/types";

import {createHospitalityTaxDocuments,type TaxDocumentRepository} from "@erpx/server-finance/application/hospitality-tax-documents";
import type {Invoice,Document,TaxDocumentCommand} from "@erpx/server-finance/domain/hospitality-tax-documents";
export {taxDocumentCommandSchema,type TaxDocumentCommand} from "@erpx/server-finance/domain/hospitality-tax-documents";
function nativeTaxDocuments(){let pending:Promise<SqlDatabase>|undefined;const ready=(actor:WorkContext)=>pending??=holdingResources(actor.holdingId).then(r=>r.write);
 const repository:TaxDocumentRepository={
 invoice:async(actor,invoiceId)=>{const db=await ready(actor);  return db.prepare(`SELECT i.id,i.company_id,i.source_type,i.source_id,i.currency,i.total_minor,i.tax_minor,i.status,
    CASE WHEN i.source_type='pos' THEN s.branch_id WHEN i.source_type='pms' THEN p.branch_id ELSE NULL END branch_id
    FROM finance_invoices i
    LEFT ${posSnapshotSources().hospitality_tax_documents_pos_sales_s_join} ON i.source_type='pos' AND s.id=i.source_id AND s.invoice_id=i.id AND s.company_id=i.company_id AND s.state='completed'
    LEFT ${pmsSnapshotSources().hospitality_tax_documents_pms_documents_d_join} ON i.source_type='pms' AND d.finance_id=i.id AND d.kind='invoice' AND d.guest_account_id=i.source_id
    LEFT ${pmsSnapshotSources().hospitality_tax_documents_pms_properties_p_join} ON p.id=d.property_id AND p.company_id=i.company_id
    WHERE i.id=? AND i.company_id=?`).bind(invoiceId,actor.companyId).first<Invoice>();
},
 document:async(invoiceId,documentId)=>(await pending!).prepare('SELECT * FROM hospitality_tax_documents WHERE id=? AND invoice_id=?').bind(documentId,invoiceId).first<Document>(),
 documents:async(companyId,invoiceId)=>(await (await pending!).prepare('SELECT * FROM hospitality_tax_documents WHERE company_id=? AND invoice_id=? ORDER BY created_at DESC,id DESC').bind(companyId,invoiceId).all<Document>()).results,
 replay:async(actor,key,operation,recordId,command)=>replayCommand(await ready(actor),actor,key,operation,recordId,command),
 commitRecord:async(facts)=>{const {actor,key,operation,command,recordId,invoice,documentId,now,result}=facts;const db=await ready(actor);    return executeCommand(db,actor,key,operation,command,recordId,1,result,[
      assertion(db,key,'invoice-current',`EXISTS(SELECT 1 FROM finance_invoices WHERE id=? AND company_id=? AND source_type=? AND currency=? AND total_minor=? AND tax_minor=? AND status IN ('posted','partially_paid','paid'))`,[invoice.id,actor.companyId,invoice.source_type,invoice.currency,invoice.total_minor,invoice.tax_minor]),
      db.prepare(`INSERT INTO hospitality_tax_documents(id,company_id,invoice_id,source_type,issuer,external_reference,issued_on,currency,total_minor,tax_minor,document_total_minor,document_tax_minor,state,version,recorded_by,reason,created_at,updated_at)
        VALUES(?,?,?,?,?,?,?,?,?,?,?,?,'recorded',1,?,?,?,?)`).bind(documentId,actor.companyId,invoice.id,invoice.source_type,command.issuer,command.externalReference,command.issuedOn,invoice.currency,invoice.total_minor,invoice.tax_minor,command.documentTotalMinor,command.documentTaxMinor,actor.userId,command.reason,now,now),
    ]);},
 commitDecision:async(facts)=>{const {actor,key,operation,command,recordId,invoice,doc,next,now,state,result}=facts;const db=await ready(actor);  return executeCommand(db,actor,key,operation,command,recordId,next,result,[
    assertion(db,key,'invoice-current',`EXISTS(SELECT 1 FROM finance_invoices WHERE id=? AND company_id=? AND source_type=? AND currency=? AND total_minor=? AND tax_minor=? AND status IN ('posted','partially_paid','paid'))`,[invoice.id,actor.companyId,invoice.source_type,invoice.currency,invoice.total_minor,invoice.tax_minor]),
    assertion(db,key,'document-current',`EXISTS(SELECT 1 FROM hospitality_tax_documents WHERE id=? AND company_id=? AND invoice_id=? AND version=? AND state=? AND currency=? AND total_minor=? AND tax_minor=?)`,[doc.id,actor.companyId,invoice.id,doc.version,doc.state,invoice.currency,invoice.total_minor,invoice.tax_minor]),
    db.prepare(`UPDATE hospitality_tax_documents SET state=?,version=?,${command.action==='verify'?'verified_by':'voided_by'}=?,reason=?,updated_at=? WHERE id=? AND company_id=? AND version=?`).bind(state,next,actor.userId,command.action==='void'?command.reason:doc.reason,now,doc.id,actor.companyId,doc.version),
  ]);},
 };return createHospitalityTaxDocuments({repository,ready:async actor=>{await ready(actor);},assertEnabled:async(actor,source)=>assertProductionModuleEnabled(await ready(actor),actor.companyId,source),hasPermission,now:Date.now,id:randomUUID,error:(...args)=>new ApiError(...args)});}
export async function taxDocumentView(actor:WorkContext,invoiceId:string){return nativeTaxDocuments().taxDocumentView(actor,invoiceId);}
export async function frontTaxDocumentView(actor:WorkContext,invoiceId:string,app:'pos'|'pms'){return nativeTaxDocuments().frontTaxDocumentView(actor,invoiceId,app);}
export async function frontTaxDocumentCommand(actor:WorkContext,command:TaxDocumentCommand,key:string,app:'pos'|'pms'){return nativeTaxDocuments().frontTaxDocumentCommand(actor,command,key,app);}
export async function taxDocumentCommand(actor:WorkContext,command:TaxDocumentCommand,key:string){return nativeTaxDocuments().taxDocumentCommand(actor,command,key);}
