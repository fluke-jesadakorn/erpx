import {inventoryAuditHistorySql} from './inventory-audit-history-source';
import {nativeWorkflowWrite} from './native-workflow-write';
import type {SqlDatabase} from '@erpx/server-runtime/public/runtime/types';import type {CountRow} from '@erpx/server-inventory/public/count-contract';
export function inventoryCountSources(db:SqlDatabase){return {async countHistory(companyId:string,id:string){return db.prepare(inventoryAuditHistorySql).bind(companyId,id).all<CountRow>();},
prepareDecisionEvidence(value:string,key:string,id:string,operation:string,userId:string,assignmentId:string,expectedVersion:number,value2:string,now:number){return nativeWorkflowWrite('wf_native_inventory_counts_sources_8', [value,key,id,operation,userId,assignmentId,expectedVersion,value2,now]);}};}
