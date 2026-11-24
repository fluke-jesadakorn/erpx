import {inventoryAuditHistorySql} from './inventory-audit-history-source';
import {nativeWorkflowWrite} from './native-workflow-write';
import type {SqlDatabase} from '@erpx/server-runtime/public/runtime/types';import type {TransferRow} from '@erpx/server-inventory/public/transfer-contract';
export function inventoryTransferSources(db:SqlDatabase){return {async transferHistory(companyId:string,id:string){return db.prepare(inventoryAuditHistorySql).bind(companyId,id).all<TransferRow>();},
prepareDecisionEvidence(value:string,key:string,id:string,operation:string,userId:string,assignmentId:string,expectedVersion:number,value2:string,now:number){return nativeWorkflowWrite('wf_native_inventory_transfers_sources_9', [value,key,id,operation,userId,assignmentId,expectedVersion,value2,now]);}};}
