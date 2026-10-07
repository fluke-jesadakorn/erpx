import type {SqlDatabase} from '../../runtime/public/runtime/types';
import type {Row} from '../../workflow/public/flow-routing-contract';
const tables={'stock_count':'inventory_counts','transfer':'inventory_transfers'};
export function workflowDocument(db:SqlDatabase,kind:'stock_count' | 'transfer',companyId:string,id:string){return db.prepare(`SELECT * FROM ${tables[kind]} WHERE company_id=? AND id=?`).bind(companyId,id).first<Row>();}
export function workflowDocumentPage(db:SqlDatabase,kind:'stock_count' | 'transfer',companyId:string,after:string){return db.prepare(`SELECT * FROM ${tables[kind]} WHERE company_id=? AND id>? ORDER BY id LIMIT 100`).bind(companyId,after).all<Row>();}
