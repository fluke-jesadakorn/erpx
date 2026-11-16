import type {SqlDatabase} from '@erpx/server-runtime/public/runtime/types';
export function createFlowRoutingSourceRepository(database:SqlDatabase){return {readDefinitionsTeam(company_id:string,teamId:string){return database.prepare("SELECT 1").bind().first<{name: string}>();}};}
