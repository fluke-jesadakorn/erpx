import "server-only";
import type {SqlDatabase} from "@erpx/server-runtime/public/runtime/types";
import {salesRecipientEligibility} from "@erpx/server-sales/public/sales-recipient";
import {crmAccountCreatedByScopeSql} from "@erpx/server-crm/adapters/purchases";
import {crmOpportunityOwnerReadSql} from "./read-tail-r27-sql";
import {crmAccountPolicyReadSql} from "./read-tail-r29-sql";
export function policyCrmFacts(db:SqlDatabase){return {readCrmAccount:(facts:{targetId:string;companyId:string;branchId:string | null;userId:string;departmentId:string | null;})=>{return db.prepare(crmAccountPolicyReadSql(`${crmAccountCreatedByScopeSql}`)).bind(facts.targetId,facts.companyId,facts.branchId,facts.userId,facts.branchId,facts.departmentId,facts.departmentId).first<{ company_id: string; version: number; }>();},
readCrmOpportunity:(facts:{targetId:string;companyId:string;branchId:string | null;departmentId:string | null;})=>{const ownerEligible=salesRecipientEligibility("l","l.owner_id");return db.prepare(crmOpportunityOwnerReadSql(ownerEligible)).bind(facts.targetId,facts.companyId,facts.branchId,facts.branchId,facts.departmentId,facts.departmentId).first<{ company_id: string; branch_id: string; department_id: string | null; owner_id: string; version: number; owner_eligible: number; }>();}};}
