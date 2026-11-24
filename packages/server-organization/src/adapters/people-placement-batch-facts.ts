import type {SqlDatabase} from '@erpx/server-runtime/public/runtime/types';import {activeIdByCompanySource} from './org-sources';export function peoplePlacementBatchFacts(database:SqlDatabase){return {activePositions(companyId:string){return database.prepare(activeIdByCompanySource('organization_positions'))
    .bind(companyId).all<{id:string}>();}};}
