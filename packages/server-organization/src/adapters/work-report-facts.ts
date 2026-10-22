import "server-only";
import type {SqlDatabase} from "@erpx/server-runtime/public/runtime/types";
export function workReportOrganizationFacts(db:SqlDatabase){return {nodes:(companyId:string)=>db.prepare("SELECT id,display_name FROM organization_nodes WHERE company_id=?").bind(companyId).all<{id:string;display_name:string}>()};}
