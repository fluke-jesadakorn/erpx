import type { SqlDatabase } from "@erpx/server-runtime/public/runtime/types";
export async function financeSalesCompanyName(db: SqlDatabase, companyId: string) {
  return await db.prepare("SELECT display_name FROM organization_nodes WHERE id=? AND kind='company'").bind(companyId).first<string>("display_name") ?? companyId;
}
