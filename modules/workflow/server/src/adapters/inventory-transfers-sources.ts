import {nativeWorkflowReadAll} from './native-workflow-read';
import {nativeWorkflowWrite} from './native-workflow-write';
import type {SqlDatabase} from '@erpx/platform-runtime-server/public/runtime/types';import type {TransferRow} from '@erpx/inventory-server/public/transfer-contract';
export function inventoryTransferSources(db:SqlDatabase){return {async transferHistory(companyId:string,id:string){return {results:await nativeWorkflowReadAll<TransferRow>('wf_native_inventory_audit_history',[companyId,id],companyId,db)};},
prepareDecisionEvidence(value:string,key:string,id:string,operation:string,userId:string,assignmentId:string,expectedVersion:number,value2:string,now:number){return nativeWorkflowWrite('wf_native_inventory_transfers_sources_9', [value,key,id,operation,userId,assignmentId,expectedVersion,value2,now]);}};}
