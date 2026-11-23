import 'server-only';
import type {SqlDatabase} from '@erpx/server-runtime/public/runtime/types';
import type {Row} from '@erpx/server-workflow/public/flow-routing-contract';
import {
  nativeInventoryReadFirst,
  nativeInventoryReadAll,
  INVENTORY_RD_WORKFLOW_DOCUMENT_STOCK_COUNT,
  INVENTORY_RD_WORKFLOW_DOCUMENT_TRANSFER,
  INVENTORY_RD_WORKFLOW_DOCUMENT_PAGE_STOCK_COUNT,
  INVENTORY_RD_WORKFLOW_DOCUMENT_PAGE_TRANSFER,
} from './native-inventory-read';

const kinds={
  stock_count:{row:INVENTORY_RD_WORKFLOW_DOCUMENT_STOCK_COUNT,page:INVENTORY_RD_WORKFLOW_DOCUMENT_PAGE_STOCK_COUNT},
  transfer:{row:INVENTORY_RD_WORKFLOW_DOCUMENT_TRANSFER,page:INVENTORY_RD_WORKFLOW_DOCUMENT_PAGE_TRANSFER},
} as const;

export function workflowDocument(db:SqlDatabase,kind:'stock_count' | 'transfer',companyId:string,id:string){return nativeInventoryReadFirst<Row>(kinds[kind].row,[companyId,id],db);}
export function workflowDocumentPage(db:SqlDatabase,kind:'stock_count' | 'transfer',companyId:string,after:string){return nativeInventoryReadAll<Row>(kinds[kind].page,[companyId,after],db);}
