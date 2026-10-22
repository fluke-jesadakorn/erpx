import "server-only";
import type { WorkContext } from "@erpx/server-identity_access/public/authorization";
import { requirePermission } from "@erpx/server-identity_access/public/iam-permissions";
import { holdingResources } from "../../../../packages/server-runtime/src/public/runtime";
import { ApiError } from "@erpx/server-runtime/public/http";
import { parseGuestAccountJson } from "@erpx/feature-identity_access/lib/guest-account-compat";
import type { GuestAccount, Entry } from "@erpx/server-pms/domain/pms-model";

/** Financial evidence stays scoped to one account and its originating entries. */
export async function guestAccountFinanceEvidence(actor: WorkContext, id: string) {
  requirePermission(actor,"finance.read"); requirePermission(actor,"pms.reports");
  if(actor.departmentId)throw new ApiError(403,"PMS_BRANCH_REQUIRED","Choose a Company or Branch.");
  const db=(await holdingResources(actor.holdingId)).write;
  const account=await db.prepare(`SELECT r.id,r.property_id,r.data_json,p.name,p.business_date,p.config_json::jsonb->>'currency' currency
    FROM pms_records r JOIN pms_properties p ON p.id=r.property_id WHERE r.id=? AND r.kind='guest_account'
    AND p.company_id=? AND (?::text IS NULL OR p.branch_id=?)`)
    .bind(id,actor.companyId,actor.branchId,actor.branchId).first<{id:string;property_id:string;data_json:string;name:string;business_date:string;currency:string}>();
  if(!account)throw new ApiError(404,"PMS_NOT_FOUND","Guest Account not found in this workspace.");
  const entries=(await db.prepare(`SELECT e.id,e.data_json,pp.journal_id FROM pms_records e LEFT JOIN pms_postings pp ON pp.property_id=e.property_id AND pp.record_id=e.id
    WHERE e.property_id=? AND e.kind='entry' AND e.data_json::jsonb->>'guestAccountId'=? ORDER BY e.updated_at,e.id`)
    .bind(account.property_id,id).all<{id:string;data_json:string;journal_id:string|null}>()).results;
  return {id:account.id,propertyId:account.property_id,hotel:account.name,businessDate:account.business_date,currency:account.currency,
    account:parseGuestAccountJson<GuestAccount>(account.data_json),entries:entries.map(row=>({id:row.id,journalId:row.journal_id,data:parseGuestAccountJson<Entry>(row.data_json)}))};
}
