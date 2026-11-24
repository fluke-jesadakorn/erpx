import "server-only";
import {one,all} from '@erpx/server-runtime/public/runtime/sql';
import type {SqlDatabase} from "@erpx/server-runtime/public/runtime/types";
import {ORG_NODE_DISPLAY_NAME_SOURCE} from './org-sources';
export function PosReadOrganizationFacts(db:SqlDatabase){return {
readOrganizationNodes1:(facts:{company:string;})=>one(db,ORG_NODE_DISPLAY_NAME_SOURCE,facts.company),
readOrganizationNodes2:(facts:{company:string;branch:string | null;branch2:string | null;})=>all(db,"SELECT id,display_name name FROM organization_nodes WHERE company_id=? AND kind='branch' AND (?::text IS NULL OR id=?) ORDER BY display_name",facts.company,facts.branch,facts.branch2)
};}
