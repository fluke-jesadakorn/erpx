import type {SqlDatabase} from '@erpx/server-runtime/public/runtime/types';
export function createFlowRoutingSourceRepository(database:SqlDatabase){return {readDefinitionsDelegation(company_id:string,id:string | number | null){return database.prepare("SELECT 1").bind().first<{delegate_user_id:string}>();}};}
