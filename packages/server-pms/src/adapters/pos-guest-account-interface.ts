import "server-only";
import type { SqlDatabase } from "@erpx/server-runtime/public/runtime/types";
import { ApiError } from "@erpx/server-runtime/public/http";

/** PMS owns eligibility and returns only the facts needed to select a stay. */
export function posGuestAccountInterface(db: SqlDatabase) {
  return {
    async hotels(companyId: string, branchId: string | null) {
      return (await db.prepare("SELECT id,name,branch_id FROM pms_properties WHERE company_id=? AND (?::text IS NULL OR branch_id=?) ORDER BY name,id LIMIT 100").bind(companyId,branchId,branchId).all<Record<string,string|number|boolean|null>>()).results;
    },
    async choices(companyId: string, branchId: unknown, currency: unknown, propertyId: unknown) {
      return (await db.prepare(`SELECT p.id property_id,p.version property_version,s.id stay_id,
        r.data_json::jsonb->>'number' room_number,g.data_json::jsonb->>'name' guest_name
        FROM pms_properties p JOIN pms_records s ON s.property_id=p.id AND s.kind='stay'
        JOIN pms_records a ON a.id=s.data_json::jsonb->>'guestAccountId' AND a.property_id=p.id AND a.kind='guest_account'
        JOIN pms_records r ON r.id=s.data_json::jsonb->>'roomId' AND r.property_id=p.id AND r.kind='room'
        JOIN pms_records g ON g.id=s.data_json::jsonb->>'guestId' AND g.property_id=p.id AND g.kind='guest'
        WHERE p.company_id=? AND p.branch_id=? AND p.id=? AND p.state='open' AND p.config_json::jsonb->>'currency'=?
        AND s.data_json::jsonb->>'state'='in_house' AND a.data_json::jsonb->>'state'='open'
        AND COALESCE(a.data_json::jsonb->>'invoiceId','')='' ORDER BY r.data_json::jsonb->>'number',s.id LIMIT 100`)
        .bind(companyId,branchId,propertyId,currency).all<Record<string, string | number | boolean | null>>()).results;
    },
    async assertDay(companyId: string, propertyId: unknown, branchId: unknown, date: string) {
      if (!propertyId) return;
      const row = await db.prepare("SELECT state,business_date FROM pms_properties WHERE id=? AND company_id=? AND branch_id=? FOR SHARE")
        .bind(propertyId,companyId,branchId).first<{state:string;business_date:string}>();
      if (!row || row.state !== "open" || row.business_date !== date)
        throw new ApiError(409,"POS_HOTEL_DAY_CLOSED","Finish or recover the linked hotel day before selling.");
    },
  };
}
