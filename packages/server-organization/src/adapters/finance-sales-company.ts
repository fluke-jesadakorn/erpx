import type { SqlDatabase } from "@erpx/server-runtime/public/runtime/types";
import {ORG_COMPANY_DISPLAY_NAME_SOURCE} from './org-sources';
export async function financeSalesCompanyName(db: SqlDatabase, companyId: string) {
  return await db.prepare(ORG_COMPANY_DISPLAY_NAME_SOURCE).bind(companyId).first<string>("display_name") ?? companyId;
}
