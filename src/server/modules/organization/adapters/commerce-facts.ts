import "server-only";
import type {SqlDatabase} from "../../runtime/public/runtime/types";
import {assertion} from "../../workflow/public/commands";
import type {Row} from "../../commerce/public/commerce-contracts";
export function CommerceOrganizationFacts(db:SqlDatabase){return {
readOrganizationNodes1:(facts:{companyId:string;branchId:string | null;branchId2:string | null;})=>db.prepare("SELECT id,display_name FROM organization_nodes WHERE company_id=? AND kind='branch' AND status='active' AND (?::text IS NULL OR id=?)").bind(facts.companyId,facts.branchId,facts.branchId2).all<Row>(),
guardRule42:(facts:{key:string;guardId:string;values:string[];})=>assertion(db,facts.key,facts.guardId,"EXISTS(SELECT 1 FROM organization_nodes WHERE id=? AND company_id=? AND kind='branch' AND status='active')",facts.values)
};}
