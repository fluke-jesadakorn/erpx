import { platformOwnerProjectionSources as commercePlatformSources } from "../../commerce/public/platform-owner-projection-sources";
import type { SqlDatabase } from "@/server/modules/runtime/public/runtime/types";
import "server-only";
import { type Row } from "@/server/modules/commerce/public/commerce";
export function RecordDetailsOrganizationFacts(database: SqlDatabase) { return { readCommerceMasterDetailRow3(id: string, companyId: string, branchId: string | null, branchId2: string | null) { return database.prepare(`SELECT w.*,b.display_name branch_name FROM (${commercePlatformSources.commerce_warehouses}) w JOIN organization_nodes b ON b.id=w.branch_id WHERE w.id=? AND w.company_id=? AND (?::text IS NULL OR w.branch_id=?)`).bind(id, companyId, branchId, branchId2).first<Row>(); } }; }
