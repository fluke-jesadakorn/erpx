import "server-only";
import type {SqlDatabase} from "@erpx/server-runtime/public/runtime/types";
export function policyOrganizationFacts(db:SqlDatabase){return {readBranchBrand:(facts:{targetId:string;companyId:string;})=>{return db.prepare("SELECT 1 FROM organization_nodes WHERE id=? AND company_id=? AND kind='branch' AND status='active'").bind(facts.targetId,facts.companyId).first<Record<string, unknown>>();}};}
