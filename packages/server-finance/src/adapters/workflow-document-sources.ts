import {policyRead} from "@erpx/server-identity_access/public/iam-read-snapshot";
import type {SqlDatabase} from '@erpx/server-runtime/public/runtime/types';
import type {Row} from '@erpx/server-workflow/public/flow-routing-contract';
const tables={'invoice':'finance_invoices','journal':'finance_journals'};
export function workflowDocument(db:SqlDatabase,kind:'invoice' | 'journal',companyId:string,id:string){return db.prepare(`SELECT * FROM ${tables[kind]} WHERE company_id=? AND id=?`).bind(companyId,id).first<Row>();}
export function workflowDocumentPage(db:SqlDatabase,kind:'invoice' | 'journal',companyId:string,after:string){return db.prepare(`SELECT * FROM ${tables[kind]} WHERE company_id=? AND id>? ORDER BY id LIMIT 100`).bind(companyId,after).all<Row>();}
export function workflowReferenceRows(db:SqlDatabase,kind:'invoice' | 'journal',companyId:string){const updated=kind==='journal'?'created_at':'updated_at';return policyRead(db,JSON.stringify(["workflow-document-rows",companyId,kind]),()=>db.prepare(`SELECT * FROM ${tables[kind]} WHERE company_id=? ORDER BY ${updated} DESC`).bind(companyId).all<Row>());}
