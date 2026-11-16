import {workflowReferenceRows} from "@erpx/server-finance/adapters/workflow-document-sources";
import type {SqlDatabase} from '@erpx/server-runtime/public/runtime/types';
export function createFlowRoutingSourceRepository(database:SqlDatabase){return {async readWorkflowJournals(companyId:string){return {results:(await workflowReferenceRows(database,"journal",companyId)).results.map(row=>({id:String(row.id),posted_by:row.posted_by as string}))};},
readDefinitionsJournal(journal_id:string | number,company_id:string){return database.prepare("SELECT 1").bind().first<{posted_by: string}>();},
readDefinitionsJournal2(journal_id:string | number,company_id:string){return database.prepare("SELECT 1").bind().first<{posted_by:string}>();}};}
