import type {SqlDatabase} from '../../runtime/public/runtime/types';
export function createFlowRoutingSourceRepository(database:SqlDatabase){return {readDefinitionsJournal(journal_id:string | number,company_id:string){return database.prepare("SELECT posted_by FROM finance_journals WHERE id=? AND company_id=?").bind(journal_id, company_id).first<{posted_by: string}>();},
readDefinitionsJournal2(journal_id:string | number,company_id:string){return database.prepare("SELECT posted_by FROM finance_journals WHERE id=? AND company_id=?").bind(journal_id,company_id).first<{posted_by:string}>();}};}
