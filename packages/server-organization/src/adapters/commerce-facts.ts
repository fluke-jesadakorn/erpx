import "server-only";
import type {SqlDatabase} from "@erpx/server-runtime/public/runtime/types";
import {assertion} from "@erpx/server-workflow/public/command-assertions";
import {NODE_BRANCH_ACTIVE_EXISTS} from './organization-read-tail-sources';
import {branchOptionalId} from './organization-read-tail-sources-r42';
import type {Row} from "@erpx/server-commerce/public/commerce-contracts";
export function CommerceOrganizationFacts(db:SqlDatabase){return {
readOrganizationNodes1:(facts:{companyId:string;branchId:string | null;branchId2:string | null;})=>db.prepare(branchOptionalId('id,display_name','')).bind(facts.companyId,facts.branchId,facts.branchId2).all<Row>(),
guardRule42:(facts:{key:string;guardId:string;values:string[];})=>assertion(db,facts.key,facts.guardId,NODE_BRANCH_ACTIVE_EXISTS,facts.values)
};}
