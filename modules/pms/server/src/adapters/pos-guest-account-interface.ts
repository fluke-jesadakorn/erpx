import "server-only";
import type { SqlDatabase } from "@erpx/platform-runtime-server/public/runtime/types";
import { ApiError } from "@erpx/platform-runtime-server/public/http";
import { nativePmsReadAll, PMS_RD_POS_GUEST_HOTELS, PMS_RD_POS_GUEST_CHOICES } from "./native-pms-read";

/** PMS owns eligibility and returns only the facts needed to select a stay. */
export function posGuestAccountInterface(db: SqlDatabase) {
  return {
    async hotels(companyId: string, branchId: string | null) {
      return (await nativePmsReadAll<Record<string,string|number|boolean|null>>(PMS_RD_POS_GUEST_HOTELS,[companyId,branchId,branchId],db)).results;
    },
    async choices(companyId: string, branchId: unknown, currency: unknown, propertyId: unknown) {
      return (await nativePmsReadAll<Record<string, string | number | boolean | null>>(PMS_RD_POS_GUEST_CHOICES,[companyId,branchId,propertyId,currency],db)).results;
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
