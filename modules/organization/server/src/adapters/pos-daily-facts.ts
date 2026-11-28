import "server-only";
import {one} from '@erpx/platform-runtime-server/public/runtime/sql';
import type {SqlDatabase} from "@erpx/platform-runtime-server/public/runtime/types";
import {nodeKindByIdCompany} from './organization-column-alias-families-r27';
export function PosDayOrganizationFacts(db:SqlDatabase){return {
readOrganizationNodes1:(facts:{branch:string;companyId:string;})=>one(db,nodeKindByIdCompany('id','branch'),facts.branch,facts.companyId)
};}
