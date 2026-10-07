import "server-only";
import {one} from "../../pos/public/pos-core";
import type {SqlDatabase} from "../../runtime/public/runtime/types";
export function PosDayOrganizationFacts(db:SqlDatabase){return {
readOrganizationNodes1:(facts:{branch:string;companyId:string;})=>one(db,"SELECT id FROM organization_nodes WHERE id=? AND company_id=? AND kind='branch'",facts.branch,facts.companyId)
};}
