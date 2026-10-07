import "server-only";
import {one,all} from "../../pos/public/pos-core";
import type {SqlDatabase} from "../../runtime/public/runtime/types";
export function PosReadOrganizationFacts(db:SqlDatabase){return {
readOrganizationNodes1:(facts:{company:string;})=>one(db,'SELECT display_name FROM organization_nodes WHERE id=?',facts.company),
readOrganizationNodes2:(facts:{company:string;branch:string | null;branch2:string | null;})=>all(db,"SELECT id,display_name name FROM organization_nodes WHERE company_id=? AND kind='branch' AND (?::text IS NULL OR id=?) ORDER BY display_name",facts.company,facts.branch,facts.branch2)
};}
