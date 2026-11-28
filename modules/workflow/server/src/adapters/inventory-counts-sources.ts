import {nativeWorkflowReadAll} from './native-workflow-read';
import {nativeWorkflowWrite} from './native-workflow-write';
import type {SqlDatabase} from '@erpx/platform-runtime-server/public/runtime/types';import type {CountRow} from '@erpx/inventory-server/public/count-contract';
export function inventoryCountSources(db:SqlDatabase){return {async countHistory(companyId:string,id:string){return {results:await nativeWorkflowReadAll<CountRow>('wf_native_inventory_audit_history',[companyId,id],companyId,db)};},
prepareDecisionEvidence(value:string,key:string,id:string,operation:string,userId:string,assignmentId:string,expectedVersion:number,value2:string,now:number){return nativeWorkflowWrite('wf_native_inventory_counts_sources_8', [value,key,id,operation,userId,assignmentId,expectedVersion,value2,now]);}};}
