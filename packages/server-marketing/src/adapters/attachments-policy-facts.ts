import "server-only";
import type {SqlDatabase} from "@erpx/server-runtime/public/runtime/types";
export function policyMarketingFacts(db:SqlDatabase){return {readMarketingCampaign:(facts:{targetId:string;companyId:string;})=>{return db.prepare("SELECT company_id,version FROM marketing_campaigns WHERE id=? AND company_id=?").bind(facts.targetId,facts.companyId).first<{ company_id: string; version: number; }>();}};}
