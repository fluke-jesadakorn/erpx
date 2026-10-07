import type {SqlDatabase} from '../../runtime/public/runtime/types';
export function createFlowRoutingSourceRepository(database:SqlDatabase){return {readDefinitionsTeam(company_id:string,teamId:string){return database.prepare("SELECT name FROM service_teams WHERE company_id=? AND id=?").bind(company_id, teamId).first<{name: string}>();}};}
