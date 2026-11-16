import "server-only";
import type {SqlDatabase} from "@erpx/server-runtime/public/runtime/types";
export function policyOrganizationFacts(db:SqlDatabase){return {readBranchBrand:(facts:{targetId:string;companyId:string;})=>{return db.prepare("SELECT 1").bind().first<Record<string, unknown>>();}};}
