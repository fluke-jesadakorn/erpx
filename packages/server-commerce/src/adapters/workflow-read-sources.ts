import type {SqlDatabase} from '@erpx/server-runtime/public/runtime/types';
import type {Row} from '@erpx/server-workflow/public/flow-routing-contract';
const tables={'order':'commerce_orders'};
export function workflowDocument(db:SqlDatabase,kind:'order',companyId:string,id:string){return db.prepare(`SELECT * FROM ${tables[kind]} WHERE company_id=? AND id=?`).bind(companyId,id).first<Row>();}
export function workflowDocumentPage(db:SqlDatabase,kind:'order',companyId:string,after:string){return db.prepare(`SELECT * FROM ${tables[kind]} WHERE company_id=? AND id>? ORDER BY id LIMIT 100`).bind(companyId,after).all<Row>();}
export function workflowReferenceRows(db:SqlDatabase,kind:'order',companyId:string){const updated='updated_at';return db.prepare(`SELECT * FROM ${tables[kind]} WHERE company_id=? ORDER BY ${updated} DESC`).bind(companyId).all<Row>();}
