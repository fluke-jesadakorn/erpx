import type {SqlDatabase} from '@erpx/server-runtime/public/runtime/types';export function peoplePlacementBatchFacts(database:SqlDatabase){return {activePositions(companyId:string){return database.prepare("SELECT id FROM organization_positions WHERE company_id=? AND active=1")
    .bind(companyId).all<{id:string}>();}};}
