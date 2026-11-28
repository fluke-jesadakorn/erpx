import type {SqlDatabase} from '@erpx/platform-runtime-server/public/runtime/types';
import type {Row} from '@erpx/workflow-server/public/flow-routing-contract';
import {
  nativeOperationsReadFirst,
  nativeOperationsReadAll,
  OPERATIONS_RD_WORKFLOW_DOCUMENT_DETAIL,
  OPERATIONS_RD_WORKFLOW_DOCUMENT_PAGE,
} from './native-operations-read';

const kinds={
  staff_task:{row:OPERATIONS_RD_WORKFLOW_DOCUMENT_DETAIL,page:OPERATIONS_RD_WORKFLOW_DOCUMENT_PAGE},
} as const;

export function workflowDocument(db:SqlDatabase,kind:'staff_task',companyId:string,id:string){return nativeOperationsReadFirst<Row>(kinds[kind].row,[companyId,id],db);}
export function workflowDocumentPage(db:SqlDatabase,kind:'staff_task',companyId:string,after:string){return nativeOperationsReadAll<Row>(kinds[kind].page,[companyId,after],db);}
