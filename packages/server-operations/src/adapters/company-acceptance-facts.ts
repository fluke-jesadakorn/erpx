import {operationsActiveStaffLevels} from './staff-facts';
import type {SqlDatabase} from "@erpx/server-runtime/public/runtime/types";
export function companyAcceptanceFacts(db:SqlDatabase){return {
async levels(companyId:string){return db.prepare(operationsActiveStaffLevels('count(*) count',' AND level_number BETWEEN 1 AND 6')).bind(companyId).first<{ count: number }>();}
};}
