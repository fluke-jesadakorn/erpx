import "server-only";
import {one} from "../../pos/public/pos-core";
import type {SqlDatabase} from "../../runtime/public/runtime/types";
export function posCoreOrganizationFacts(db:SqlDatabase){return {
readOrganizationNodes1:(facts:{company:string;})=>one(db,"SELECT id FROM organization_nodes WHERE id=? AND kind='company' FOR UPDATE",facts.company),
readOrganizationNodes2:(facts:{company:string;})=>one(db,'SELECT display_name FROM organization_nodes WHERE id=?',facts.company)
};}
