import type {SqlDatabase} from '@erpx/platform-runtime-server/public/runtime/types';
import {nativeOperationsReadFirst,OPERATIONS_RD_FLOW_DEFINITIONS_DELEGATION} from './native-operations-read';
/** Owner-native read: the statement is planned by the Operations native read
 * port (`RD_FLOW_DEFINITIONS_DELEGATION`) and executed on the caller's open
 * unit. This module carries no SQL for this read. */
export function createFlowRoutingSourceRepository(database:SqlDatabase){return {readDefinitionsDelegation(company_id:string,id:string | number | null):Promise<{delegate_user_id:string}|null>{
 return nativeOperationsReadFirst<{delegate_user_id:string}>(OPERATIONS_RD_FLOW_DEFINITIONS_DELEGATION,[company_id,id],database);
}};}
