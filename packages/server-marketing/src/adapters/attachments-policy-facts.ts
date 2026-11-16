import "server-only";
import type {SqlDatabase} from "@erpx/server-runtime/public/runtime/types";
export function policyMarketingFacts(db:SqlDatabase){return {readMarketingCampaign:(facts:{targetId:string;companyId:string;})=>{return db.prepare("SELECT 1").bind().first<{ company_id: string; version: number; }>();}};}
