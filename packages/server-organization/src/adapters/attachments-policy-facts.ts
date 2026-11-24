import "server-only";
import type {SqlDatabase} from "@erpx/server-runtime/public/runtime/types";
import {NODE_BRANCH_ACTIVE} from './organization-read-tail-sources';
export function policyOrganizationFacts(db:SqlDatabase){return {readBranchBrand:(facts:{targetId:string;companyId:string;})=>{return db.prepare(NODE_BRANCH_ACTIVE).bind(facts.targetId,facts.companyId).first<Record<string, unknown>>();}};}
