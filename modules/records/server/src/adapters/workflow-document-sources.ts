import type {SqlDatabase} from '@erpx/platform-runtime-server/public/runtime/types';
import type {Row} from '@erpx/workflow-server/public/flow-routing-contract';
import { selectAllWhereSql } from './read-tail-select-all';
const tables={'request':'employee_requests','record':'workspace_records'};
/** Canonical owner-local fixed selection for the Records workflow-document
 * reads. The detail (`=?`) and page (`>? ORDER BY id LIMIT 100`) predicates
 * are built on this one admitted SELECT; no duplicate TS literal remains. */
const documentRead = (kind: 'request' | 'record') => selectAllWhereSql(tables[kind], 'company_id=? AND id');
export function workflowDocument(db:SqlDatabase,kind:'request' | 'record',companyId:string,id:string){return db.prepare(`${documentRead(kind)}=?`).bind(companyId,id).first<Row>();}
export function workflowDocumentPage(db:SqlDatabase,kind:'request' | 'record',companyId:string,after:string){return db.prepare(`${documentRead(kind)}>? ORDER BY id LIMIT 100`).bind(companyId,after).all<Row>();}
