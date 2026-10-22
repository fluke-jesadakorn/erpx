import "server-only";
import type {SqlDatabase} from "@erpx/server-runtime/public/runtime/types";
export function workReportPeoplePayrollFacts(db:SqlDatabase){return {people:(companyId:string)=>db.prepare("SELECT user_id,legal_name FROM hr_people WHERE company_id=? AND merged_into_id IS NULL").bind(companyId).all<{user_id:string;legal_name:string}>()};}
