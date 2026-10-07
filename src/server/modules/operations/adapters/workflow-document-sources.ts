import type {SqlDatabase} from '../../runtime/public/runtime/types';
import type {Row} from '../../workflow/public/flow-routing-contract';
const tables={'staff_task':'staff_tasks'};
export function workflowDocument(db:SqlDatabase,kind:'staff_task',companyId:string,id:string){return db.prepare(`SELECT * FROM ${tables[kind]} WHERE company_id=? AND id=?`).bind(companyId,id).first<Row>();}
export function workflowDocumentPage(db:SqlDatabase,kind:'staff_task',companyId:string,after:string){return db.prepare(`SELECT * FROM ${tables[kind]} WHERE company_id=? AND id>? ORDER BY id LIMIT 100`).bind(companyId,after).all<Row>();}
