import "server-only";
import {one} from '@erpx/server-runtime/public/runtime/sql';
import type {SqlDatabase} from "@erpx/server-runtime/public/runtime/types";
import {ORG_NODE_DISPLAY_NAME_SOURCE} from './org-sources';
export function posCoreOrganizationFacts(db:SqlDatabase){return {
readOrganizationNodes1:(facts:{company:string;})=>one(db,"SELECT id FROM organization_nodes WHERE id=? AND kind='company' FOR UPDATE",facts.company),
readOrganizationNodes2:(facts:{company:string;})=>one(db,ORG_NODE_DISPLAY_NAME_SOURCE,facts.company)
};}
