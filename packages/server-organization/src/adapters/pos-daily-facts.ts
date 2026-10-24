import "server-only";
import {one} from '@erpx/server-runtime/public/runtime/sql';
import type {SqlDatabase} from "@erpx/server-runtime/public/runtime/types";
export function PosDayOrganizationFacts(db:SqlDatabase){return {
readOrganizationNodes1:(facts:{branch:string;companyId:string;})=>one(db,"SELECT id FROM organization_nodes WHERE id=? AND company_id=? AND kind='branch'",facts.branch,facts.companyId)
};}
