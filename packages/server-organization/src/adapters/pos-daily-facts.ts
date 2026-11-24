import "server-only";
import {one} from '@erpx/server-runtime/public/runtime/sql';
import type {SqlDatabase} from "@erpx/server-runtime/public/runtime/types";
import {nodeKindByIdCompany} from './organization-column-alias-families-r27';
export function PosDayOrganizationFacts(db:SqlDatabase){return {
readOrganizationNodes1:(facts:{branch:string;companyId:string;})=>one(db,nodeKindByIdCompany('id','branch'),facts.branch,facts.companyId)
};}
