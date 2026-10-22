import "server-only";
import type {SqlDatabase} from "@erpx/server-runtime/public/runtime/types";
import {salesRecipientEligibility} from "@erpx/server-sales/public/sales-recipient";
export function policyCrmFacts(db:SqlDatabase){return {readCrmAccount:(facts:{targetId:string;companyId:string;branchId:string | null;userId:string;departmentId:string | null;})=>{return db.prepare(`SELECT account.company_id,account.version
    FROM crm_accounts account
    WHERE account.id=? AND account.company_id=?
      AND (?::text IS NULL OR account.created_by=? OR EXISTS(
        SELECT 1 FROM crm_leads lead
        WHERE lead.account_id=account.id AND lead.company_id=account.company_id
          AND lead.branch_id=? AND (?::text IS NULL OR lead.department_id=?)
      ))`).bind(facts.targetId,facts.companyId,facts.branchId,facts.userId,facts.branchId,facts.departmentId,facts.departmentId).first<{ company_id: string; version: number; }>();},
readCrmOpportunity:(facts:{targetId:string;companyId:string;branchId:string | null;departmentId:string | null;})=>{const ownerEligible=salesRecipientEligibility("l","l.owner_id");return db.prepare(`SELECT l.company_id,l.branch_id,l.department_id,l.owner_id,o.version,
      CASE WHEN ${ownerEligible} THEN 1 ELSE 0 END owner_eligible
    FROM crm_opportunities o JOIN crm_leads l ON l.id=o.lead_id
    WHERE o.id=? AND l.company_id=?
      AND (?::text IS NULL OR l.branch_id=?)
      AND (?::text IS NULL OR l.department_id=?)`).bind(facts.targetId,facts.companyId,facts.branchId,facts.branchId,facts.departmentId,facts.departmentId).first<{ company_id: string; branch_id: string; department_id: string | null; owner_id: string; version: number; owner_eligible: number; }>();}};}
