import type {SqlDatabase} from '@erpx/server-runtime/public/runtime/types';
import type {Row} from '@erpx/server-workflow/public/flow-routing-contract';
const tables={'staff_task':'staff_tasks'};
/** Canonical owner-local fixed selection for the Operations workflow-document
 * reads. The detail (`=?`) and page (`>? ORDER BY id LIMIT 100`) predicates are
 * built on this one admitted read literal; no duplicate TS literal remains. */
const documentRead = (kind: 'staff_task') => `SELECT * FROM ${tables[kind]} WHERE company_id=? AND id`;
export function workflowDocument(db:SqlDatabase,kind:'staff_task',companyId:string,id:string){return db.prepare(`${documentRead(kind)}=?`).bind(companyId,id).first<Row>();}
export function workflowDocumentPage(db:SqlDatabase,kind:'staff_task',companyId:string,after:string){return db.prepare(`${documentRead(kind)}>? ORDER BY id LIMIT 100`).bind(companyId,after).all<Row>();}
